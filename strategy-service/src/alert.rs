//! 告警系统模块
//!
//! 重要信号触发时发送通知（日志/Webhook）

use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// 告警级别
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AlertLevel {
    Info,
    Warning,
    Critical,
}

impl AlertLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            AlertLevel::Info => "INFO",
            AlertLevel::Warning => "WARNING",
            AlertLevel::Critical => "CRITICAL",
        }
    }
}

/// 告警消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertMessage {
    pub level: AlertLevel,
    pub title: String,
    pub message: String,
    pub symbol: Option<String>,
    pub strategy: Option<String>,
    pub data: Option<serde_json::Value>,
}

/// Webhook 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub enabled: bool,
    pub url: String,
    pub headers: Option<std::collections::HashMap<String, String>>,
}

/// 告警配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertConfig {
    /// 是否启用日志告警
    pub log_enabled: bool,
    /// 是否启用 Webhook
    pub webhook: WebhookConfig,
    /// 最小告警级别
    pub min_level: AlertLevel,
    /// 告警冷却时间（秒），避免重复告警
    pub cooldown_secs: u64,
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            log_enabled: true,
            webhook: WebhookConfig {
                enabled: false,
                url: String::new(),
                headers: None,
            },
            min_level: AlertLevel::Warning,
            cooldown_secs: 300, // 5分钟冷却
        }
    }
}

/// 告警历史记录（用于冷却）
#[derive(Debug, Clone)]
struct AlertRecord {
    pub key: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// 告警管理器
pub struct AlertManager {
    config: AlertConfig,
    http_client: Client,
    history: Arc<RwLock<Vec<AlertRecord>>>,
}

impl AlertManager {
    pub fn new(config: AlertConfig) -> Self {
        Self {
            config,
            http_client: Client::new(),
            history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 检查是否应该发送告警（冷却检查）
    async fn should_alert(&self, key: &str) -> bool {
        let now = chrono::Utc::now();
        let mut history = self.history.write().await;

        // 清理过期记录
        history.retain(|r| {
            (now - r.timestamp).num_seconds() < self.config.cooldown_secs as i64
        });

        // 检查是否有重复
        !history.iter().any(|r| r.key == key)
    }

    /// 记录告警历史
    async fn record_alert(&self, key: &str) {
        let mut history = self.history.write().await;
        history.push(AlertRecord {
            key: key.to_string(),
            timestamp: chrono::Utc::now(),
        });
    }

    /// 发送告警
    pub async fn send(&self, alert: &AlertMessage) -> Result<()> {
        // 检查告警级别
        if !self.should_alert_level(&alert.level) {
            return Ok(());
        }

        // 生成告警 key（用于冷却检查）
        let key = format!("{}:{:?}:{:?}", alert.level.as_str(), alert.symbol, alert.strategy);

        // 冷却检查
        if !self.should_alert(&key).await {
            info!("Alert cooldown active, skipping: {}", key);
            return Ok(());
        }

        // 发送日志告警
        if self.config.log_enabled {
            self.send_log_alert(alert);
        }

        // 发送 Webhook 告警
        if self.config.webhook.enabled && !self.config.webhook.url.is_empty() {
            self.send_webhook_alert(alert).await?;
        }

        // 记录告警历史
        self.record_alert(&key).await;

        Ok(())
    }

    /// 检查告警级别是否满足最小要求
    fn should_alert_level(&self, level: &AlertLevel) -> bool {
        let level_value = match level {
            AlertLevel::Info => 0,
            AlertLevel::Warning => 1,
            AlertLevel::Critical => 2,
        };
        let min_value = match &self.config.min_level {
            AlertLevel::Info => 0,
            AlertLevel::Warning => 1,
            AlertLevel::Critical => 2,
        };
        level_value >= min_value
    }

    /// 发送日志告警
    fn send_log_alert(&self, alert: &AlertMessage) {
        let prefix = format!("[ALERT][{}]", alert.level.as_str());
        match alert.level {
            AlertLevel::Info => {
                info!("{} {}: {}", prefix, alert.title, alert.message);
            }
            AlertLevel::Warning => {
                warn!("{} {}: {}", prefix, alert.title, alert.message);
            }
            AlertLevel::Critical => {
                error!("{} {}: {}", prefix, alert.title, alert.message);
            }
        }
    }

    /// 发送 Webhook 告警
    async fn send_webhook_alert(&self, alert: &AlertMessage) -> Result<()> {
        let mut request = self.http_client.post(&self.config.webhook.url)
            .json(alert);

        // 添加自定义 headers
        if let Some(headers) = &self.config.webhook.headers {
            for (key, value) in headers {
                request = request.header(key, value);
            }
        }

        match request.send().await {
            Ok(response) => {
                if !response.status().is_success() {
                    warn!(
                        "Webhook alert failed with status: {}",
                        response.status()
                    );
                }
            }
            Err(e) => {
                error!("Webhook alert failed: {}", e);
            }
        }

        Ok(())
    }
}

/// 创建信号告警消息
pub fn create_signal_alert(
    symbol: &str,
    strategy: &str,
    direction: &str,
    entry_price: f64,
    signal_strength: f64,
    reason: &str,
) -> AlertMessage {
    let level = if signal_strength > 0.8 {
        AlertLevel::Critical
    } else if signal_strength > 0.5 {
        AlertLevel::Warning
    } else {
        AlertLevel::Info
    };

    AlertMessage {
        level,
        title: format!("策略信号: {} {}", direction.to_uppercase(), symbol),
        message: format!(
            "策略: {}\n方向: {}\n入场价: {:.2}\n信号强度: {:.2}\n原因: {}",
            strategy, direction, entry_price, signal_strength, reason
        ),
        symbol: Some(symbol.to_string()),
        strategy: Some(strategy.to_string()),
        data: Some(serde_json::json!({
            "direction": direction,
            "entry_price": entry_price,
            "signal_strength": signal_strength,
        })),
    }
}

/// 创建交易执行告警消息
pub fn create_trade_alert(
    symbol: &str,
    side: &str,
    quantity: f64,
    order_id: &str,
) -> AlertMessage {
    AlertMessage {
        level: AlertLevel::Info,
        title: format!("交易执行: {} {}", side, symbol),
        message: format!(
            "交易对: {}\n方向: {}\n数量: {:.4}\n订单ID: {}",
            symbol, side, quantity, order_id
        ),
        symbol: Some(symbol.to_string()),
        strategy: None,
        data: Some(serde_json::json!({
            "side": side,
            "quantity": quantity,
            "order_id": order_id,
        })),
    }
}

/// 创建错误告警消息
pub fn create_error_alert(
    title: &str,
    error: &str,
    symbol: Option<&str>,
) -> AlertMessage {
    AlertMessage {
        level: AlertLevel::Critical,
        title: title.to_string(),
        message: error.to_string(),
        symbol: symbol.map(|s| s.to_string()),
        strategy: None,
        data: None,
    }
}
