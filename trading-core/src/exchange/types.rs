// exchange/types.rs

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// K 线数据结构（从交易所 REST API 获取）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KlineData {
    /// K 线开始时间（UTC）
    pub timestamp: DateTime<Utc>,
    /// 交易对
    pub symbol: String,
    /// 开盘价
    pub open: Decimal,
    /// 最高价
    pub high: Decimal,
    /// 最低价
    pub low: Decimal,
    /// 收盘价
    pub close: Decimal,
    /// 成交量
    pub volume: Decimal,
    /// 成交笔数
    pub trade_count: u64,
}

/// Binance specific trade message format
#[derive(Debug, Deserialize, Clone)]
pub struct BinanceTradeMessage {
    /// Symbol
    #[serde(rename = "s")]
    pub symbol: String,

    /// Trade ID
    #[serde(rename = "t")]
    pub trade_id: u64,

    /// Price
    #[serde(rename = "p")]
    pub price: String,

    /// Quantity
    #[serde(rename = "q")]
    pub quantity: String,

    /// Trade time
    #[serde(rename = "T")]
    pub trade_time: u64,

    /// Is the buyer the market maker?
    #[serde(rename = "m")]
    pub is_buyer_maker: bool,
}

/// Binance WebSocket stream wrapper for combined streams
#[derive(Debug, Deserialize)]
pub struct BinanceStreamMessage {
    /// Stream name (e.g., "btcusdt@trade")
    #[allow(dead_code)] // Required for JSON deserialization
    pub stream: String,

    /// The actual trade data
    pub data: BinanceTradeMessage,
}

/// Binance subscription message format
#[derive(Debug, Serialize)]
pub struct BinanceSubscribeMessage {
    pub method: String,
    pub params: Vec<String>,
    pub id: u32,
}

impl BinanceSubscribeMessage {
    pub fn new(streams: Vec<String>) -> Self {
        Self {
            method: "SUBSCRIBE".to_string(),
            params: streams,
            id: 1,
        }
    }
}

// =================================================================
// 市场情绪数据结构
// =================================================================

/// 资金费率数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundingRateData {
    pub symbol: String,
    pub funding_rate: Decimal,
    pub funding_time: DateTime<Utc>,
    pub mark_price: Option<Decimal>,
}

/// 持仓量数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenInterestData {
    pub symbol: String,
    pub open_interest: Decimal,
    pub open_value: Option<Decimal>,
    pub timestamp: DateTime<Utc>,
}

/// 多空比数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongShortRatioData {
    pub symbol: String,
    pub long_ratio: Decimal,
    pub short_ratio: Decimal,
    pub ratio: Decimal,
    pub timestamp: DateTime<Utc>,
}

/// 订单簿深度数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookData {
    pub symbol: String,
    pub bids: Vec<(Decimal, Decimal)>,  // (价格, 数量)
    pub asks: Vec<(Decimal, Decimal)>,  // (价格, 数量)
    pub timestamp: DateTime<Utc>,
}

/// 大单成交数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LargeTradeData {
    pub symbol: String,
    pub price: Decimal,
    pub quantity: Decimal,
    pub quote_qty: Decimal,  // price * quantity (USDT)
    pub side: String,        // "BUY" or "SELL"
    pub timestamp: DateTime<Utc>,
}
