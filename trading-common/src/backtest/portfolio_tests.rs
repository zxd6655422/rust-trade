// Portfolio 单元测试
// 测试做多、做空、盈亏计算等核心逻辑

#[cfg(test)]
mod tests {
    use crate::backtest::portfolio::{Portfolio, PositionSide};
    use crate::data::types::TradeSide;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    /// 创建测试用 Portfolio（默认无滑点，保持原有测试逻辑）
    fn create_portfolio(initial_capital: &str) -> Portfolio {
        Portfolio::new(Decimal::from_str(initial_capital).unwrap())
            .with_commission_rate(Decimal::from_str("0.001").unwrap()) // 0.1%
            .with_slippage_pct(Decimal::ZERO)
    }

    // ========== 基础功能测试 ==========

    #[test]
    fn test_portfolio_initialization() {
        let portfolio = create_portfolio("10000");
        assert_eq!(portfolio.initial_capital, Decimal::from(10000));
        assert_eq!(portfolio.cash, Decimal::from(10000));
        assert!(portfolio.positions.is_empty());
        assert!(portfolio.trades.is_empty());
    }

    #[test]
    fn test_total_value_no_positions() {
        let portfolio = create_portfolio("10000");
        assert_eq!(portfolio.total_value(), Decimal::from(10000));
    }

    // ========== 做多测试 ==========

    #[test]
    fn test_buy_basic() {
        let mut portfolio = create_portfolio("100000");

        // 买入 1 BTC @ 50000
        let result = portfolio.execute_buy("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(50000));
        assert!(result.is_ok());

        // 验证现金减少
        // cost = 50000, commission = 50, total = 50050
        assert_eq!(portfolio.cash, Decimal::from(49950));

        // 验证持仓
        let position = portfolio.positions.get("BTCUSDT").unwrap();
        assert_eq!(position.quantity, Decimal::from(1));
        assert_eq!(position.avg_price, Decimal::from(50000));
        assert_eq!(position.side, PositionSide::Long);
    }

    #[test]
    fn test_buy_insufficient_funds() {
        let mut portfolio = create_portfolio("1000");

        // 尝试买入 1 BTC @ 50000
        let result = portfolio.execute_buy("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(50000));
        assert!(result.is_err());
        assert_eq!(portfolio.cash, Decimal::from(1000)); // 资金不变
    }

    #[test]
    fn test_buy_multiple_times() {
        let mut portfolio = create_portfolio("200000");

        // 第一次买入 1 BTC @ 50000
        portfolio.execute_buy("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(50000)).unwrap();

        // 第二次买入 1 BTC @ 60000
        portfolio.execute_buy("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(60000)).unwrap();

        // 验证平均价格: (50000 + 60000) / 2 = 55000
        let position = portfolio.positions.get("BTCUSDT").unwrap();
        assert_eq!(position.quantity, Decimal::from(2));
        assert_eq!(position.avg_price, Decimal::from(55000));
    }

    #[test]
    fn test_sell_basic() {
        let mut portfolio = create_portfolio("100000");

        // 买入 1 BTC @ 50000
        portfolio.execute_buy("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(50000)).unwrap();

        // 卖出 1 BTC @ 60000
        let result = portfolio.execute_sell("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(60000));
        assert!(result.is_ok());

        // 验证盈亏
        // 买入成本: 50000 + 50 = 50050
        // 卖出收入: 60000 - 60 = 59940
        // 盈利: 59940 - 50050 = 9890
        let expected_cash = Decimal::from(100000) - Decimal::from(50050) + Decimal::from(59940);
        assert_eq!(portfolio.cash, expected_cash);

        // 验证持仓已清空
        assert!(!portfolio.positions.contains_key("BTCUSDT"));
    }

    #[test]
    fn test_sell_partial() {
        let mut portfolio = create_portfolio("200000");

        // 买入 2 BTC @ 50000
        portfolio.execute_buy("BTCUSDT".to_string(), Decimal::from(2), Decimal::from(50000)).unwrap();

        // 卖出 1 BTC @ 60000
        portfolio.execute_sell("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(60000)).unwrap();

        // 验证持仓
        let position = portfolio.positions.get("BTCUSDT").unwrap();
        assert_eq!(position.quantity, Decimal::from(1));
    }

    #[test]
    fn test_sell_no_position() {
        let mut portfolio = create_portfolio("10000");

        let result = portfolio.execute_sell("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(50000));
        assert!(result.is_err());
    }

    #[test]
    fn test_sell_exceeds_position() {
        let mut portfolio = create_portfolio("100000");

        // 买入 1 BTC @ 50000
        portfolio.execute_buy("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(50000)).unwrap();

        // 尝试卖出 2 BTC
        let result = portfolio.execute_sell("BTCUSDT".to_string(), Decimal::from(2), Decimal::from(60000));
        assert!(result.is_err());
    }

    // ========== 做空测试 ==========

    #[test]
    fn test_short_open_basic() {
        let mut portfolio = create_portfolio("10000");

        // 开空 1 BTC @ 50000
        let result = portfolio.execute_short_open("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(50000));
        assert!(result.is_ok());

        // 验证现金增加
        // proceeds = 50000, commission = 50, net = 49950
        // 初始 10000 + 49950 = 59950
        assert_eq!(portfolio.cash, Decimal::from(59950));

        // 验证持仓
        let position = portfolio.positions.get("BTCUSDT").unwrap();
        assert_eq!(position.quantity, Decimal::from(1));
        assert_eq!(position.avg_price, Decimal::from(50000));
        assert_eq!(position.side, PositionSide::Short);
    }

    #[test]
    fn test_short_open_multiple() {
        let mut portfolio = create_portfolio("10000");

        // 第一次开空 1 BTC @ 50000
        portfolio.execute_short_open("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(50000)).unwrap();

        // 第二次开空 1 BTC @ 60000
        portfolio.execute_short_open("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(60000)).unwrap();

        // 验证平均价格: (50000 + 60000) / 2 = 55000
        let position = portfolio.positions.get("BTCUSDT").unwrap();
        assert_eq!(position.quantity, Decimal::from(2));
        assert_eq!(position.avg_price, Decimal::from(55000));
        assert_eq!(position.side, PositionSide::Short);
    }

    #[test]
    fn test_short_close_profit() {
        let mut portfolio = create_portfolio("10000");

        // 开空 1 BTC @ 50000
        portfolio.execute_short_open("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(50000)).unwrap();

        // 平空 1 BTC @ 40000 (价格下跌，盈利)
        let result = portfolio.execute_short_close("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(40000));
        assert!(result.is_ok());

        // 验证持仓已清空
        assert!(!portfolio.positions.contains_key("BTCUSDT"));
    }

    #[test]
    fn test_short_close_loss() {
        let mut portfolio = create_portfolio("10000");

        // 开空 1 BTC @ 50000
        portfolio.execute_short_open("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(50000)).unwrap();

        // 平空 1 BTC @ 60000 (价格上涨，亏损)
        let result = portfolio.execute_short_close("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(60000));
        assert!(result.is_ok());
    }

    #[test]
    fn test_short_close_no_position() {
        let mut portfolio = create_portfolio("10000");

        let result = portfolio.execute_short_close("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(50000));
        assert!(result.is_err());
    }

    #[test]
    fn test_short_close_exceeds_position() {
        let mut portfolio = create_portfolio("10000");

        // 开空 1 BTC @ 50000
        portfolio.execute_short_open("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(50000)).unwrap();

        // 尝试平空 2 BTC
        let result = portfolio.execute_short_close("BTCUSDT".to_string(), Decimal::from(2), Decimal::from(40000));
        assert!(result.is_err());
    }

    // ========== 价格更新测试 ==========

    #[test]
    fn test_update_price_long() {
        let mut portfolio = create_portfolio("100000");

        // 买入 1 BTC @ 50000
        portfolio.execute_buy("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(50000)).unwrap();

        // 更新价格到 60000
        portfolio.update_price("BTCUSDT", Decimal::from(60000));

        let position = portfolio.positions.get("BTCUSDT").unwrap();
        assert_eq!(position.market_value, Decimal::from(60000));
        assert_eq!(position.unrealized_pnl, Decimal::from(10000));
    }

    #[test]
    fn test_update_price_short() {
        let mut portfolio = create_portfolio("10000");

        // 开空 1 BTC @ 50000
        portfolio.execute_short_open("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(50000)).unwrap();

        // 更新价格到 40000 (价格下跌，盈利)
        portfolio.update_price("BTCUSDT", Decimal::from(40000));

        let position = portfolio.positions.get("BTCUSDT").unwrap();
        assert_eq!(position.unrealized_pnl, Decimal::from(10000));
    }

    #[test]
    fn test_update_price_short_loss() {
        let mut portfolio = create_portfolio("10000");

        // 开空 1 BTC @ 50000
        portfolio.execute_short_open("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(50000)).unwrap();

        // 更新价格到 60000 (价格上涨，亏损)
        portfolio.update_price("BTCUSDT", Decimal::from(60000));

        let position = portfolio.positions.get("BTCUSDT").unwrap();
        assert_eq!(position.unrealized_pnl, Decimal::from(-10000));
    }

    // ========== 总价值测试 ==========

    #[test]
    fn test_total_value_with_long() {
        let mut portfolio = create_portfolio("100000");

        // 买入 1 BTC @ 50000
        portfolio.execute_buy("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(50000)).unwrap();

        // 更新价格到 60000
        portfolio.update_price("BTCUSDT", Decimal::from(60000));

        // 总价值 = 现金 + 持仓市值
        // 现金 = 100000 - 50050 = 49950
        // 持仓市值 = 60000
        // 总价值 = 49950 + 60000 = 109950
        let total = portfolio.total_value();
        assert_eq!(total, Decimal::from(109950));
    }

    #[test]
    fn test_total_value_with_short() {
        let mut portfolio = create_portfolio("10000");

        // 开空 1 BTC @ 50000
        portfolio.execute_short_open("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(50000)).unwrap();

        // 更新价格到 40000
        portfolio.update_price("BTCUSDT", Decimal::from(40000));

        // 总价值 = 现金 + 空头盈亏
        // 现金 = 10000 + 49950 = 59950
        // 空头盈亏 = 10000
        // 总价值 = 59950 + 10000 = 69950
        let total = portfolio.total_value();
        assert_eq!(total, Decimal::from(69950));
    }

    // ========== 交易记录测试 ==========

    #[test]
    fn test_trade_history() {
        let mut portfolio = create_portfolio("100000");

        // 买入
        portfolio.execute_buy("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(50000)).unwrap();

        // 卖出
        portfolio.execute_sell("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(60000)).unwrap();

        // 验证交易记录
        assert_eq!(portfolio.trades.len(), 2);
        assert_eq!(portfolio.trades[0].side, TradeSide::Buy);
        assert_eq!(portfolio.trades[1].side, TradeSide::Sell);
    }

    #[test]
    fn test_commission_tracking() {
        let mut portfolio = create_portfolio("100000");

        // 买入 1 BTC @ 50000
        portfolio.execute_buy("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(50000)).unwrap();

        // 验证手续费
        // commission = 50000 * 0.001 = 50
        assert_eq!(portfolio.trades[0].commission, Decimal::from(50));
    }

    // ========== 边界情况测试 ==========

    #[test]
    fn test_buy_zero_quantity() {
        let mut portfolio = create_portfolio("10000");

        let result = portfolio.execute_buy("BTCUSDT".to_string(), Decimal::ZERO, Decimal::from(50000));
        // 零数量应该成功，但不产生实际影响
        assert!(result.is_ok());
    }

    #[test]
    fn test_sell_zero_quantity() {
        let mut portfolio = create_portfolio("100000");

        // 先买入
        portfolio.execute_buy("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(50000)).unwrap();

        let result = portfolio.execute_sell("BTCUSDT".to_string(), Decimal::ZERO, Decimal::from(60000));
        assert!(result.is_ok());
    }

    #[test]
    fn test_multiple_symbols() {
        let mut portfolio = create_portfolio("200000");

        // 买入 BTC
        portfolio.execute_buy("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(50000)).unwrap();

        // 买入 ETH
        portfolio.execute_buy("ETHUSDT".to_string(), Decimal::from(10), Decimal::from(3000)).unwrap();

        // 验证持仓数量
        assert_eq!(portfolio.positions.len(), 2);
        assert!(portfolio.positions.contains_key("BTCUSDT"));
        assert!(portfolio.positions.contains_key("ETHUSDT"));
    }

    // ========== 滑点测试 ==========

    #[test]
    fn test_slippage_buy_price_increased() {
        // 买入滑点：实际成交价高于报价
        let mut portfolio = Portfolio::new(Decimal::from_str("100000").unwrap())
            .with_commission_rate(Decimal::ZERO)
            .with_slippage_pct(Decimal::from_str("0.001").unwrap()); // 0.1%

        portfolio.execute_buy("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(50000)).unwrap();

        // 实际成交价 = 50000 * (1 + 0.001) = 50050
        let position = portfolio.positions.get("BTCUSDT").unwrap();
        assert_eq!(position.avg_price, Decimal::from(50050));
        // 现金 = 100000 - 50050 = 49950
        assert_eq!(portfolio.cash, Decimal::from(49950));
        // 滑点成本 = 50 * 1 = 50
        assert_eq!(portfolio.total_slippage_cost, Decimal::from(50));
    }

    #[test]
    fn test_slippage_sell_price_decreased() {
        // 卖出滑点：实际成交价低于报价
        let mut portfolio = Portfolio::new(Decimal::from_str("100000").unwrap())
            .with_commission_rate(Decimal::ZERO)
            .with_slippage_pct(Decimal::from_str("0.001").unwrap()); // 0.1%

        // 先买入（无滑点影响买入后再卖）
        portfolio.execute_buy("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(50000)).unwrap();
        let cash_after_buy = portfolio.cash;

        portfolio.execute_sell("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(60000)).unwrap();

        // 卖出实际成交价 = 60000 * (1 - 0.001) = 59940
        // 现金 = cash_after_buy + 59940
        assert_eq!(portfolio.cash, cash_after_buy + Decimal::from(59940));
        assert!(portfolio.total_slippage_cost > Decimal::ZERO);
    }

    #[test]
    fn test_slippage_zero_disabled() {
        // slippage_pct = 0 时不影响价格
        let mut portfolio = Portfolio::new(Decimal::from_str("100000").unwrap())
            .with_commission_rate(Decimal::ZERO)
            .with_slippage_pct(Decimal::ZERO);

        portfolio.execute_buy("BTCUSDT".to_string(), Decimal::from(1), Decimal::from(50000)).unwrap();

        let position = portfolio.positions.get("BTCUSDT").unwrap();
        assert_eq!(position.avg_price, Decimal::from(50000));
        assert_eq!(portfolio.cash, Decimal::from(50000));
        assert_eq!(portfolio.total_slippage_cost, Decimal::ZERO);
    }
}
