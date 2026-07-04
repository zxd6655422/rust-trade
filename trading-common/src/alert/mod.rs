// alert/mod.rs
// 告警模块 - 支持多种通知渠道

pub mod notifier;

pub use notifier::{
    Alert, AlertConfig, AlertLevel, AlertType, AlertNotifier, WebhookConfig,
};
