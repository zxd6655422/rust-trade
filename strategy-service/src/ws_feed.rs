//! Binance WebSocket 实时数据源
//!
//! 通过 WebSocket 订阅 Binance K线数据流，实时更新 KlineManager。
//! 支持：
//! - 多 (symbol, timeframe) 订阅
//! - 自动重连（指数退避）
//! - 断连后 REST 补拉缺口
//! - 未完成K线实时更新，完成后触发策略

use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::{broadcast, RwLock};
use tokio::time::sleep;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{error, info, warn};

use crate::kline_loader;
use crate::kline_store::{KlineBar, KlineManager};
use crate::redis_reader::Timeframe;

/// Binance Futures WebSocket 基础 URL
const BINANCE_WS_FUTURES: &str = "wss://fstream.binance.com";

/// Binance Spot WebSocket 基础 URL
const BINANCE_WS_SPOT: &str = "wss://stream.binance.com:9443";

/// 重连参数
const RECONNECT_BASE_MS: u64 = 1000;   // 初始退避 1s
const RECONNECT_MAX_MS: u64 = 30000;   // 最大退避 30s
const MAX_RECONNECT_ATTEMPTS: u32 = 20;

/// K线事件（从 WebSocket 推送）
#[derive(Debug, Clone)]
pub struct KlineEvent {
    pub symbol: String,
    pub timeframe: Timeframe,
    pub bar: KlineBar,
}

/// Binance WS kline 消息结构
#[derive(Debug, Deserialize)]
struct WsKlineMessage {
    stream: String,
    data: WsKlineData,
}

#[derive(Debug, Deserialize)]
struct WsKlineData {
    #[serde(rename = "s")]
    symbol: String,
    k: WsKline,
}

#[derive(Debug, Deserialize)]
struct WsKline {
    #[serde(rename = "t")]
    open_time: i64,
    #[serde(rename = "o")]
    open: String,
    #[serde(rename = "h")]
    high: String,
    #[serde(rename = "l")]
    low: String,
    #[serde(rename = "c")]
    close: String,
    #[serde(rename = "v")]
    volume: String,
    #[serde(rename = "x")]
    closed: bool,
}

/// WebSocket 数据源
pub struct WsFeed {
    /// 订阅列表
    subscriptions: Vec<(String, Timeframe)>,
    /// 事件广播通道
    event_sender: broadcast::Sender<KlineEvent>,
    /// 市场类型
    market_type: String,
}

impl WsFeed {
    /// 创建新的 WsFeed
    ///
    /// # 返回
    /// (WsFeed, event_receiver) — receiver 用于接收 KlineEvent
    pub fn new(
        subscriptions: Vec<(String, Timeframe)>,
        market_type: String,
        buffer_size: usize,
    ) -> (Self, broadcast::Receiver<KlineEvent>) {
        let (sender, receiver) = broadcast::channel(buffer_size);
        (
            WsFeed {
                subscriptions,
                event_sender: sender,
                market_type,
            },
            receiver,
        )
    }

    /// 构建订阅流名称列表
    fn stream_names(&self) -> Vec<String> {
        self.subscriptions
            .iter()
            .map(|(symbol, tf)| {
                format!("{}@kline_{}", symbol.to_lowercase(), tf.as_str())
            })
            .collect()
    }

    /// 获取 WebSocket URL（组合流模式）
    fn ws_url(&self) -> String {
        let base = match self.market_type.as_str() {
            "spot" => BINANCE_WS_SPOT,
            _ => BINANCE_WS_FUTURES,
        };

        let streams = self.stream_names().join("/");
        format!("{}/stream?streams={}", base, streams)
    }

    /// 启动 WebSocket 连接（主循环，自动重连）
    pub async fn run(
        self,
        kline_manager: Arc<RwLock<KlineManager>>,
        market_type: String,
    ) {
        info!("[WsFeed] Starting with {} subscriptions", self.subscriptions.len());
        let mut attempt = 0u32;

        loop {
            info!(
                "[WsFeed] Connecting to Binance WebSocket (attempt {})...",
                attempt + 1
            );

            match self.connect_and_listen(&kline_manager, &market_type).await {
                Ok(()) => {
                    // 正常断开，重置重连计数
                    attempt = 0;
                    info!("[WsFeed] Connection closed normally, reconnecting...");
                }
                Err(e) => {
                    attempt += 1;
                    error!("[WsFeed] Connection error: {}", e);

                    if attempt >= MAX_RECONNECT_ATTEMPTS {
                        error!(
                            "[WsFeed] Max reconnect attempts ({}) reached, giving up",
                            MAX_RECONNECT_ATTEMPTS
                        );
                        return;
                    }
                }
            }

            // 指数退避
            let delay_ms = (RECONNECT_BASE_MS * 2u64.pow(attempt.min(5))).min(RECONNECT_MAX_MS);
            warn!(
                "[WsFeed] Reconnecting in {}ms (attempt {}/{})",
                delay_ms, attempt, MAX_RECONNECT_ATTEMPTS
            );
            sleep(Duration::from_millis(delay_ms)).await;

            // 重连后补拉缺口
            if let Err(e) = self.fill_gaps_after_reconnect(&kline_manager, &market_type).await {
                error!("[WsFeed] Failed to fill gaps after reconnect: {}", e);
            }
        }
    }

    /// 建立连接并监听消息
    async fn connect_and_listen(
        &self,
        kline_manager: &Arc<RwLock<KlineManager>>,
        _market_type: &str,
    ) -> Result<()> {
        let url = self.ws_url();
        info!("[WsFeed] Connecting to: {}", url);

        let connect_result = connect_async(&url).await;
        match &connect_result {
            Ok((_, response)) => {
                info!("[WsFeed] Connected! Status: {}", response.status());
            }
            Err(e) => {
                error!("[WsFeed] Connection failed: {}", e);
            }
        }
        let (ws_stream, _) = connect_result?;

        let (mut write, mut read) = ws_stream.split();

        // 启动心跳（每 30 秒发送 ping）
        let ping_handle = tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(30)).await;
                if let Err(e) = write.send(Message::Ping(vec![])).await {
                    error!("[WsFeed] Ping failed: {}", e);
                    break;
                }
            }
        });

        // 读取消息
        let mut msg_count: u64 = 0;
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    msg_count += 1;
                    if msg_count <= 3 || msg_count % 100 == 0 {
                        info!("[WsFeed] Received message #{}: {} bytes", msg_count, text.len());
                    }
                    if let Err(e) = self.handle_message(&text, kline_manager).await {
                        warn!("[WsFeed] Error handling message: {}", e);
                    }
                }
                Ok(Message::Ping(data)) => {
                    // 自动 pong 由 tungstenite 处理
                    tracing::debug!("[WsFeed] Received ping: {:?}", data);
                }
                Ok(Message::Pong(_)) => {
                    tracing::debug!("[WsFeed] Received pong");
                }
                Ok(Message::Close(_)) => {
                    info!("[WsFeed] Server sent close frame");
                    break;
                }
                Err(e) => {
                    error!("[WsFeed] WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }

        ping_handle.abort();
        Ok(())
    }

    /// 处理单条 WebSocket 消息
    async fn handle_message(
        &self,
        text: &str,
        kline_manager: &Arc<RwLock<KlineManager>>,
    ) -> Result<()> {
        let msg: WsKlineMessage = serde_json::from_str(text)?;

        // 从 stream 名称解析 symbol 和 timeframe
        // stream 格式: "btcusdt@kline_30m"
        let parts: Vec<&str> = msg.stream.split('@').collect();
        if parts.len() != 2 {
            return Err(anyhow!("Invalid stream format: {}", msg.stream));
        }

        let symbol = msg.data.symbol.clone();

        // 解析 timeframe
        let tf_str = parts[1].strip_prefix("kline_").unwrap_or(parts[1]);
        let timeframe = Timeframe::from_str(tf_str)
            .ok_or_else(|| anyhow!("Unknown timeframe: {}", tf_str))?;

        // 构建 KlineBar
        let bar = KlineBar {
            open_time: msg.data.k.open_time,
            open: msg.data.k.open.parse::<f64>().unwrap_or(0.0),
            high: msg.data.k.high.parse::<f64>().unwrap_or(0.0),
            low: msg.data.k.low.parse::<f64>().unwrap_or(0.0),
            close: msg.data.k.close.parse::<f64>().unwrap_or(0.0),
            volume: msg.data.k.volume.parse::<f64>().unwrap_or(0.0),
            closed: msg.data.k.closed,
        };

        // 更新 KlineManager
        let is_closed = bar.closed;
        {
            let mut manager = kline_manager.write().await;
            if let Some(store) = manager.get_mut(&symbol, timeframe) {
                if is_closed {
                    // 检测间隙
                    if let Some(gap) = kline_loader::detect_gap(store, &bar) {
                        warn!(
                            "[WsFeed] Gap detected for {} {}: {} missing bars ({} -> {})",
                            symbol,
                            tf_str,
                            gap.missing_bars,
                            gap.from,
                            gap.to
                        );
                        // 间隙会在下次重连时补拉
                    }
                    store.push_closed(bar.clone());
                    tracing::debug!(
                        "[WsFeed] {} {} closed bar: O={} H={} L={} C={} V={}",
                        symbol,
                        tf_str,
                        bar.open,
                        bar.high,
                        bar.low,
                        bar.close,
                        bar.volume
                    );
                } else {
                    store.update_current(bar.clone());
                }
            }
        }

        // 广播事件（仅已完成的K线）
        if is_closed {
            let event = KlineEvent {
                symbol,
                timeframe,
                bar,
            };
            let _ = self.event_sender.send(event);
        }

        Ok(())
    }

    /// 重连后补拉缺口
    async fn fill_gaps_after_reconnect(
        &self,
        kline_manager: &Arc<RwLock<KlineManager>>,
        market_type: &str,
    ) -> Result<()> {
        // 先收集需要补拉的 (symbol, timeframe, latest_time)
        let gaps: Vec<(String, Timeframe, i64, usize)> = {
            let manager = kline_manager.read().await;
            let mut gaps = Vec::new();

            for (symbol, tf) in &self.subscriptions {
                if let Some(store) = manager.get(symbol, *tf) {
                    if let Some(latest_time) = store.latest_closed_time() {
                        let duration_ms = store.timeframe_duration_ms();
                        let now_ms = chrono::Utc::now().timestamp_millis();
                        let gap_ms = now_ms - latest_time;

                        if gap_ms > duration_ms * 2 {
                            let missing_bars = (gap_ms / duration_ms) as usize;
                            gaps.push((symbol.clone(), *tf, latest_time, missing_bars));
                        }
                    }
                }
            }
            gaps
        };

        // 逐个补拉缺口
        for (symbol, tf, latest_time, missing_bars) in gaps {
            info!(
                "[WsFeed] Filling gap for {} {}: {} bars",
                symbol,
                tf.as_str(),
                missing_bars
            );

            let fill_bars = kline_loader::fill_gap_from_exchange(
                &symbol,
                tf,
                latest_time,
                missing_bars + 10,
                market_type,
            )
            .await?;

            let mut manager = kline_manager.write().await;
            if let Some(store) = manager.get_mut(&symbol, tf) {
                store.extend_closed(fill_bars);
            }
        }

        Ok(())
    }
}

/// 启动 WebSocket 数据源任务
///
/// 创建 WsFeed 并在后台运行，返回事件接收器
pub async fn start_ws_feed(
    subscriptions: Vec<(String, Timeframe)>,
    kline_manager: Arc<RwLock<KlineManager>>,
    market_type: String,
) -> broadcast::Receiver<KlineEvent> {
    let buffer_size = 1024;
    let (feed, receiver) = WsFeed::new(subscriptions, market_type.clone(), buffer_size);

    // 后台运行 WebSocket
    let km = kline_manager.clone();
    let mt = market_type.clone();
    tokio::spawn(async move {
        feed.run(km, mt).await;
    });

    receiver
}
