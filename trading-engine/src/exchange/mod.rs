// exchange/mod.rs
// 交易所模块

pub mod adapters;
pub mod errors;
pub mod traits;
pub mod types;

pub use adapters::{BinanceAdapter, BinanceSpotAdapter, OkxAdapter, RedisDataSource, RedisDataSourceConfig};
pub use errors::ExchangeError;
pub use traits::Exchange;

/// 交易所工厂
pub struct ExchangeFactory;

impl ExchangeFactory {
    /// 根据配置创建交易所适配器
    ///
    /// 支持的 exchange_id:
    /// - "binance"       → Binance USDⓈ-M 合约
    /// - "binance-spot"  → Binance 现货
    /// - "okx"           → OKX 合约
    pub fn create(
        exchange_id: &str,
        testnet: bool,
        api_key: &str,
        api_secret: &str,
        passphrase: Option<&str>,
    ) -> Result<Box<dyn Exchange>, ExchangeError> {
        match exchange_id {
            "binance" => {
                let config = crate::exchange::adapters::binance_adapter::BinanceConfig {
                    api_key: api_key.to_string(),
                    api_secret: api_secret.to_string(),
                    testnet,
                    recv_window: 5000,
                    timeout: std::time::Duration::from_secs(10),
                };
                let adapter = BinanceAdapter::new(config)?;
                Ok(Box::new(adapter))
            }
            "binance-spot" => {
                let config = crate::exchange::adapters::binance_spot_adapter::BinanceSpotConfig {
                    api_key: api_key.to_string(),
                    api_secret: api_secret.to_string(),
                    testnet,
                    recv_window: 5000,
                    timeout: std::time::Duration::from_secs(10),
                };
                let adapter = BinanceSpotAdapter::new(config)?;
                Ok(Box::new(adapter))
            }
            "okx" => {
                let config = crate::exchange::adapters::okx_adapter::OkxConfig {
                    api_key: api_key.to_string(),
                    api_secret: api_secret.to_string(),
                    passphrase: passphrase.unwrap_or("").to_string(),
                    simulated: testnet,
                };
                let adapter = OkxAdapter::new(config)?;
                Ok(Box::new(adapter))
            }
            _ => Err(ExchangeError::Unknown(format!(
                "Unsupported exchange: {}",
                exchange_id
            ))),
        }
    }
}
