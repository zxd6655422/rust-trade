//! WebSocket 实时推送模块
//!
//! 将策略信号通过 WebSocket 实时推送到前端

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures::{sink::SinkExt, stream::StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};

/// WebSocket 消息类型
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WsMessage {
    pub msg_type: String,  // signal, status, error
    pub data: serde_json::Value,
}

/// WebSocket 状态
pub struct WsState {
    /// 信号广播发送器
    pub signal_tx: broadcast::Sender<WsMessage>,
    /// 连接的客户端数量
    pub client_count: Arc<RwLock<usize>>,
}

impl WsState {
    pub fn new() -> Self {
        let (signal_tx, _) = broadcast::channel(100);
        Self {
            signal_tx,
            client_count: Arc::new(RwLock::new(0)),
        }
    }

    /// 广播信号给所有连接的客户端
    pub fn broadcast_signal(&self, msg: WsMessage) {
        if let Err(e) = self.signal_tx.send(msg) {
            warn!("Failed to broadcast signal: {}", e);
        }
    }
}

/// WebSocket 升级处理
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<WsState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// 处理 WebSocket 连接
async fn handle_socket(socket: WebSocket, state: Arc<WsState>) {
    // 增加客户端计数
    {
        let mut count = state.client_count.write().await;
        *count += 1;
        info!("WebSocket client connected, total: {}", *count);
    }

    let (mut sender, mut receiver) = socket.split();

    // 订阅信号广播
    let mut rx = state.signal_tx.subscribe();

    // 发送欢迎消息
    let welcome = WsMessage {
        msg_type: "status".to_string(),
        data: serde_json::json!({
            "connected": true,
            "message": "Strategy signal WebSocket connected"
        }),
    };
    if let Ok(msg) = serde_json::to_string(&welcome) {
        let _ = sender.send(Message::Text(msg.into())).await;
    }

    // 创建任务处理消息发送
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if let Ok(json) = serde_json::to_string(&msg) {
                if sender.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
        }
    });

    // 创建任务处理消息接收（客户端可能发送订阅请求等）
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    debug!("Received WebSocket message: {}", text);
                    // 处理客户端消息（如订阅特定交易对）
                }
                Message::Close(_) => {
                    break;
                }
                _ => {}
            }
        }
    });

    // 等待任一任务完成
    tokio::select! {
        _ = &mut send_task => {
            recv_task.abort();
        }
        _ = &mut recv_task => {
            send_task.abort();
        }
    }

    // 减少客户端计数
    {
        let mut count = state.client_count.write().await;
        *count -= 1;
        info!("WebSocket client disconnected, total: {}", *count);
    }
}

/// 创建 WebSocket 路由
pub fn create_ws_router(state: Arc<WsState>) -> axum::Router {
    axum::Router::new()
        .route("/ws/signals", axum::routing::get(ws_handler))
        .with_state(state)
}
