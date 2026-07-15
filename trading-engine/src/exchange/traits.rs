// exchange/traits.rs
// 交易所通用 trait 定义
// 分层设计：MarketDataProvider（只读数据）+ TradingOperations（交易操作）

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use tokio::sync::broadcast;

use super::errors::ExchangeError;
use super::types::*;
use trading_common::data::types::TickData;

// Re-export new types for trait usage
use super::types::{ConditionalOrderRequest, ConditionalOrderResult, IncomeRecord};

/// 只读市场数据接口（公开 API，无需认证）
///
/// 提供行情查询、K线数据、订单簿等公开市场数据接口
/// 大部分方法不需要 API Key，适合数据采集、监控等场景
#[async_trait]
pub trait MarketDataProvider: Send + Sync {
    // ===== 元信息 =====

    /// 交易所 ID
    fn exchange_id(&self) -> &str;

    /// 是否测试网
    fn is_testnet(&self) -> bool;

    // ===== 公开数据接口 =====

    /// 获取服务器时间
    async fn get_server_time(&self) -> Result<DateTime<Utc>, ExchangeError>;

    /// 获取交易对精度
    async fn get_symbol_precision(&self, symbol: &str) -> Result<SymbolPrecision, ExchangeError>;

    /// 获取行情快照 (Ticker)
    async fn get_ticker(&self, symbol: &str) -> Result<Ticker, ExchangeError>;

    /// 获取多个交易对行情快照
    async fn get_tickers(&self, symbols: &[String]) -> Result<Vec<Ticker>, ExchangeError>;

    /// 获取标记价格
    async fn get_mark_price(&self, symbol: &str) -> Result<MarkPrice, ExchangeError>;

    /// 获取资金费率
    async fn get_funding_rate(&self, symbol: &str, limit: Option<u32>) -> Result<Vec<FundingRate>, ExchangeError>;

    /// 获取K线数据
    async fn get_klines(&self, symbol: &str, interval: &str, limit: Option<u32>) -> Result<Vec<Kline>, ExchangeError>;

    /// 获取订单簿深度
    async fn get_order_book(&self, symbol: &str, limit: Option<u32>) -> Result<OrderBook, ExchangeError>;

    /// 获取最近成交
    async fn get_recent_trades(&self, symbol: &str, limit: Option<u32>) -> Result<Vec<PublicTrade>, ExchangeError>;

    // ===== WebSocket =====

    /// 订阅实时行情 (WebSocket)
    async fn subscribe_trades(
        &self,
        symbols: &[String],
        callback: Box<dyn Fn(TickData) + Send + Sync>,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), ExchangeError>;
}

/// 认证交易操作接口（需要 API Key）
///
/// 提供下单、撤单、持仓查询等交易操作接口
/// 所有方法都需要 API Key 认证，涉及资金操作
#[async_trait]
pub trait TradingOperations: Send + Sync {
    // ===== 账户接口 =====

    /// 获取现货账户信息
    async fn get_account(&self) -> Result<AccountInfo, ExchangeError>;

    /// 获取合约账户信息
    async fn get_futures_account(&self) -> Result<FuturesAccountInfo, ExchangeError>;

    /// 获取持仓信息
    async fn get_position(&self, symbol: &str) -> Result<PositionInfo, ExchangeError>;

    /// 获取所有持仓
    async fn get_positions(&self) -> Result<Vec<PositionInfo>, ExchangeError>;

    // ===== 订单接口 =====

    /// 下单
    async fn place_order(&self, order: OrderRequest) -> Result<OrderResult, ExchangeError>;

    /// 撤单
    async fn cancel_order(&self, symbol: &str, order_id: &str) -> Result<(), ExchangeError>;

    /// 批量撤单
    async fn cancel_all_orders(&self, symbol: Option<&str>) -> Result<(), ExchangeError>;

    /// 获取未成交订单
    async fn get_open_orders(&self, symbol: Option<&str>) -> Result<Vec<OrderInfo>, ExchangeError>;

    /// 获取订单详情
    async fn get_order(&self, symbol: &str, order_id: &str) -> Result<OrderInfo, ExchangeError>;

    /// 获取所有订单 (包括历史)
    async fn get_all_orders(&self, symbol: &str, limit: Option<u32>) -> Result<Vec<OrderInfo>, ExchangeError>;

    /// 获取成交历史
    async fn get_trade_history(&self, symbol: &str, limit: Option<u32>) -> Result<Vec<TradeInfo>, ExchangeError>;

    /// 批量下单
    async fn batch_place_orders(&self, orders: Vec<BatchOrderRequest>) -> Result<Vec<BatchOrderResult>, ExchangeError>;

    /// 批量撤单
    async fn batch_cancel_orders(&self, symbol: &str, order_ids: Vec<String>) -> Result<Vec<BatchOrderResult>, ExchangeError>;

    // ===== 合约配置接口 =====

    /// 设置杠杆倍数
    async fn set_leverage(&self, symbol: &str, leverage: u32) -> Result<(), ExchangeError>;

    /// 设置保证金模式 (逐仓/全仓)
    async fn set_margin_type(&self, symbol: &str, margin_type: MarginType) -> Result<(), ExchangeError>;

    // ===== 用户数据流 =====

    /// 订阅用户数据流 (订单更新、余额更新等)
    async fn subscribe_user_data(
        &self,
        order_callback: Box<dyn Fn(OrderUpdate) + Send + Sync>,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), ExchangeError>;

    // ===== 条件单接口 (止盈止损) =====

    /// 下条件单（止盈止损）
    ///
    /// Binance: POST /fapi/v1/algo/order (algoType=CONDITIONAL)
    /// OKX: POST /api/v5/trade/order (ordType=conditional)
    async fn place_conditional_order(
        &self,
        order: ConditionalOrderRequest,
    ) -> Result<ConditionalOrderResult, ExchangeError>;

    /// 撤销条件单
    ///
    /// Binance: DELETE /fapi/v1/algo/order
    /// OKX: POST /api/v5/trade/cancel-algos
    async fn cancel_conditional_order(
        &self,
        symbol: &str,
        strategy_id: &str,
    ) -> Result<(), ExchangeError>;

    /// 查询条件单列表
    ///
    /// Binance: GET /fapi/v1/algo/openOrders
    /// OKX: GET /api/v5/trade/orders-algo-pending
    async fn get_conditional_orders(
        &self,
        symbol: Option<&str>,
    ) -> Result<Vec<ConditionalOrderResult>, ExchangeError>;

    // ===== 收入查询接口 =====

    /// 查询已实现盈亏历史
    ///
    /// Binance: GET /fapi/v1/income?incomeType=REALIZED_PNL
    /// OKX: GET /api/v5/trade/fills-history
    async fn get_income_history(
        &self,
        symbol: Option<&str>,
        income_type: Option<&str>,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
        limit: Option<u32>,
    ) -> Result<Vec<IncomeRecord>, ExchangeError>;
}

/// 组合 trait，保持向后兼容
///
/// 同时实现 MarketDataProvider 和 TradingOperations 的类型自动实现此 trait
/// 用于需要完整交易所功能的场景（如交易引擎）
#[async_trait]
pub trait Exchange: MarketDataProvider + TradingOperations {
    /// 获取 AccountProvider 引用，用于账户信息查询
    fn as_account_provider(&self) -> &dyn trading_common::data::account_types::AccountProvider;
}

// 注意：Exchange 实现在各适配器中手动提供（因为需要 as_account_provider 方法）

/// 交易对精度信息
#[derive(Debug, Clone)]
pub struct SymbolPrecision {
    pub symbol: String,
    pub base_asset_precision: u32,
    pub quote_asset_precision: u32,
    pub min_quantity: Decimal,
    pub max_quantity: Decimal,
    pub min_notional: Decimal,
    pub step_size: Decimal,
    pub tick_size: Decimal,
}
