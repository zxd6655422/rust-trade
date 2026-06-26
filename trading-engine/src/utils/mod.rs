// utils/mod.rs
// 工具模块

/// 日志工具
pub mod logger {
    use tracing_subscriber::{fmt, util::SubscriberInitExt, EnvFilter};

    /// 初始化日志系统
    pub fn init(level: &str) {
        let _filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

        fmt::Subscriber::builder()
            .with_target(true)
            .with_file(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .init();
    }
}

/// 数学工具
pub mod math {
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    /// 计算百分比变化
    pub fn percentage_change(old: Decimal, new: Decimal) -> Decimal {
        if old == dec!(0) {
            return dec!(0);
        }
        ((new - old) / old) * dec!(100)
    }

    /// 限制小数位数
    pub fn round_decimal(value: Decimal, places: u32) -> Decimal {
        value.round_dp(places)
    }

    /// 计算止损价格
    pub fn calculate_stop_loss(entry_price: Decimal, stop_loss_pct: Decimal) -> Decimal {
        entry_price * (dec!(1) - stop_loss_pct)
    }

    /// 计算止盈价格
    pub fn calculate_take_profit(entry_price: Decimal, take_profit_pct: Decimal) -> Decimal {
        entry_price * (dec!(1) + take_profit_pct)
    }
}

/// 时间工具
pub mod time {
    use chrono::{DateTime, Utc};

    /// 格式化时间戳
    pub fn format_timestamp(dt: &DateTime<Utc>) -> String {
        dt.format("%Y-%m-%d %H:%M:%S UTC").to_string()
    }

    /// 获取当前时间戳字符串
    pub fn now_string() -> String {
        Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string()
    }
}
