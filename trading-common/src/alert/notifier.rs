// alert/notifier.rs
// 告警通知器 - 支持日志、Webhook 等通知渠道

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{warn, error, info};

// ===== 告警类型 =====

/// 告警级别
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlertLevel {
    /// 信息
    Info,
    /// 警告
    Warning,
    /// 严重
    Critical,
}

impl std::fmt::Display for AlertLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertLevel::Info => write!(f, "INFO"),
            AlertLevel::Warning => write!(f, "WARNING"),
            AlertLevel::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// 告警类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AlertType {
    /// 交易失败
    TradeFailure,
    /// 风控触发
    RiskControl,
    /// 服务异常
    ServiceError,
    /// 连接断开
    ConnectionLost,
    /// 资金异常
    FundAnomaly,
    /// 黑天鹅检测
    BlackSwan,
    /// 熔断触发
    CircuitBreaker,
}

impl std::fmt::Display for AlertType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertType::TradeFailure => write!(f, "TRADE_FAILURE"),
            AlertType::RiskControl => write!(f, "RISK_CONTROL"),
            AlertType::ServiceError => write!(f, "SERVICE_ERROR"),
            AlertType::ConnectionLost => write!(f, "CONNECTION_LOST"),
            AlertType::FundAnomaly => write!(f, "FUND_ANOMALY"),
            AlertType::BlackSwan => write!(f, "BLACK_SWAN"),
            AlertType::CircuitBreaker => write!(f, "CIRCUIT_BREAKER"),
        }
    }
}

/// 告警消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    /// 告警级别
    pub level: AlertLevel,
    /// 告警类型
    pub alert_type: AlertType,
    /// 告警标题
    pub title: String,
    /// 告警详情
    pub message: String,
    /// 相关交易对
    pub symbol: Option<String>,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
}

impl Alert {
    pub fn new(
        level: AlertLevel,
        alert_type: AlertType,
        title: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            level,
            alert_type,
            title: title.into(),
            message: message.into(),
            symbol: None,
            timestamp: Utc::now(),
        }
    }

    pub fn with_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.symbol = Some(symbol.into());
        self
    }
}

// ===== 配置 =====

/// Webhook 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    /// Webhook URL (支持 Slack, Discord, 飞书, 企业微信等)
    pub url: String,
    /// 是否启用
    pub enabled: bool,
    /// 最低告警级别
    pub min_level: AlertLevel,
}

/// 告警配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertConfig {
    /// 是否启用告警
    pub enabled: bool,
    /// 是否输出到日志
    pub log_enabled: bool,
    /// Webhook 配置
    pub webhook: Option<WebhookConfig>,
    /// 告警冷却时间 (秒) - 同类型告警的最小间隔
    pub cooldown_secs: u64,
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            log_enabled: true,
            webhook: None,
            cooldown_secs: 60, // 默认1分钟冷却
        }
    }
}

// ===== 通知器 =====

/// 告警通知器
pub struct AlertNotifier {
    config: AlertConfig,
    /// 最近告警记录 (用于冷却)
    recent_alerts: Arc<RwLock<Vec<(AlertType, DateTime<Utc>)>>>,
    /// HTTP 客户端
    client: reqwest::Client,
}

impl AlertNotifier {
    pub fn new(config: AlertConfig) -> Self {
        Self {
            config,
            recent_alerts: Arc::new(RwLock::new(Vec::new())),
            client: reqwest::Client::new(),
        }
    }

    /// 发送告警
    pub async fn notify(&self, alert: Alert) {
        if !self.config.enabled {
            return;
        }

        // 检查冷却
        if self.is_in_cooldown(&alert.alert_type).await {
            return;
        }

        // 记录告警
        self.record_alert(alert.alert_type.clone()).await;

        // 日志输出
        if self.config.log_enabled {
            self.log_alert(&alert);
        }

        // Webhook 通知
        if let Some(ref webhook) = self.config.webhook {
            if webhook.enabled && alert.level >= webhook.min_level {
                self.send_webhook(webhook, &alert).await;
            }
        }
    }

    /// 检查是否在冷却期
    async fn is_in_cooldown(&self, alert_type: &AlertType) -> bool {
        let recent = self.recent_alerts.read().await;
        let now = Utc::now();
        let cooldown = chrono::Duration::seconds(self.config.cooldown_secs as i64);

        recent.iter().any(|(t, ts)| {
            t == alert_type && (now - *ts) < cooldown
        })
    }

    /// 记录告警时间
    async fn record_alert(&self, alert_type: AlertType) {
        let mut recent = self.recent_alerts.write().await;
        recent.push((alert_type, Utc::now()));

        // 清理过期记录 (保留最近100条)
        if recent.len() > 100 {
            let cutoff = Utc::now() - chrono::Duration::hours(1);
            recent.retain(|(_, ts)| *ts > cutoff);
        }
    }

    /// 日志输出
    fn log_alert(&self, alert: &Alert) {
        let prefix = format!("[ALERT][{}][{}]", alert.level, alert.alert_type);
        let symbol_info = alert.symbol.as_deref().unwrap_or("N/A");
        let full_msg = format!("{} {} | Symbol: {} | {}",
            prefix, alert.title, symbol_info, alert.message);

        match alert.level {
            AlertLevel::Info => info!("{}", full_msg),
            AlertLevel::Warning => warn!("{}", full_msg),
            AlertLevel::Critical => error!("{}", full_msg),
        }
    }

    /// 发送 Webhook
    async fn send_webhook(&self, config: &WebhookConfig, alert: &Alert) {
        let payload = serde_json::json!({
            "level": alert.level.to_string(),
            "type": alert.alert_type.to_string(),
            "title": alert.title,
            "message": alert.message,
            "symbol": alert.symbol,
            "timestamp": alert.timestamp.to_rfc3339(),
        });

        match self.client.post(&config.url)
            .json(&payload)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
        {
            Ok(resp) => {
                if !resp.status().is_success() {
                    warn!("Webhook returned status: {}", resp.status());
                }
            }
            Err(e) => {
                warn!("Failed to send webhook: {}", e);
            }
        }
    }

    // ===== 便捷方法 =====

    /// 交易失败告警
    pub async fn trade_failure(&self, symbol: &str, reason: &str) {
        self.notify(Alert::new(
            AlertLevel::Warning,
            AlertType::TradeFailure,
            "交易执行失败",
            format!("交易对: {}, 原因: {}", symbol, reason),
        ).with_symbol(symbol)).await;
    }

    /// 风控触发告警
    pub async fn risk_triggered(&self, symbol: &str, rule: &str, detail: &str) {
        self.notify(Alert::new(
            AlertLevel::Warning,
            AlertType::RiskControl,
            "风控规则触发",
            format!("交易对: {}, 规则: {}, 详情: {}", symbol, rule, detail),
        ).with_symbol(symbol)).await;
    }

    /// 服务异常告警
    pub async fn service_error(&self, service: &str, error: &str) {
        self.notify(Alert::new(
            AlertLevel::Critical,
            AlertType::ServiceError,
            "服务异常",
            format!("服务: {}, 错误: {}", service, error),
        )).await;
    }

    /// 连接断开告警
    pub async fn connection_lost(&self, target: &str) {
        self.notify(Alert::new(
            AlertLevel::Warning,
            AlertType::ConnectionLost,
            "连接断开",
            format!("目标: {} 连接已断开", target),
        )).await;
    }

    /// 黑天鹅检测告警
    pub async fn black_swan_detected(&self, symbol: &str, change_pct: &str) {
        self.notify(Alert::new(
            AlertLevel::Critical,
            AlertType::BlackSwan,
            "黑天鹅事件检测",
            format!("交易对: {}, 价格波动: {}", symbol, change_pct),
        ).with_symbol(symbol)).await;
    }

    /// 熔断触发告警
    pub async fn circuit_breaker_triggered(&self, reason: &str, duration_secs: u64) {
        self.notify(Alert::new(
            AlertLevel::Critical,
            AlertType::CircuitBreaker,
            "熔断机制触发",
            format!("原因: {}, 持续时间: {}秒", reason, duration_secs),
        )).await;
    }
}

/// 共享的告警通知器
pub type SharedAlertNotifier = Arc<AlertNotifier>;
