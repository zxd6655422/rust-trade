//! 交易所公开 API 模块
//!
//! 仅包含公开 API（不需要 API Key）：
//! - 获取实时价格（Ticker）
//!
//! 注意：账户信息、下单等需要 API Key 的功能由 trading-engine 负责

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;

// =================================================================
// 公开 API（不需要 API Key）
// =================================================================

/// Ticker 价格响应
#[derive(Debug, Clone, Deserialize)]
pub struct TickerPrice {
    pub symbol: String,
    pub price: String,
    #[serde(default)]
    pub time: i64,
}

/// 获取实时价格（公开 API，无需 API Key）
///
/// 根据 market_type 自动选择 API：
/// - spot: /api/v3/ticker/price
/// - futures: /fapi/v2/ticker/price
pub async fn get_ticker_price(symbol: &str, market_type: &str) -> Result<f64> {
    let url = match market_type {
        "spot" => format!("https://api.binance.com/api/v3/ticker/price?symbol={}", symbol),
        _ => format!("https://fapi.binance.com/fapi/v2/ticker/price?symbol={}", symbol),
    };

    let client = Client::new();
    let resp = client.get(&url).send().await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("Ticker API error {}: {}", status, body));
    }

    let ticker: TickerPrice = resp.json().await?;
    let price: f64 = ticker.price.parse()
        .map_err(|_| anyhow!("Invalid price format: {}", ticker.price))?;

    Ok(price)
}

/// 批量获取多个交易对的实时价格（公开 API）
///
/// 根据 market_type 自动选择 API：
/// - spot: /api/v3/ticker/price
/// - futures: /fapi/v2/ticker/price
pub async fn get_all_ticker_prices(market_type: &str) -> Result<HashMap<String, f64>> {
    let url = match market_type {
        "spot" => "https://api.binance.com/api/v3/ticker/price",
        _ => "https://fapi.binance.com/fapi/v2/ticker/price",
    };

    let client = Client::new();
    let resp = client.get(url).send().await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("Ticker API error {}: {}", status, body));
    }

    let tickers: Vec<TickerPrice> = resp.json().await?;
    let mut prices = HashMap::new();

    for ticker in tickers {
        if let Ok(price) = ticker.price.parse::<f64>() {
            prices.insert(ticker.symbol, price);
        }
    }

    Ok(prices)
}
