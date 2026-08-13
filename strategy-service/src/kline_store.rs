//! 内存K线存储模块
//!
//! 策略服务自主管理K线数据，不依赖 trading-core/Redis。
//! - KlineBar: 单根K线数据
//! - KlineStore: 单个 (symbol, timeframe) 的滚动窗口
//! - KlineManager: 全局管理所有 Store

use std::collections::{HashMap, VecDeque};
use chrono::{DateTime, Utc};
use tracing::info;

use crate::redis_reader::Timeframe;

/// 单根K线数据
#[derive(Debug, Clone)]
pub struct KlineBar {
    pub open_time: i64,     // 毫秒时间戳
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub closed: bool,       // 是否已完成
}

impl KlineBar {
    /// 从 redis_reader::KlineData 转换（已关闭的K线）
    pub fn from_kline_data(k: &crate::redis_reader::KlineData) -> Self {
        KlineBar {
            open_time: k.timestamp,
            open: k.open,
            high: k.high,
            low: k.low,
            close: k.close,
            volume: k.volume,
            closed: true,
        }
    }

    /// 转换为 redis_reader::KlineData（供策略层使用）
    pub fn to_kline_data(&self) -> crate::redis_reader::KlineData {
        crate::redis_reader::KlineData {
            timestamp: self.open_time,
            open: self.open,
            high: self.high,
            low: self.low,
            close: self.close,
            volume: self.volume,
            trade_count: 0,
        }
    }
}

/// 单个 (symbol, timeframe, market_type) 的K线滚动窗口
pub struct KlineStore {
    pub symbol: String,
    pub timeframe: Timeframe,
    pub market_type: String,
    bars: VecDeque<KlineBar>,
    max_size: usize,
    last_update: DateTime<Utc>,
}

impl KlineStore {
    pub fn new(symbol: String, timeframe: Timeframe, market_type: String, max_size: usize) -> Self {
        KlineStore {
            symbol,
            timeframe,
            market_type,
            bars: VecDeque::with_capacity(max_size),
            max_size,
            last_update: Utc::now(),
        }
    }

    /// 批量追加已完成的K线（启动时加载）
    pub fn extend_closed(&mut self, bars: Vec<KlineBar>) {
        for bar in bars {
            debug_assert!(bar.closed, "extend_closed only accepts closed bars");
            self.bars.push_back(bar);
        }
        // 截断到最大容量
        while self.bars.len() > self.max_size {
            self.bars.pop_front();
        }
        self.last_update = Utc::now();
    }

    /// 追加单根已完成的K线（前端截断）
    pub fn push_closed(&mut self, bar: KlineBar) {
        debug_assert!(bar.closed, "push_closed only accepts closed bars");
        self.bars.push_back(bar);
        if self.bars.len() > self.max_size {
            self.bars.pop_front();
        }
        self.last_update = Utc::now();
    }

    /// 更新当前未完成的K线（最后一根）
    pub fn update_current(&mut self, bar: KlineBar) {
        debug_assert!(!bar.closed, "update_current only accepts non-closed bars");
        if let Some(last) = self.bars.back_mut() {
            if !last.closed {
                *last = bar;  // 覆盖未完成的
            } else {
                self.bars.push_back(bar);  // 新的一根未完成
            }
        } else {
            self.bars.push_back(bar);
        }
        self.last_update = Utc::now();
    }

    /// 获取最近 N 根已完成K线（供策略计算）
    pub fn closed_bars(&self, n: usize) -> Vec<&KlineBar> {
        let closed: Vec<&KlineBar> = self.bars.iter().filter(|b| b.closed).collect();
        let len = closed.len();
        if len <= n {
            closed
        } else {
            closed[len - n..].to_vec()
        }
    }

    /// 获取所有已完成K线的引用
    pub fn all_closed(&self) -> Vec<&KlineBar> {
        self.bars.iter().filter(|b| b.closed).collect()
    }

    /// 检查是否有足够数据
    pub fn has_enough(&self, required: usize) -> bool {
        self.bars.iter().filter(|b| b.closed).count() >= required
    }

    /// 最新已完成K线的时间戳（毫秒）
    pub fn latest_closed_time(&self) -> Option<i64> {
        self.bars.iter().rev().find(|b| b.closed).map(|b| b.open_time)
    }

    /// 已完成K线数量
    pub fn closed_count(&self) -> usize {
        self.bars.iter().filter(|b| b.closed).count()
    }

    /// 当前价格（最后一根K线的收盘价）
    pub fn current_price(&self) -> f64 {
        self.bars.back().map(|b| b.close).unwrap_or(0.0)
    }

    /// 最后更新时间
    pub fn last_update(&self) -> DateTime<Utc> {
        self.last_update
    }

    /// 是否有数据
    pub fn is_empty(&self) -> bool {
        self.bars.is_empty()
    }

    /// 时间框架的毫秒持续时间
    pub fn timeframe_duration_ms(&self) -> i64 {
        self.timeframe.as_duration().num_milliseconds()
    }
}

/// 全局K线管理器
pub struct KlineManager {
    stores: HashMap<(String, Timeframe, String), KlineStore>,
    max_bars: usize,
}

impl KlineManager {
    pub fn new(max_bars: usize) -> Self {
        KlineManager {
            stores: HashMap::new(),
            max_bars,
        }
    }

    /// 获取指定 symbol+timeframe+market_type 的 store
    pub fn get(&self, symbol: &str, tf: Timeframe, market_type: &str) -> Option<&KlineStore> {
        self.stores.get(&(symbol.to_string(), tf, market_type.to_string()))
    }

    /// 获取指定 symbol+timeframe+market_type 的 store（可变引用）
    pub fn get_mut(&mut self, symbol: &str, tf: Timeframe, market_type: &str) -> Option<&mut KlineStore> {
        self.stores.get_mut(&(symbol.to_string(), tf, market_type.to_string()))
    }

    /// 启动时创建所有需要的 store
    pub fn init_stores(&mut self, pairs: &[(String, Timeframe, String)]) {
        for (symbol, tf, market_type) in pairs {
            let key = (symbol.clone(), *tf, market_type.clone());
            if !self.stores.contains_key(&key) {
                info!("[KlineManager] Creating store for {} {} ({})", symbol, tf.as_str(), market_type);
                self.stores.insert(
                    key,
                    KlineStore::new(symbol.clone(), *tf, market_type.clone(), self.max_bars),
                );
            }
        }
    }

    /// 获取所有 store 的键
    pub fn keys(&self) -> Vec<(String, Timeframe, String)> {
        self.stores.keys().cloned().collect()
    }

    /// 移除指定的 store
    pub fn remove(&mut self, symbol: &str, tf: Timeframe, market_type: &str) -> bool {
        self.stores.remove(&(symbol.to_string(), tf, market_type.to_string())).is_some()
    }

    /// 获取全局 max_bars 配置
    pub fn max_bars(&self) -> usize {
        self.max_bars
    }
}
