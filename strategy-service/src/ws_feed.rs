//! K线实时数据源
//!
//! 提供两种模式：
//! 1. WebSocket 模式（ws）— 实时推送，低延迟
//! 2. 轮询模式（poll）— 每 N 秒从 REST API 拉取，简单可靠
//!
//! 通过环境变量 KLINE_FEED_MODE 选择，默认 "poll"

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::sync::{broadcast, RwLock};
use tracing::{error, info, warn};

use crate::kline_loader;
use crate::kline_store::{KlineBar, KlineManager};
use crate::redis_reader::Timeframe;

/// K线事件
#[derive(Debug, Clone)]
pub struct KlineEvent {
    pub symbol: String,
    pub timeframe: Timeframe,
    pub bar: KlineBar,
}

/// 数据源配置
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

    /// 启动数据源
    pub async fn run(
        self,
        kline_manager: Arc<RwLock<KlineManager>>,
        market_type: String,
    ) {
        let mode = std::env::var("KLINE_FEED_MODE").unwrap_or_else(|_| "poll".to_string());

        match mode.as_str() {
            "ws" => {
                info!("[KlineFeed] Starting in WebSocket mode");
                self.run_ws(kline_manager, market_type).await;
            }
            _ => {
                info!("[KlineFeed] Starting in polling mode (interval: 30s)");
                self.run_poll(kline_manager, market_type).await;
            }
        }
    }

    /// 轮询模式：每 30 秒从 REST API 拉取最新数据
    async fn run_poll(
        self,
        kline_manager: Arc<RwLock<KlineManager>>,
        market_type: String,
    ) {
        let poll_interval = Duration::from_secs(30);

        info!(
            "[KlinePoll] Starting polling for {} subscriptions, interval: {:?}",
            self.subscriptions.len(),
            poll_interval
        );

        let mut interval = tokio::time::interval(poll_interval);

        // 首次立即执行
        poll_all(&self.subscriptions, &kline_manager, &market_type).await;

        loop {
            interval.tick().await;
            poll_all(&self.subscriptions, &kline_manager, &market_type).await;
        }
    }

    /// WebSocket 模式（备用，如果服务器支持 WS）
    async fn run_ws(
        self,
        kline_manager: Arc<RwLock<KlineManager>>,
        market_type: String,
    ) {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::{connect_async, tungstenite::Message};

        let base_url = match market_type.as_str() {
            "spot" => "wss://stream.binance.com:9443/ws",
            _ => "wss://fstream.binance.com/ws",
        };

        // 每个流一个连接
        for (symbol, tf) in &self.subscriptions {
            let km = kline_manager.clone();
            let mt = market_type.clone();
            let sym = symbol.clone();
            let timeframe = *tf;
            let sender = self.event_sender.clone();
            let url = format!("{}/{}@kline_{}", base_url, sym.to_lowercase(), tf.as_str());

            tokio::spawn(async move {
                let mut attempt = 0u32;
                loop {
                    info!("[WsFeed:{}_{}] Connecting (attempt {})...", sym, timeframe.as_str(), attempt + 1);

                    match tokio::time::timeout(
                        Duration::from_secs(15),
                        connect_async(&url),
                    )
                    .await
                    {
                        Ok(Ok((ws_stream, resp))) => {
                            info!("[WsFeed:{}_{}] Connected! Status: {}", sym, timeframe.as_str(), resp.status());
                            attempt = 0;

                            let (mut write, mut read) = ws_stream.split();
                            let mut msg_count = 0u64;

                            while let Ok(Some(msg)) = tokio::time::timeout(
                                Duration::from_secs(300),
                                read.next(),
                            )
                            .await
                            {
                                match msg {
                                    Ok(Message::Text(text)) => {
                                        msg_count += 1;
                                        if msg_count <= 3 {
                                            info!("[WsFeed:{}_{}] msg #{}: {}", sym, timeframe.as_str(), msg_count, &text[..text.len().min(200)]);
                                        }
                                        if let Ok(kline_msg) = serde_json::from_str::<WsKlineMessage>(&text) {
                                            let bar = kline_msg.to_kline_bar();
                                            let is_closed = bar.closed;
                                            {
                                                let mut mgr = km.write().await;
                                                if let Some(store) = mgr.get_mut(&sym, timeframe) {
                                                    if is_closed { store.push_closed(bar.clone()); }
                                                    else { store.update_current(bar.clone()); }
                                                }
                                            }
                                            if is_closed {
                                                let _ = sender.send(KlineEvent { symbol: sym.clone(), timeframe, bar });
                                            }
                                        }
                                    }
                                    Ok(Message::Ping(d)) => { let _ = write.send(Message::Pong(d)).await; }
                                    Ok(Message::Close(_)) => break,
                                    Err(_) => break,
                                    _ => {}
                                }
                            }
                            info!("[WsFeed:{}_{}] Ended after {} msgs", sym, timeframe.as_str(), msg_count);
                        }
                        Ok(Err(e)) => {
                            attempt += 1;
                            error!("[WsFeed:{}_{}] Connection failed: {}", sym, timeframe.as_str(), e);
                        }
                        Err(_) => {
                            attempt += 1;
                            error!("[WsFeed:{}_{}] Connection timed out", sym, timeframe.as_str());
                        }
                    }

                    if attempt >= 20 { return; }
                    let delay = (1000 * 2u64.pow(attempt.min(5))).min(30000);
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                }
            });
        }
    }
}

/// WS kline 消息结构
#[derive(serde::Deserialize)]
struct WsKlineMessage {
    #[serde(rename = "s")]
    symbol: String,
    k: WsKlineData,
}

#[derive(serde::Deserialize)]
struct WsKlineData {
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

impl WsKlineMessage {
    fn to_kline_bar(&self) -> KlineBar {
        KlineBar {
            open_time: self.k.open_time,
            open: self.k.open.parse().unwrap_or(0.0),
            high: self.k.high.parse().unwrap_or(0.0),
            low: self.k.low.parse().unwrap_or(0.0),
            close: self.k.close.parse().unwrap_or(0.0),
            volume: self.k.volume.parse().unwrap_or(0.0),
            closed: self.k.closed,
        }
    }
}

/// 轮询所有订阅
async fn poll_all(
    subscriptions: &[(String, Timeframe)],
    kline_manager: &Arc<RwLock<KlineManager>>,
    market_type: &str,
) {
    for (symbol, tf) in subscriptions {
        if let Err(e) = poll_single(symbol, *tf, kline_manager, market_type).await {
            warn!("[KlinePoll] Failed to poll {} {}: {}", symbol, tf.as_str(), e);
        }
    }
}

/// 轮询单个 (symbol, timeframe)
async fn poll_single(
    symbol: &str,
    tf: Timeframe,
    kline_manager: &Arc<RwLock<KlineManager>>,
    market_type: &str,
) -> Result<()> {
    // 从交易所拉取最新 2 根 K线（1 根已完成 + 1 根进行中）
    let bars = kline_loader::fetch_klines_from_exchange(
        symbol,
        tf.as_str(),
        2,
        None,
        market_type,
    )
    .await?;

    if bars.is_empty() {
        return Ok(());
    }

    let mut manager = kline_manager.write().await;
    if let Some(store) = manager.get_mut(symbol, tf) {
        for bar in &bars {
            if bar.closed {
                // 检查是否是新数据（避免重复）
                if let Some(latest) = store.latest_closed_time() {
                    if bar.open_time > latest {
                        store.push_closed(bar.clone());
                    }
                } else {
                    store.push_closed(bar.clone());
                }
            } else {
                store.update_current(bar.clone());
            }
        }
    }

    Ok(())
}

/// 启动数据源任务
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
