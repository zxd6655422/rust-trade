// exchange/traits.rs
// 交易所通用 trait 定义

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use tokio::sync::broadcast;

use super::errors::ExchangeError;
use super::types::*;
use trading_common::data::types::TickData;

/// 交易所 trait - 定义所有交易所必须实现的接口
#[async_trait]
pub trait Exchange: Send + Sync {
    // ===== 行情接口 (只读) =====

    /// 订阅实时行情 (WebSocket)
    async fn subscribe_trades(
        &self,
        symbols: &[String],
        callback: Box<dyn Fn(TickData) + Send + Sync>,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), ExchangeError>;

    // ===== 账户接口 =====

    /// 获取账户信息
    async fn get_account(&self) -> Result<AccountInfo, ExchangeError>;

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

    // ===== 用户数据流 (WebSocket) =====

    /// 订阅用户数据流 (订单更新、余额更新等)
    async fn subscribe_user_data(
        &self,
        order_callback: Box<dyn Fn(OrderUpdate) + Send + Sync>,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), ExchangeError>;

    // ===== 元信息 =====

    /// 交易所 ID
    fn exchange_id(&self) -> &str;

    /// 是否测试网
    fn is_testnet(&self) -> bool;

    /// 获取服务器时间
    async fn get_server_time(&self) -> Result<DateTime<Utc>, ExchangeError>;

    /// 获取交易对精度
    async fn get_symbol_precision(&self, symbol: &str) -> Result<SymbolPrecision, ExchangeError>;

    // ===== 合约交易接口 =====

    /// 设置杠杆倍数
    async fn set_leverage(&self, symbol: &str, leverage: u32) -> Result<(), ExchangeError>;

    /// 设置保证金模式 (逐仓/全仓)
    async fn set_margin_type(&self, symbol: &str, margin_type: MarginType) -> Result<(), ExchangeError>;

    /// 获取所有订单 (包括历史)
    async fn get_all_orders(&self, symbol: &str, limit: Option<u32>) -> Result<Vec<OrderInfo>, ExchangeError>;

    /// 获取成交历史
    async fn get_trade_history(&self, symbol: &str, limit: Option<u32>) -> Result<Vec<TradeInfo>, ExchangeError>;

    // ===== 行情数据接口 =====

    /// 获取标记价格
    async fn get_mark_price(&self, symbol: &str) -> Result<MarkPrice, ExchangeError>;

    /// 获取资金费率
    async fn get_funding_rate(&self, symbol: &str, limit: Option<u32>) -> Result<Vec<FundingRate>, ExchangeError>;

    /// 获取K线数据
    async fn get_klines(&self, symbol: &str, interval: &str, limit: Option<u32>) -> Result<Vec<Kline>, ExchangeError>;

    /// 获取订单簿深度
    async fn get_order_book(&self, symbol: &str, limit: Option<u32>) -> Result<OrderBook, ExchangeError>;

    /// 批量下单
    async fn batch_place_orders(&self, orders: Vec<BatchOrderRequest>) -> Result<Vec<BatchOrderResult>, ExchangeError>;

    /// 批量撤单
    async fn batch_cancel_orders(&self, symbol: &str, order_ids: Vec<String>) -> Result<Vec<BatchOrderResult>, ExchangeError>;

    // ===== 账户扩展接口 =====

    /// 获取合约账户信息
    async fn get_futures_account(&self) -> Result<FuturesAccountInfo, ExchangeError>;
}

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
