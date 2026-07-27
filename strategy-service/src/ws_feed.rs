//! Binance WebSocket 实时数据源
//!
//! 使用直接 URL 流模式：wss://fstream.binance.com/ws/<streamName>
//! 每个流一个独立连接，自动接收数据，无需 SUBSCRIBE。

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

/// Binance Futures 直接流 URL
const BINANCE_WS_FUTURES: &str = "wss://fstream.binance.com/ws";

/// Binance Spot 直接流 URL
const BINANCE_WS_SPOT: &str = "wss://stream.binance.com:9443/ws";

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

    /// 启动所有订阅（每个流一个独立连接）
    pub async fn run(
        self,
        kline_manager: Arc<RwLock<KlineManager>>,
        market_type: String,
    ) {
        info!("[WsFeed] Starting {} stream connections", self.subscriptions.len());

        for (symbol, tf) in &self.subscriptions {
            let km = kline_manager.clone();
            let mt = market_type.clone();
            let sym = symbol.clone();
            let timeframe = *tf;
            let sender = self.event_sender.clone();

            tokio::spawn(async move {
                run_single_stream(sym, timeframe, km, mt, sender).await;
            });
        }
    }
}

/// 单个流的连接循环
async fn run_single_stream(
    symbol: String,
    tf: Timeframe,
    kline_manager: Arc<RwLock<KlineManager>>,
    market_type: String,
    event_sender: broadcast::Sender<KlineEvent>,
) {
    let stream_name = format!("{}@kline_{}", symbol.to_lowercase(), tf.as_str());
    let base_url = match market_type.as_str() {
        "spot" => BINANCE_WS_SPOT,
        _ => BINANCE_WS_FUTURES,
    };
    let url = format!("{}/{}", base_url, stream_name);

    let mut attempt = 0u32;

    loop {
        info!("[WsFeed:{}] Connecting (attempt {})...", stream_name, attempt + 1);

        match connect_and_listen(
            &url,
            &stream_name,
            &symbol,
            tf,
            &kline_manager,
            &event_sender,
        )
        .await
        {
            Ok(()) => {
                attempt = 0;
                info!("[WsFeed:{}] Connection ended, reconnecting...", stream_name);
            }
            Err(e) => {
                attempt += 1;
                error!("[WsFeed:{}] Error: {}", stream_name, e);

                if attempt >= MAX_RECONNECT_ATTEMPTS {
                    error!("[WsFeed:{}] Max attempts reached, giving up", stream_name);
                    return;
                }
            }
        }

        let delay_ms = (RECONNECT_BASE_MS * 2u64.pow(attempt.min(5))).min(RECONNECT_MAX_MS);
        warn!("[WsFeed:{}] Reconnecting in {}ms", stream_name, delay_ms);
        sleep(Duration::from_millis(delay_ms)).await;

        // 重连后补拉缺口
        if let Err(e) = fill_gap_single(&symbol, tf, &kline_manager, &market_type).await {
            error!("[WsFeed:{}] Gap fill failed: {}", stream_name, e);
        }
    }
}

/// 连接并监听单个流
async fn connect_and_listen(
    url: &str,
    stream_name: &str,
    symbol: &str,
    tf: Timeframe,
    kline_manager: &Arc<RwLock<KlineManager>>,
    event_sender: &broadcast::Sender<KlineEvent>,
) -> Result<()> {
    info!("[WsFeed:{}] Connecting to {}", stream_name, url);

    let connect_result = tokio::time::timeout(
        Duration::from_secs(15),
        connect_async(url),
    )
    .await;

    let (ws_stream, response) = match connect_result {
        Ok(Ok((ws, resp))) => {
            info!("[WsFeed:{}] Connected! Status: {}", stream_name, resp.status());
            (ws, resp)
        }
        Ok(Err(e)) => {
            error!("[WsFeed:{}] Connection failed: {}", stream_name, e);
            return Err(anyhow!("Connection failed: {}", e));
        }
        Err(_) => {
            error!("[WsFeed:{}] Connection timed out", stream_name);
            return Err(anyhow!("Connection timed out"));
        }
    };

    let (mut write, mut read) = ws_stream.split();

    // 启动 ping 任务
    let ping_name = stream_name.to_string();
    let ping_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(180));
        loop {
            interval.tick().await;
            if let Err(e) = write.send(Message::Ping(vec![])).await {
                warn!("[WsFeed:{}] Ping failed: {}", ping_name, e);
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
                    info!("[WsFeed:{}] msg #{}: {}", stream_name, msg_count, &text[..text.len().min(300)]);
                } else if msg_count % 60 == 0 {
                    info!("[WsFeed:{}] msg #{}", stream_name, msg_count);
                }
                if let Err(e) = handle_kline_message(&text, symbol, tf, kline_manager, event_sender).await {
                    if msg_count <= 10 {
                        warn!("[WsFeed:{}] Parse error: {} | raw: {}", stream_name, e, &text[..text.len().min(200)]);
                    }
                }
            }
            Ok(Message::Binary(data)) => {
                msg_count += 1;
                // Binary 消息也尝试解析
                if let Ok(text) = String::from_utf8(data.clone()) {
                    if msg_count <= 5 {
                        info!("[WsFeed:{}] binary msg #{}: {}", stream_name, msg_count, &text[..text.len().min(300)]);
                    }
                    if let Err(e) = handle_kline_message(&text, symbol, tf, kline_manager, event_sender).await {
                        if msg_count <= 10 {
                            warn!("[WsFeed:{}] Binary parse error: {} | raw: {}", stream_name, e, &text[..text.len().min(200)]);
                        }
                    }
                } else if msg_count <= 5 {
                    info!("[WsFeed:{}] binary msg #{}: {} bytes (not UTF-8)", stream_name, msg_count, data.len());
                }
            }
            Ok(Message::Ping(data)) => {
                tracing::debug!("[WsFeed:{}] Ping", stream_name);
                let _ = data;
            }
            Ok(Message::Pong(_)) => {
                tracing::debug!("[WsFeed:{}] Pong", stream_name);
            }
            Ok(Message::Close(_)) => {
                info!("[WsFeed:{}] Server close", stream_name);
                break;
            }
            Ok(Message::Frame(frame)) => {
                msg_count += 1;
                info!("[WsFeed:{}] Raw frame #{}: {:?}", stream_name, msg_count, frame);
            }
            Err(e) => {
                error!("[WsFeed:{}] Read error: {}", stream_name, e);
                break;
            }
        }
    }

    ping_handle.abort();
    info!("[WsFeed:{}] Read loop ended after {} messages", stream_name, msg_count);
    Ok(())
}

/// 处理单条 kline 消息
async fn handle_kline_message(
    text: &str,
    symbol: &str,
    tf: Timeframe,
    kline_manager: &Arc<RwLock<KlineManager>>,
    event_sender: &broadcast::Sender<KlineEvent>,
) -> Result<()> {
    let msg: WsKlineMessage = serde_json::from_str(text)?;

    let bar = KlineBar {
        open_time: msg.k.open_time,
        open: msg.k.open.parse::<f64>().unwrap_or(0.0),
        high: msg.k.high.parse::<f64>().unwrap_or(0.0),
        low: msg.k.low.parse::<f64>().unwrap_or(0.0),
        close: msg.k.close.parse::<f64>().unwrap_or(0.0),
        volume: msg.k.volume.parse::<f64>().unwrap_or(0.0),
        closed: msg.k.closed,
    };

    let is_closed = bar.closed;

    // 更新 KlineManager
    {
        let mut manager = kline_manager.write().await;
        if let Some(store) = manager.get_mut(symbol, tf) {
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
            symbol: symbol.to_string(),
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
                "[WsFeed:{}_{}] Filling gap: {} bars",
                symbol.to_lowercase(),
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
