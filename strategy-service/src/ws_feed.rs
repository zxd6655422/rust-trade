//! Binance WebSocket 实时数据源
//!
//! 连接 Binance Futures/Spot Market Streams，订阅 K线数据。
//! 流程：连接 → 发送 SUBSCRIBE → 接收数据

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

/// Binance Futures Market Streams URL（无路径，连接后发 SUBSCRIBE）
const BINANCE_WS_FUTURES: &str = "wss://fstream.binance.com";

/// Binance Spot Market Streams URL
const BINANCE_WS_SPOT: &str = "wss://stream.binance.com:9443";

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

/// Binance WS kline 消息（直接格式，非组合流包装）
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
    interval: String,  // "1m", "5m", "30m" 等
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

/// SUBSCRIBE 响应
#[derive(Debug, Deserialize)]
struct SubscribeResponse {
    result: Option<serde_json::Value>,
    id: Option<String>,
    #[serde(rename = "msg")]
    message: Option<String>,
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

    /// 启动：一个连接订阅所有流（Binance 支持单连接多订阅）
    pub async fn run(
        self,
        kline_manager: Arc<RwLock<KlineManager>>,
        market_type: String,
    ) {
        info!("[WsFeed] Starting with {} subscriptions", self.subscriptions.len());

        let km = kline_manager.clone();
        let mt = market_type.clone();
        let subs = self.subscriptions.clone();
        let sender = self.event_sender.clone();

        tokio::spawn(async move {
            run_connection(subs, km, mt, sender).await;
        });
    }
}

/// 主连接循环
async fn run_connection(
    subscriptions: Vec<(String, Timeframe)>,
    kline_manager: Arc<RwLock<KlineManager>>,
    market_type: String,
    event_sender: broadcast::Sender<KlineEvent>,
) {
    let base_url = match market_type.as_str() {
        "spot" => BINANCE_WS_SPOT,
        _ => BINANCE_WS_FUTURES,
    };

    // 构建订阅参数
    let stream_names: Vec<String> = subscriptions
        .iter()
        .map(|(symbol, tf)| format!("{}@kline_{}", symbol.to_lowercase(), tf.as_str()))
        .collect();

    let mut attempt = 0u32;

    loop {
        info!("[WsFeed] Connecting to {} (attempt {})...", base_url, attempt + 1);

        match connect_subscribe_listen(
            base_url,
            &stream_names,
            &subscriptions,
            &kline_manager,
            &event_sender,
        )
        .await
        {
            Ok(()) => {
                attempt = 0;
                info!("[WsFeed] Connection ended, reconnecting...");
            }
            Err(e) => {
                attempt += 1;
                error!("[WsFeed] Connection error: {}", e);

                if attempt >= MAX_RECONNECT_ATTEMPTS {
                    error!("[WsFeed] Max attempts reached, giving up");
                    return;
                }
            }
        }

        let delay_ms = (RECONNECT_BASE_MS * 2u64.pow(attempt.min(5))).min(RECONNECT_MAX_MS);
        warn!("[WsFeed] Reconnecting in {}ms (attempt {})", delay_ms, attempt);
        sleep(Duration::from_millis(delay_ms)).await;

        // 重连后补拉所有流的缺口
        for (symbol, tf) in &subscriptions {
            if let Err(e) = fill_gap_single(symbol, *tf, &kline_manager, &market_type).await {
                error!("[WsFeed] Gap fill failed for {} {}: {}", symbol, tf.as_str(), e);
            }
        }
    }
}

/// 连接 → SUBSCRIBE → 监听
async fn connect_subscribe_listen(
    base_url: &str,
    stream_names: &[String],
    subscriptions: &[(String, Timeframe)],
    kline_manager: &Arc<RwLock<KlineManager>>,
    event_sender: &broadcast::Sender<KlineEvent>,
) -> Result<()> {
    info!("[WsFeed] Connecting to {}", base_url);

    let connect_result = tokio::time::timeout(
        Duration::from_secs(15),
        connect_async(base_url),
    )
    .await;

    let (ws_stream, response) = match connect_result {
        Ok(Ok((ws, resp))) => {
            info!("[WsFeed] Connected! Status: {}", resp.status());
            (ws, resp)
        }
        Ok(Err(e)) => {
            error!("[WsFeed] Connection failed: {}", e);
            return Err(anyhow!("Connection failed: {}", e));
        }
        Err(_) => {
            error!("[WsFeed] Connection timed out");
            return Err(anyhow!("Connection timed out"));
        }
    };

    let (mut write, mut read) = ws_stream.split();

    // 发送 SUBSCRIBE 消息
    let subscribe_msg = serde_json::json!({
        "method": "SUBSCRIBE",
        "params": stream_names,
        "id": "1"
    });
    let msg_text = subscribe_msg.to_string();
    info!("[WsFeed] Sending SUBSCRIBE: {}", msg_text);
    write.send(Message::Text(msg_text)).await?;
    info!("[WsFeed] SUBSCRIBE sent for {} streams", stream_names.len());

    // 启动 ping 任务
    let mut write_half = write;
    let ping_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(180));
        loop {
            interval.tick().await;
            if let Err(e) = write_half.send(Message::Ping(vec![])).await {
                warn!("[WsFeed] Ping failed: {}", e);
                break;
            }
        }
    });

    // 建立 stream_name -> (symbol, tf) 的映射
    let stream_map: std::collections::HashMap<String, (String, Timeframe)> = stream_names
        .iter()
        .zip(subscriptions.iter())
        .map(|(name, (sym, tf))| (name.clone(), (sym.clone(), *tf)))
        .collect();

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

                // 前几条消息打印完整内容用于调试
                if msg_count <= 5 {
                    info!("[WsFeed] msg #{}: {}", msg_count, &text[..text.len().min(200)]);
                } else if msg_count % 100 == 0 {
                    info!("[WsFeed] msg #{}", msg_count);
                }

                // 尝试解析为 SUBSCRIBE 响应
                if text.contains("\"result\"") || text.contains("\"msg\"") {
                    if let Ok(resp) = serde_json::from_str::<SubscribeResponse>(&text) {
                        info!("[WsFeed] Subscribe response: {:?}", resp);
                        continue;
                    }
                }

                // 尝试解析为 kline 消息
                if let Err(e) = handle_kline_message(&text, &stream_map, kline_manager, event_sender).await {
                    if msg_count <= 10 {
                        warn!("[WsFeed] Parse error (msg #{}): {} - raw: {}", msg_count, e, &text[..text.len().min(200)]);
                    }
                }
            }
            Ok(Message::Binary(data)) => {
                msg_count += 1;
                if let Ok(text) = String::from_utf8(data) {
                    let _ = handle_kline_message(&text, &stream_map, kline_manager, event_sender).await;
                }
            }
            Ok(Message::Ping(data)) => {
                tracing::debug!("[WsFeed] Ping received");
                let _ = data;
            }
            Ok(Message::Pong(_)) => {
                tracing::debug!("[WsFeed] Pong received");
            }
            Ok(Message::Close(_)) => {
                info!("[WsFeed] Server sent close");
                break;
            }
            Ok(Message::Frame(_)) => {}
            Err(e) => {
                error!("[WsFeed] Read error: {}", e);
                break;
            }
        }
    }

    ping_handle.abort();
    info!("[WsFeed] Read loop ended after {} messages", msg_count);
    Ok(())
}

/// 处理单条 kline 消息
async fn handle_kline_message(
    text: &str,
    stream_map: &std::collections::HashMap<String, (String, Timeframe)>,
    kline_manager: &Arc<RwLock<KlineManager>>,
    event_sender: &broadcast::Sender<KlineEvent>,
) -> Result<()> {
    let msg: WsKlineMessage = serde_json::from_str(text)?;

    // 从 k.i (interval) 字段获取 timeframe
    let tf = Timeframe::from_str(&msg.k.interval)
        .ok_or_else(|| anyhow!("Unknown interval: {}", msg.k.interval))?;

    let symbol = msg.symbol.clone();

    // 验证此 (symbol, tf) 在我们的订阅列表中
    let stream_key = format!("{}@kline_{}", symbol.to_lowercase(), msg.k.interval);
    if !stream_map.contains_key(&stream_key) {
        return Ok(()); // 不在订阅列表，忽略
    }

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
