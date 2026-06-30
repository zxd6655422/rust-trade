// api/websocket.rs
// WebSocket 处理器

use actix::{Actor, ActorContext, AsyncContext, Handler, Message, Running, StreamHandler};
use actix_web::{web, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tracing::{error, info, warn};

use trading_common::data::types::TickData;

/// WebSocket 心跳间隔
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
/// WebSocket 超时时间
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

/// WebSocket 订阅消息
#[derive(Debug, Deserialize)]
pub enum WsRequest {
    /// 订阅交易对
    Subscribe { symbols: Vec<String> },
    /// 取消订阅
    Unsubscribe { symbols: Vec<String> },
    /// 请求回测
    Backtest {
        strategy: String,
        symbol: String,
        capital: f64,
        data_count: i64,
    },
}

/// WebSocket 响应消息
#[derive(Debug, Serialize)]
pub enum WsResponse {
    /// 实时数据
    Tick(TickMessage),
    /// 回测进度
    BacktestProgress(BacktestProgressMessage),
    /// 回测完成
    BacktestResult(BacktestResultMessage),
    /// 错误
    Error { message: String },
    /// 订阅确认
    Subscribed { symbols: Vec<String> },
    /// 取消订阅确认
    Unsubscribed { symbols: Vec<String> },
}

#[derive(Debug, Serialize)]
pub struct TickMessage {
    pub symbol: String,
    pub price: String,
    pub quantity: String,
    pub timestamp: String,
}

#[derive(Debug, Serialize)]
pub struct BacktestProgressMessage {
    pub progress: f64,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct BacktestResultMessage {
    pub success: bool,
    pub data: Option<serde_json::Value>,
}

/// WebSocket 会话
pub struct WsSession {
    /// 唯一会话 ID
    id: String,
    /// 最后心跳时间
    hb: Instant,
    /// 订阅的交易对
    subscribed_symbols: Vec<String>,
    /// 数据接收通道
    tick_rx: broadcast::Receiver<TickData>,
}

impl WsSession {
    pub fn new(tick_rx: broadcast::Receiver<TickData>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            hb: Instant::now(),
            subscribed_symbols: Vec::new(),
            tick_rx,
        }
    }

    /// 启动心跳检查
    fn start_heartbeat(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                warn!("WebSocket heartbeat failed, disconnecting");
                ctx.stop();
                return;
            }
            ctx.ping(b"");
        });
    }

    /// 处理订阅请求
    fn handle_subscribe(&mut self, symbols: Vec<String>) -> WsResponse {
        for symbol in &symbols {
            if !self.subscribed_symbols.contains(symbol) {
                self.subscribed_symbols.push(symbol.clone());
            }
        }
        info!("Session {} subscribed to: {:?}", self.id, self.subscribed_symbols);
        WsResponse::Subscribed {
            symbols: self.subscribed_symbols.clone(),
        }
    }

    /// 处理取消订阅请求
    fn handle_unsubscribe(&mut self, symbols: Vec<String>) -> WsResponse {
        self.subscribed_symbols.retain(|s| !symbols.contains(s));
        info!("Session {} unsubscribed, remaining: {:?}", self.id, self.subscribed_symbols);
        WsResponse::Unsubscribed {
            symbols: self.subscribed_symbols.clone(),
        }
    }

    /// 检查是否订阅了该交易对
    fn is_subscribed(&self, symbol: &str) -> bool {
        self.subscribed_symbols.is_empty() || self.subscribed_symbols.contains(&symbol.to_string())
    }
}

impl Actor for WsSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("WebSocket session started: {}", self.id);
        self.start_heartbeat(ctx);

        // 启动数据接收任务
        let addr = ctx.address();
        let mut tick_rx = self.tick_rx.resubscribe();

        tokio::spawn(async move {
            loop {
                match tick_rx.recv().await {
                    Ok(tick) => {
                        addr.do_send(TickDataMessage(tick));
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("WebSocket tick receiver lagged by {} messages", n);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("Tick channel closed");
                        break;
                    }
                }
            }
        });
    }

    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        info!("WebSocket session stopping: {}", self.id);
        Running::Stop
    }
}

/// Tick 数据消息
#[derive(Message)]
#[rtype(result = "()")]
struct TickDataMessage(TickData);

impl Handler<TickDataMessage> for WsSession {
    type Result = ();

    fn handle(&mut self, msg: TickDataMessage, ctx: &mut Self::Context) {
        let tick = msg.0;

        // 检查是否订阅了该交易对
        if !self.is_subscribed(&tick.symbol) {
            return;
        }

        let response = WsResponse::Tick(TickMessage {
            symbol: tick.symbol,
            price: tick.price.to_string(),
            quantity: tick.quantity.to_string(),
            timestamp: tick.timestamp.format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
        });

        if let Ok(json) = serde_json::to_string(&response) {
            ctx.text(json);
        }
    }
}

/// 处理 WebSocket 消息
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WsSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        let msg = match msg {
            Ok(msg) => msg,
            Err(e) => {
                error!("WebSocket message error: {}", e);
                ctx.stop();
                return;
            }
        };

        match msg {
            ws::Message::Ping(msg) => {
                self.hb = Instant::now();
                ctx.pong(&msg);
            }
            ws::Message::Pong(_) => {
                self.hb = Instant::now();
            }
            ws::Message::Text(text) => {
                self.hb = Instant::now();

                // 解析请求
                match serde_json::from_str::<WsRequest>(&text) {
                    Ok(request) => {
                        let response = match request {
                            WsRequest::Subscribe { symbols } => self.handle_subscribe(symbols),
                            WsRequest::Unsubscribe { symbols } => self.handle_unsubscribe(symbols),
                            WsRequest::Backtest { .. } => {
                                // 回测通过 HTTP API 处理，WebSocket 只用于实时数据
                                WsResponse::Error {
                                    message: "Backtest should be called via HTTP API".to_string(),
                                }
                            }
                        };

                        if let Ok(json) = serde_json::to_string(&response) {
                            ctx.text(json);
                        }
                    }
                    Err(e) => {
                        warn!("Invalid WebSocket request: {}", e);
                        let response = WsResponse::Error {
                            message: format!("Invalid request: {}", e),
                        };
                        if let Ok(json) = serde_json::to_string(&response) {
                            ctx.text(json);
                        }
                    }
                }
            }
            ws::Message::Binary(_) => {
                warn!("Binary messages not supported");
            }
            ws::Message::Close(reason) => {
                info!("WebSocket close requested: {:?}", reason);
                ctx.close(reason);
                ctx.stop();
            }
            ws::Message::Continuation(_) => {
                // 不处理 continuation 消息
            }
            ws::Message::Nop => {
                // 不处理 nop 消息
            }
        }
    }
}

/// WebSocket 入口点
pub async fn ws_handler(
    req: HttpRequest,
    stream: web::Payload,
    tick_tx: web::Data<broadcast::Sender<TickData>>,
) -> Result<HttpResponse, actix_web::Error> {
    let tick_rx = tick_tx.subscribe();
    let session = WsSession::new(tick_rx);
    ws::start(session, &req, stream)
}
