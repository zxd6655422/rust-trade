//! 混合K线加载器
//!
//! 启动时从 DB 加载历史数据，从交易所补最新缺口。
//! 支持：
//! - PostgreSQL 加载（kline_1m ~ kline_1w）
//! - Binance REST API 补拉/全量加载
//! - 自动检测缺口并填补

use std::sync::Arc;
use anyhow::{anyhow, Result};
use chrono::Utc;
use reqwest::Client;
use sqlx::PgPool;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::kline_store::{KlineBar, KlineManager};
use crate::redis_reader::Timeframe;

/// Binance REST API 基础 URL
const BINANCE_FUTURES_BASE: &str = "https://fapi.binance.com";
const BINANCE_SPOT_BASE: &str = "https://api.binance.com";

/// Binance kline 响应中的字段索引
const KLINE_OPEN_TIME: usize = 0;
const KLINE_OPEN: usize = 1;
const KLINE_HIGH: usize = 2;
const KLINE_LOW: usize = 3;
const KLINE_CLOSE: usize = 4;
const KLINE_VOLUME: usize = 5;

/// 从 Binance REST API 获取K线数据
///
/// # 参数
/// - `symbol`: 交易对（如 "BTCUSDT"）
/// - `tf`: 时间框架（如 30m / 5m），用于拼 URL 和判断 K 线是否已收盘
/// - `limit`: 获取数量（最大 1000）
/// - `end_time`: 结束时间戳（毫秒），None 表示最新
/// - `market_type`: "spot" 或 "futures"
pub async fn fetch_klines_from_exchange(
    symbol: &str,
    tf: Timeframe,
    limit: usize,
    end_time: Option<i64>,
    market_type: &str,
) -> Result<Vec<KlineBar>> {
    let base_url = match market_type {
        "spot" => BINANCE_SPOT_BASE,
        _ => BINANCE_FUTURES_BASE,
    };

    let path = match market_type {
        "spot" => "/api/v3/klines",
        _ => "/fapi/v1/klines",
    };

    let mut url = format!(
        "{}{}?symbol={}&interval={}&limit={}",
        base_url, path, symbol, tf.as_str(), limit.min(1000)
    );

    if let Some(end) = end_time {
        url.push_str(&format!("&endTime={}", end));
    }

    let client = Client::new();
    let resp = client.get(&url).send().await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("Binance klines API error {}: {}", status, body));
    }

    let raw: Vec<Vec<serde_json::Value>> = resp.json().await?;

    // 按时间戳判断 K 线是否已收盘：open_time + 周期 <= 当前时间 → 已收盘；
    // 否则为当前时间框架下正在形成的 K 线（closed=false），由下游走 update_current。
    let now_ms = Utc::now().timestamp_millis();
    let duration_ms = tf.as_duration().num_milliseconds();

    let bars: Vec<KlineBar> = raw
        .iter()
        .filter_map(|row| {
            let open_time = row.get(KLINE_OPEN_TIME)?.as_i64()?;
            let open = row.get(KLINE_OPEN)?.as_str()?.parse::<f64>().ok()?;
            let high = row.get(KLINE_HIGH)?.as_str()?.parse::<f64>().ok()?;
            let low = row.get(KLINE_LOW)?.as_str()?.parse::<f64>().ok()?;
            let close = row.get(KLINE_CLOSE)?.as_str()?.parse::<f64>().ok()?;
            let volume = row.get(KLINE_VOLUME)?.as_str()?.parse::<f64>().ok()?;

            Some(KlineBar {
                open_time,
                open,
                high,
                low,
                close,
                volume,
                closed: open_time + duration_ms <= now_ms,
            })
        })
        .collect();

    Ok(bars)
}

/// 填补缺口：从交易所补拉指定时间范围的K线
pub async fn fill_gap_from_exchange(
    symbol: &str,
    tf: Timeframe,
    after_time: i64,
    needed: usize,
    market_type: &str,
) -> Result<Vec<KlineBar>> {
    info!(
        "[KlineLoader] Filling gap for {} {} after {} (need {} bars)",
        symbol,
        tf.as_str(),
        after_time,
        needed
    );

    let bars = fetch_klines_from_exchange(
        symbol,
        tf,
        needed.min(1000),
        None, // 不指定 endTime，获取最新的
        market_type,
    )
    .await?;

    // 只保留 after_time 之后的K线
    let filtered: Vec<KlineBar> = bars
        .into_iter()
        .filter(|b| b.open_time > after_time)
        .collect();

    info!(
        "[KlineLoader] Gap fill: got {} bars for {} {}",
        filtered.len(),
        symbol,
        tf.as_str()
    );

    Ok(filtered)
}

/// 从交易所全量加载（DB 无数据时的兜底方案）
///
/// 分页获取，直到达到 required 数量或没有更多数据
pub async fn load_full_from_exchange(
    symbol: &str,
    tf: Timeframe,
    required: usize,
    market_type: &str,
) -> Result<Vec<KlineBar>> {
    info!(
        "[KlineLoader] Full load from exchange: {} {} (need {} bars)",
        symbol,
        tf.as_str(),
        required
    );

    let mut all = Vec::new();
    let mut end_time: Option<i64> = None;

    while all.len() < required {
        let limit = (required - all.len()).min(1000);
        let batch = fetch_klines_from_exchange(
            symbol,
            tf,
            limit,
            end_time,
            market_type,
        )
        .await?;

        if batch.is_empty() {
            break;
        }

        // 设置下一次查询的 endTime（当前批次最早时间 - 1ms）
        end_time = Some(batch.first().unwrap().open_time - 1);
        // 插入到前面（因为分页是倒序获取的）
        all.splice(0..0, batch);
    }

    // 只保留需要的数量（最新的）
    if all.len() > required {
        all.drain(0..all.len() - required);
    }

    info!(
        "[KlineLoader] Full load complete: {} bars for {} {}",
        all.len(),
        symbol,
        tf.as_str()
    );

    Ok(all)
}

/// 检测K线缺口
///
/// 对比 store 中最新已完成K线时间与新K线时间，判断是否有缺失
pub fn detect_gap(store: &crate::kline_store::KlineStore, new_bar: &KlineBar) -> Option<GapInfo> {
    let last_time = store.latest_closed_time()?;
    let duration_ms = store.timeframe_duration_ms();
    let expected_next = last_time + duration_ms;

    // 如果新K线时间超过预期时间 + 1个周期，认为有缺口
    if new_bar.open_time > expected_next + duration_ms {
        let missing_bars = ((new_bar.open_time - expected_next) / duration_ms) as usize;
        Some(GapInfo {
            from: expected_next,
            to: new_bar.open_time,
            missing_bars,
        })
    } else {
        None
    }
}

/// 缺口信息
pub struct GapInfo {
    pub from: i64,
    pub to: i64,
    pub missing_bars: usize,
}

/// 从交易所全量加载 K 线（历史 + 最新）
///
/// 策略服务的实时 K 线统一从交易所拉取（按策略配置的 market_type），
/// 不再依赖 DB（DB 现货数据仅用于回测）。
pub async fn hybrid_load(
    manager: &mut KlineManager,
    symbol: &str,
    tf: Timeframe,
    max_bars: usize,
    market_type: &str,
) -> Result<()> {
    let store = manager
        .get_mut(symbol, tf, market_type)
        .ok_or_else(|| anyhow!("Store not found for {} {} ({})", symbol, tf.as_str(), market_type))?;

    let exchange_bars = load_full_from_exchange(symbol, tf, max_bars, market_type).await?;
    let fetched = exchange_bars.len();
    store.extend_closed(exchange_bars);

    let total = store.total_count();
    let closed = store.closed_count();
    if fetched >= max_bars {
        info!(
            "[KlineLoader] {} {} ({}) 加载完整: {} bars (已收盘 {}, 进行中 {})",
            symbol, tf.as_str(), market_type, total, closed, total.saturating_sub(closed)
        );
    } else {
        warn!(
            "[KlineLoader] {} {} ({}) 加载不完整: 拉取 {} / {} bars (已收盘 {})",
            symbol, tf.as_str(), market_type, fetched, max_bars, closed
        );
    }

    Ok(())
}

// ============================================================
// Phase 3: 健康检查
// ============================================================

/// 单个 Store 的健康状态
#[derive(Debug, Clone)]
pub struct StoreHealth {
    pub symbol: String,
    pub timeframe: Timeframe,
    pub market_type: String,
    pub closed_count: usize,
    pub latest_closed_time: Option<i64>,
    pub age_periods: f64,      // 距最新数据有多少个周期
    pub is_stale: bool,        // 数据是否过旧（> 3 个周期）
    pub is_empty: bool,
}

/// 整体健康报告
#[derive(Debug)]
pub struct HealthReport {
    pub stores: Vec<StoreHealth>,
    pub stale_count: usize,
    pub empty_count: usize,
    pub total_stores: usize,
}

/// 检查 KlineManager 中所有 Store 的健康状态
pub fn check_health(manager: &KlineManager) -> HealthReport {
    let mut stores = Vec::new();
    let mut stale_count = 0;
    let mut empty_count = 0;

    for (symbol, tf, market_type) in manager.keys() {
        if let Some(store) = manager.get(&symbol, tf, &market_type) {
            let now_ms = Utc::now().timestamp_millis();
            let latest = store.latest_closed_time();
            let duration_ms = store.timeframe_duration_ms();

            let (age_periods, is_stale) = if let Some(latest_time) = latest {
                let age_ms = now_ms - latest_time;
                let periods = age_ms as f64 / duration_ms as f64;
                // 与 engine 的 validate_strategy_data 保持一致：超过 2 个周期即为过旧
                (periods, periods > 2.0)
            } else {
                (f64::INFINITY, true)
            };

            let is_empty = store.is_empty();

            if is_stale { stale_count += 1; }
            if is_empty { empty_count += 1; }

            stores.push(StoreHealth {
                symbol,
                timeframe: tf,
                market_type,
                closed_count: store.closed_count(),
                latest_closed_time: latest,
                age_periods,
                is_stale,
                is_empty,
            });
        }
    }

    let total_stores = stores.len();
    HealthReport {
        stores,
        stale_count,
        empty_count,
        total_stores,
    }
}

/// 健康检查 + 自动修复
///
/// 对过旧的 Store 从交易所补拉数据
pub async fn health_check_and_refill(
    manager: &Arc<tokio::sync::RwLock<KlineManager>>,
) -> HealthReport {
    // 先读取健康状态
    let report = {
        let mgr = manager.read().await;
        check_health(&mgr)
    };

    // 对过旧的 Store 补拉
    for health in &report.stores {
        if health.is_stale && !health.is_empty {
            if let Some(latest_time) = health.latest_closed_time {
                tracing::warn!(
                    "[HealthCheck] {} {} is stale ({:.1} periods behind), refilling...",
                    health.symbol,
                    health.timeframe.as_str(),
                    health.age_periods
                );

                let needed = health.age_periods.ceil() as usize + 10;
                match fill_gap_from_exchange(
                    &health.symbol,
                    health.timeframe,
                    latest_time,
                    needed,
                    &health.market_type,
                )
                .await
                {
                    Ok(bars) => {
                        if !bars.is_empty() {
                            let mut mgr = manager.write().await;
                            if let Some(store) = mgr.get_mut(&health.symbol, health.timeframe, &health.market_type) {
                                let count = bars.len();
                                let first_ts = bars.first().map(|b| b.open_time).unwrap_or(0);
                                let last_ts = bars.last().map(|b| b.open_time).unwrap_or(0);
                                store.extend_closed(bars);
                                let new_latest = store.latest_closed_time().unwrap_or(0);
                                let new_count = store.closed_count();
                                tracing::info!(
                                    "[HealthCheck] Refilled {} {} with {} bars (first={}, last={}), store now: latest={}, count={}",
                                    health.symbol,
                                    health.timeframe.as_str(),
                                    count,
                                    first_ts,
                                    last_ts,
                                    new_latest,
                                    new_count,
                                );
                            }
                        } else {
                            tracing::warn!(
                                "[HealthCheck] Exchange returned 0 bars for {} {} (after_time={})",
                                health.symbol,
                                health.timeframe.as_str(),
                                latest_time
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            "[HealthCheck] Failed to refill {} {}: {}",
                            health.symbol,
                            health.timeframe.as_str(),
                            e
                        );
                    }
                }
            }
        }
    }

    report
}

/// 启动定期健康检查任务
pub fn start_health_check(
    manager: Arc<tokio::sync::RwLock<KlineManager>>,
    interval_secs: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        loop {
            interval.tick().await;
            let report = health_check_and_refill(&manager).await;

            if report.stale_count > 0 || report.empty_count > 0 {
                tracing::warn!(
                    "[HealthCheck] {} total stores, {} stale, {} empty",
                    report.total_stores,
                    report.stale_count,
                    report.empty_count
                );
            } else {
                tracing::debug!(
                    "[HealthCheck] All {} stores healthy",
                    report.total_stores
                );
            }
        }
    })
}

/// 从策略实例收集所有 (symbol, timeframe) 对并计算 max_bars
///
/// 返回去重后的 (symbol, timeframe) 列表和所需的最大K线数
pub fn collect_data_requirements(
    strategies: &[crate::db::strategies::StrategyInstance],
) -> (Vec<(String, Timeframe, String)>, usize) {
    let mut pairs: Vec<(String, Timeframe, String)> = Vec::new();
    let mut max_bars: usize = 500; // 最小需求

    for strategy in strategies {
        let strategy_max = strategy.params
            .get("kline_limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(500) as usize;

        if strategy_max > max_bars {
            max_bars = strategy_max;
        }

        // 从策略获取需要的时间框架
        let timeframes = crate::strategies::get_strategy_timeframes(
            &strategy.strategy_type,
            &strategy.params,
        );

        for symbol in &strategy.symbols {
            for tf in &timeframes {
                // 每个 (symbol, timeframe, market_type) 独立建 store，spot/futures 隔离
                let pair = (symbol.clone(), *tf, strategy.market_type.clone());
                if !pairs.contains(&pair) {
                    pairs.push(pair);
                }
            }
        }
    }

    // 向上取整到 1000
    let max_bars = ((max_bars + 999) / 1000) * 1000;
    let max_bars = max_bars.max(1000);

    (pairs, max_bars)
}

// ============================================================
// Phase 4: 动态 Store 管理
// ============================================================

/// 确保所有策略所需的 Store 都存在
///
/// 检查当前活跃策略需要的 (symbol, timeframe) 对，
/// 如果 Store 不存在则创建并从交易所加载数据。
/// 返回新创建的 (symbol, timeframe) 对列表（用于 WS 订阅）。
pub async fn ensure_stores_for_strategies(
    pool: &PgPool,
    manager: &Arc<RwLock<KlineManager>>,
) -> Result<Vec<(String, Timeframe, String)>> {
    // 查询当前活跃策略
    let strategies = crate::db::strategies::list_active_strategies(pool).await?;
    let (required_pairs, _) = collect_data_requirements(&strategies);

    // 检查哪些 Store 缺失
    let missing: Vec<(String, Timeframe, String)> = {
        let mgr = manager.read().await;
        required_pairs
            .iter()
            .filter(|(symbol, tf, market_type)| mgr.get(symbol, *tf, market_type).is_none())
            .cloned()
            .collect()
    };

    if missing.is_empty() {
        return Ok(vec![]);
    }

    info!(
        "[Dynamic] Found {} missing stores, creating...",
        missing.len()
    );

    // 创建缺失的 Store 并加载数据
    let max_bars = {
        let mgr = manager.read().await;
        mgr.max_bars()
    };

    for (symbol, tf, market_type) in &missing {
        info!("[Dynamic] Creating store for {} {} ({})", symbol, tf.as_str(), market_type);

        // 创建 Store
        {
            let mut mgr = manager.write().await;
            mgr.init_stores(&[(symbol.clone(), *tf, market_type.clone())]);
        }

        // 从交易所加载数据（按策略配置的市场）
        {
            let mut mgr = manager.write().await;
            if let Err(e) = hybrid_load(&mut mgr, symbol, *tf, max_bars, market_type).await {
                error!(
                    "[Dynamic] Failed to load data for {} {} ({}): {}",
                    symbol,
                    tf.as_str(),
                    market_type,
                    e
                );
            }
        }
    }

    info!("[Dynamic] Created {} new stores", missing.len());
    Ok(missing)
}

/// 清理不再被任何策略使用的 Store
///
/// 返回被清理的 (symbol, timeframe) 对列表（用于取消 WS 订阅）
pub async fn cleanup_unused_stores(
    pool: &PgPool,
    manager: &Arc<RwLock<KlineManager>>,
) -> Result<Vec<(String, Timeframe, String)>> {
    // 查询当前活跃策略
    let strategies = crate::db::strategies::list_active_strategies(pool).await?;
    let (required_pairs, _) = collect_data_requirements(&strategies);

    // 找出不再需要的 Store
    let unused: Vec<(String, Timeframe, String)> = {
        let mgr = manager.read().await;
        mgr.keys()
            .into_iter()
            .filter(|key| !required_pairs.contains(key))
            .collect()
    };

    if unused.is_empty() {
        return Ok(vec![]);
    }

    info!(
        "[Dynamic] Found {} unused stores, cleaning up...",
        unused.len()
    );

    // 移除不再使用的 Store
    {
        let mut mgr = manager.write().await;
        for (symbol, tf, market_type) in &unused {
            if mgr.remove(symbol, *tf, market_type) {
                info!("[Dynamic] Removed unused store: {} {} ({})", symbol, tf.as_str(), market_type);
            }
        }
    }

    Ok(unused)
}

/// 启动定期动态管理任务
///
/// 每隔 check_interval_secs 秒检查一次策略变化，
/// 自动创建缺失的 Store，清理不再使用的 Store。
pub fn start_dynamic_manager(
    pool: PgPool,
    manager: Arc<RwLock<KlineManager>>,
    check_interval_secs: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(check_interval_secs));
        loop {
            interval.tick().await;

            // 确保所有策略所需的 Store 存在
            match ensure_stores_for_strategies(&pool, &manager).await {
                Ok(new_stores) => {
                    if !new_stores.is_empty() {
                        info!(
                            "[Dynamic] Created {} new stores: {:?}",
                            new_stores.len(),
                            new_stores.iter().map(|(s, t, m)| format!("{} {} ({})", s, t.as_str(), m)).collect::<Vec<_>>()
                        );
                        // TODO: 新增 WS 订阅
                    }
                }
                Err(e) => {
                    error!("[Dynamic] Error ensuring stores: {}", e);
                }
            }

            // 清理不再使用的 Store
            match cleanup_unused_stores(&pool, &manager).await {
                Ok(unused) => {
                    if !unused.is_empty() {
                        info!(
                            "[Dynamic] Found {} unused stores: {:?}",
                            unused.len(),
                            unused.iter().map(|(s, t, m)| format!("{} {} ({})", s, t.as_str(), m)).collect::<Vec<_>>()
                        );
                        // TODO: 取消 WS 订阅
                    }
                }
                Err(e) => {
                    error!("[Dynamic] Error cleaning up stores: {}", e);
                }
            }
        }
    })
}
