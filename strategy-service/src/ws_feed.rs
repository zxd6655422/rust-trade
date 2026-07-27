//! Binance WebSocket 实时数据源
//!
//! 使用组合流模式：单个连接订阅多个 stream，避免超过连接数限制。
//! Binance Futures 限制：每 IP 最多 10 个连接，每连接最多 200 个流。
//!
//! 流程：连接 wss://fstream.binance.com/stream?streams=s1/s2/s3 → 自动接收数据

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

/// Binance Futures 组合流 URL
const BINANCE_WS_FUTURES: &str = "wss://fstream.binance.com/stream";

/// Binance Spot 组合流 URL
const BINANCE_WS_SPOT: &str = "wss://stream.binance.com:9443/stream";

/// 每个连接最多订阅的流数量（Binance 限制 200，保守用 100）
const MAX_STREAMS_PER_CONNECTION: usize = 100;

/// 重连参数
const RECONNECT_BASE_MS: u64 = 1000;
const RECONNECT_MAX_MS: u64 = 30000;
const MAX_RECONNECT_ATTEMPTS: u32 = 20;

/// K线事件
#[derive(Debug, Clone)]
pub struct KlineEvent {
    pub symbol: String,
    pub timeframe: Timeframe,
    pub bar: KlineBar,
}

/// 组合流消息包装
#[derive(Debug, Deserialize)]
struct CombinedStreamMessage {
    stream: String,
    data: WsKlineMessage,
}

/// Binance WS kline 消息
#[derive(Debug, Deserialize)]
struct WsKlineMessage {
    #[serde(rename = "e")]
    event_type: Option<String>,
    #[serde(rename = "s")]
    symbol: String,
    k: WsKline,
}

#[derive(Debug, Deserialize)]
struct WsKline {
    #[serde(rename = "t")]
    open_time: i64,
    #[serde(rename = "i")]
    interval: String,
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
    subscriptions: Vec<(String, Timeframe)>,
    event_sender: broadcast::Sender<KlineEvent>,
    market_type: String,
}

impl WsFeed {
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

    /// 启动：将订阅分组，每组一个连接
    pub async fn run(
        self,
        kline_manager: Arc<RwLock<KlineManager>>,
        market_type: String,
    ) {
        // 将 subscriptions 分组
        let groups = chunk_subscriptions(&self.subscriptions, MAX_STREAMS_PER_CONNECTION);
        info!(
            "[WsFeed] Starting {} stream connections for {} subscriptions",
            groups.len(),
            self.subscriptions.len()
        );

        for (group_idx, group) in groups.into_iter().enumerate() {
            let km = kline_manager.clone();
            let mt = market_type.clone();
            let sender = self.event_sender.clone();

            let stream_names: Vec<String> = group
                .iter()
                .map(|(s, tf)| format!("{}@kline_{}", s.to_lowercase(), tf.as_str()))
                .collect();

            info!(
                "[WsFeed] Group {}: {} streams: {:?}",
                group_idx,
                stream_names.len(),
                stream_names
            );

            tokio::spawn(async move {
                run_connection_group(group_idx, group, stream_names, km, mt, sender).await;
            });
        }
    }
}

/// 将订阅分组
fn chunk_subscriptions(
    subs: &[(String, Timeframe)],
    chunk_size: usize,
) -> Vec<Vec<(String, Timeframe)>> {
    subs.chunks(chunk_size)
        .map(|chunk| chunk.to_vec())
        .collect()
}

/// 单个连接组的主循环
async fn run_connection_group(
    group_idx: usize,
    subscriptions: Vec<(String, Timeframe)>,
    stream_names: Vec<String>,
    kline_manager: Arc<RwLock<KlineManager>>,
    market_type: String,
    event_sender: broadcast::Sender<KlineEvent>,
) {
    let base_url = match market_type.as_str() {
        "spot" => BINANCE_WS_SPOT,
        _ => BINANCE_WS_FUTURES,
    };

    let streams_param = stream_names.join("/");
    let url = format!("{}?streams={}", base_url, streams_param);

    let mut attempt = 0u32;

    loop {
        info!(
            "[WsFeed:G{}] Connecting (attempt {}) with {} streams...",
            group_idx, attempt + 1, stream_names.len()
        );

        match connect_and_listen_group(
            group_idx,
            &url,
            &subscriptions,
            &kline_manager,
            &event_sender,
        )
        .await
        {
            Ok(()) => {
                attempt = 0;
                info!("[WsFeed:G{}] Connection ended, reconnecting...", group_idx);
            }
            Err(e) => {
                attempt += 1;
                error!("[WsFeed:G{}] Error: {}", group_idx, e);

                if attempt >= MAX_RECONNECT_ATTEMPTS {
                    error!("[WsFeed:G{}] Max attempts reached, giving up", group_idx);
                    return;
                }
            }
        }

        let delay_ms = (RECONNECT_BASE_MS * 2u64.pow(attempt.min(5))).min(RECONNECT_MAX_MS);
        warn!("[WsFeed:G{}] Reconnecting in {}ms", group_idx, delay_ms);
        sleep(Duration::from_millis(delay_ms)).await;

        // 重连后补拉所有流的缺口
        for (symbol, tf) in &subscriptions {
            if let Err(e) = fill_gap_single(symbol, *tf, &kline_manager, &market_type).await {
                error!("[WsFeed:G{}] Gap fill failed for {} {}: {}", group_idx, symbol, tf.as_str(), e);
            }
        }
    }
}

/// 连接并监听组合流
async fn connect_and_listen_group(
    group_idx: usize,
    url: &str,
    subscriptions: &[(String, Timeframe)],
    kline_manager: &Arc<RwLock<KlineManager>>,
    event_sender: &broadcast::Sender<KlineEvent>,
) -> Result<()> {
    info!("[WsFeed:G{}] Connecting to {}", group_idx, url);

    let connect_result = tokio::time::timeout(
        Duration::from_secs(15),
        connect_async(url),
    )
    .await;

    let (ws_stream, response) = match connect_result {
        Ok(Ok((ws, resp))) => {
            info!("[WsFeed:G{}] Connected! Status: {}", group_idx, resp.status());
            (ws, resp)
        }
        Ok(Err(e)) => {
            error!("[WsFeed:G{}] Connection failed: {}", group_idx, e);
            return Err(anyhow!("Connection failed: {}", e));
        }
        Err(_) => {
            error!("[WsFeed:G{}] Connection timed out", group_idx);
            return Err(anyhow!("Connection timed out"));
        }
    };

    let (mut write, mut read) = ws_stream.split();

    // 启动 ping 任务
    let ping_idx = group_idx;
    let ping_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(180));
        loop {
            interval.tick().await;
            if let Err(e) = write.send(Message::Ping(vec![])).await {
                warn!("[WsFeed:G{}] Ping failed: {}", ping_idx, e);
                break;
            }
        }
    });

    let mut msg_count: u64 = 0;

    // 读取消息（300秒超时）
    while let Ok(Some(msg)) = tokio::time::timeout(
        Duration::from_secs(300),
        read.next(),
    )
    .await
    {
        match msg {
            Ok(Message::Text(text)) => {
                msg_count += 1;
                if msg_count <= 5 {
                    info!("[WsFeed:G{}] msg #{}: {}", group_idx, msg_count, &text[..text.len().min(300)]);
                } else if msg_count % 100 == 0 {
                    info!("[WsFeed:G{}] msg #{}", group_idx, msg_count);
                }
                if let Err(e) = handle_combined_message(&text, kline_manager, event_sender).await {
                    if msg_count <= 10 {
                        warn!("[WsFeed:G{}] Parse error: {} | raw: {}", group_idx, e, &text[..text.len().min(200)]);
                    }
                }
            }
            Ok(Message::Binary(data)) => {
                msg_count += 1;
                if let Ok(text) = String::from_utf8(data) {
                    if msg_count <= 5 {
                        info!("[WsFeed:G{}] binary msg #{}: {}", group_idx, msg_count, &text[..text.len().min(300)]);
                    }
                    let _ = handle_combined_message(&text, kline_manager, event_sender).await;
                }
            }
            Ok(Message::Ping(data)) => {
                tracing::debug!("[WsFeed:G{}] Ping", group_idx);
                let _ = data;
            }
            Ok(Message::Pong(_)) => {
                tracing::debug!("[WsFeed:G{}] Pong", group_idx);
            }
            Ok(Message::Close(_)) => {
                info!("[WsFeed:G{}] Server close", group_idx);
                break;
            }
            Ok(Message::Frame(_)) => {}
            Err(e) => {
                error!("[WsFeed:G{}] Read error: {}", group_idx, e);
                break;
            }
        }
    }

    ping_handle.abort();
    info!("[WsFeed:G{}] Read loop ended after {} messages", group_idx, msg_count);
    Ok(())
}

/// 处理组合流消息
async fn handle_combined_message(
    text: &str,
    kline_manager: &Arc<RwLock<KlineManager>>,
    event_sender: &broadcast::Sender<KlineEvent>,
) -> Result<()> {
    // 组合流格式: {"stream": "btcusdt@kline_5m", "data": {...}}
    let msg: CombinedStreamMessage = serde_json::from_str(text)?;

    let symbol = msg.data.symbol.clone();
    let tf = Timeframe::from_str(&msg.data.k.interval)
        .ok_or_else(|| anyhow!("Unknown interval: {}", msg.data.k.interval))?;

    let bar = KlineBar {
        open_time: msg.data.k.open_time,
        open: msg.data.k.open.parse::<f64>().unwrap_or(0.0),
        high: msg.data.k.high.parse::<f64>().unwrap_or(0.0),
        low: msg.data.k.low.parse::<f64>().unwrap_or(0.0),
        close: msg.data.k.close.parse::<f64>().unwrap_or(0.0),
        volume: msg.data.k.volume.parse::<f64>().unwrap_or(0.0),
        closed: msg.data.k.closed,
    };

    let is_closed = bar.closed;

    // 更新 KlineManager
    {
        let mut manager = kline_manager.write().await;
        if let Some(store) = manager.get_mut(&symbol, tf) {
            if is_closed {
                store.push_closed(bar.clone());
            } else {
                store.update_current(bar.clone());
            }
        }
    }

    // 广播已完成的K线
    if is_closed {
        let _ = event_sender.send(KlineEvent {
            symbol,
            timeframe: tf,
            bar,
        });
    }

    Ok(())
}

/// 重连后补拉缺口
async fn fill_gap_single(
    symbol: &str,
    tf: Timeframe,
    kline_manager: &Arc<RwLock<KlineManager>>,
    market_type: &str,
) -> Result<()> {
    let (latest_time, duration_ms) = {
        let mgr = kline_manager.read().await;
        if let Some(store) = mgr.get(symbol, tf) {
            (store.latest_closed_time(), store.timeframe_duration_ms())
        } else {
            return Ok(());
        }
    };

    if let Some(latest) = latest_time {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let gap_ms = now_ms - latest;

        if gap_ms > duration_ms * 2 {
            let missing = (gap_ms / duration_ms) as usize;
            info!(
                "[WsFeed] Filling gap for {} {}: {} bars",
                symbol,
                tf.as_str(),
                missing
            );

            let bars = kline_loader::fill_gap_from_exchange(
                symbol,
                tf,
                latest,
                missing + 10,
                market_type,
            )
            .await?;

            if !bars.is_empty() {
                let mut mgr = kline_manager.write().await;
                if let Some(store) = mgr.get_mut(symbol, tf) {
                    store.extend_closed(bars);
                }
            }
        }
    }

    Ok(())
}

/// 启动 WebSocket 数据源任务
pub async fn start_ws_feed(
    subscriptions: Vec<(String, Timeframe)>,
    kline_manager: Arc<RwLock<KlineManager>>,
    market_type: String,
) -> broadcast::Receiver<KlineEvent> {
    let buffer_size = 1024;
    let (feed, receiver) = WsFeed::new(subscriptions, market_type.clone(), buffer_size);

    let km = kline_manager.clone();
    let mt = market_type.clone();
    tokio::spawn(async move {
        feed.run(km, mt).await;
    });

    receiver
}
