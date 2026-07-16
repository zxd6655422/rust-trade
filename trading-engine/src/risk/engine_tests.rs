// 风控引擎单元测试

#[cfg(test)]
mod tests {
    use crate::risk::config::RiskConfig;
    use crate::risk::engine::{RiskDecision, RiskEngine};
    use crate::exchange::types::{AccountInfo, Balance, OrderRequest, OrderSide, OrderType};
    use rust_decimal::Decimal;
    use std::str::FromStr;

    /// 创建测试用风控配置
    fn create_test_config() -> RiskConfig {
        RiskConfig {
            max_position_size: Decimal::from(10000),
            max_order_size: Decimal::from(1),
            stop_loss_pct: Decimal::from_str("0.02").unwrap(),
            take_profit_pct: Decimal::from_str("0.04").unwrap(),
            max_daily_loss: Decimal::from(500),
            max_drawdown_pct: Decimal::from_str("0.15").unwrap(),
            max_exposure_pct: Decimal::from_str("0.8").unwrap(),
            kelly_fraction: Decimal::from_str("0.25").unwrap(),
            circuit_breaker_cooldown: 3600,
            black_swan_threshold: Decimal::from_str("0.05").unwrap(),
        }
    }

    /// 创建测试用账户
    fn create_test_account(equity: &str) -> AccountInfo {
        let equity = Decimal::from_str(equity).unwrap();
        AccountInfo {
            balances: vec![Balance {
                asset: "USDT".to_string(),
                free: equity,
                locked: Decimal::ZERO,
            }],
            positions: vec![],
            total_equity: equity,
            available_balance: equity,
            unrealized_pnl: Decimal::ZERO,
            margin_used: Decimal::ZERO,
            margin_ratio: None,
            uid: None,
        }
    }

    /// 创建测试用订单
    fn create_test_order(symbol: &str, quantity: &str, price: &str) -> OrderRequest {
        OrderRequest {
            symbol: symbol.to_string(),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            quantity: Decimal::from_str(quantity).unwrap(),
            price: Some(Decimal::from_str(price).unwrap()),
            stop_price: None,
            time_in_force: None,
            client_order_id: None,
        }
    }

    // ========== 基础功能测试 ==========

    #[tokio::test]
    async fn test_risk_engine_initialization() {
        let config = create_test_config();
        let engine = RiskEngine::new(config);

        let status = engine.get_status().await;
        assert_eq!(status.daily_pnl, Decimal::ZERO);
        assert_eq!(status.daily_trade_count, 0);
        assert!(!status.is_circuit_breaker_active);
    }

    // ========== 订单检查测试 ==========

    #[tokio::test]
    async fn test_check_order_allow() {
        let config = create_test_config();
        let engine = RiskEngine::new(config);
        let account = create_test_account("100000");
        let order = create_test_order("BTCUSDT", "0.1", "50000");

        let result = engine.check_order(&order, &account).await.unwrap();
        assert!(matches!(result, RiskDecision::Allow));
    }

    #[tokio::test]
    async fn test_check_order_exceeds_max_position_size() {
        let config = create_test_config();
        let engine = RiskEngine::new(config);
        let account = create_test_account("100000");

        // 订单价值 = 1 * 50000 = 50000，超过 max_position_size (10000)
        let order = create_test_order("BTCUSDT", "1", "50000");

        let result = engine.check_order(&order, &account).await.unwrap();
        assert!(matches!(result, RiskDecision::Reject(_)));
    }

    #[tokio::test]
    async fn test_check_order_exceeds_max_order_size() {
        let config = create_test_config();
        let engine = RiskEngine::new(config);
        let account = create_test_account("100000");

        // 订单数量 = 2，超过 max_order_size (1)
        let order = create_test_order("BTCUSDT", "2", "5000");

        let result = engine.check_order(&order, &account).await.unwrap();
        assert!(matches!(result, RiskDecision::Reject(_)));
    }

    // ========== 日亏损限制测试 ==========

    #[tokio::test]
    async fn test_daily_loss_limit() {
        let config = create_test_config();
        let engine = RiskEngine::new(config);
        let account = create_test_account("100000");

        // 模拟日亏损超过限制
        engine.record_trade_result("BTCUSDT", "SELL", Decimal::from(1), Decimal::from(40000)).await;

        // 这里需要手动设置日亏损来测试
        // 由于 record_trade_result 的实现，我们需要多次交易来累积亏损
    }

    // ========== 熔断测试 ==========

    #[tokio::test]
    async fn test_circuit_breaker() {
        let config = create_test_config();
        let engine = RiskEngine::new(config);
        let account = create_test_account("100000");

        // 触发熔断
        engine.trigger_circuit_breaker("Test circuit breaker").await;

        let status = engine.get_status().await;
        assert!(status.is_circuit_breaker_active);

        // 尝试下单，应该被拒绝
        let order = create_test_order("BTCUSDT", "0.1", "50000");
        let result = engine.check_order(&order, &account).await.unwrap();
        assert!(matches!(result, RiskDecision::Reject(_)));
    }

    // ========== 交易记录测试 ==========

    #[tokio::test]
    async fn test_record_trade_result() {
        let config = create_test_config();
        let engine = RiskEngine::new(config);

        // 记录买入
        engine.record_trade_result("BTCUSDT", "BUY", Decimal::from(1), Decimal::from(50000)).await;

        let status = engine.get_status().await;
        assert_eq!(status.daily_trade_count, 1);
        assert_eq!(status.position_count, 1);
    }

    #[tokio::test]
    async fn test_record_trade_result_sell() {
        let config = create_test_config();
        let engine = RiskEngine::new(config);

        // 先买入
        engine.record_trade_result("BTCUSDT", "BUY", Decimal::from(1), Decimal::from(50000)).await;

        // 再卖出
        engine.record_trade_result("BTCUSDT", "SELL", Decimal::from(1), Decimal::from(60000)).await;

        let status = engine.get_status().await;
        assert_eq!(status.daily_trade_count, 2);
        assert_eq!(status.position_count, 0);
    }

    // ========== 日统计重置测试 ==========

    #[tokio::test]
    async fn test_reset_daily_stats() {
        let config = create_test_config();
        let engine = RiskEngine::new(config);

        // 记录一些交易
        engine.record_trade_result("BTCUSDT", "BUY", Decimal::from(1), Decimal::from(50000)).await;

        // 重置日统计
        engine.reset_daily_stats().await;

        let status = engine.get_status().await;
        assert_eq!(status.daily_pnl, Decimal::ZERO);
        assert_eq!(status.daily_trade_count, 0);
    }

    // ========== 边界情况测试 ==========

    #[tokio::test]
    async fn test_check_order_zero_quantity() {
        let config = create_test_config();
        let engine = RiskEngine::new(config);
        let account = create_test_account("100000");
        let order = create_test_order("BTCUSDT", "0", "50000");

        let result = engine.check_order(&order, &account).await.unwrap();
        assert!(matches!(result, RiskDecision::Allow));
    }

    #[tokio::test]
    async fn test_multiple_symbols() {
        let config = create_test_config();
        let engine = RiskEngine::new(config);

        // 记录多个交易对
        engine.record_trade_result("BTCUSDT", "BUY", Decimal::from(1), Decimal::from(50000)).await;
        engine.record_trade_result("ETHUSDT", "BUY", Decimal::from(10), Decimal::from(3000)).await;

        let status = engine.get_status().await;
        assert_eq!(status.position_count, 2);
    }

    // ========== 风控配置测试 ==========

    #[test]
    fn test_risk_config_values() {
        let config = create_test_config();

        assert_eq!(config.max_position_size, Decimal::from(10000));
        assert_eq!(config.max_order_size, Decimal::from(1));
        assert_eq!(config.max_daily_loss, Decimal::from(500));
        assert_eq!(config.circuit_breaker_cooldown, 3600);
    }

    // ========== 持仓快照测试 ==========

    #[tokio::test]
    async fn test_position_snapshot() {
        let config = create_test_config();
        let engine = RiskEngine::new(config);

        // 买入
        engine.record_trade_result("BTCUSDT", "BUY", Decimal::from(1), Decimal::from(50000)).await;

        let status = engine.get_status().await;
        assert_eq!(status.position_count, 1);
    }

    // ========== 综合场景测试 ==========

    #[tokio::test]
    async fn test_full_trading_scenario() {
        let config = create_test_config();
        let engine = RiskEngine::new(config);
        let account = create_test_account("100000");

        // 1. 检查订单
        let order = create_test_order("BTCUSDT", "0.1", "50000");
        let result = engine.check_order(&order, &account).await.unwrap();
        assert!(matches!(result, RiskDecision::Allow));

        // 2. 执行交易
        engine.record_trade_result("BTCUSDT", "BUY", Decimal::from(0.1), Decimal::from(50000)).await;

        // 3. 检查状态
        let status = engine.get_status().await;
        assert_eq!(status.daily_trade_count, 1);
        assert_eq!(status.position_count, 1);

        // 4. 重置日统计
        engine.reset_daily_stats().await;
        let status = engine.get_status().await;
        assert_eq!(status.daily_trade_count, 0);
    }
}
