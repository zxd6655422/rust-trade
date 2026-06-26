// exchange/errors.rs
// 交易所错误类型

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ExchangeError {
    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("WebSocket error: {0}")]
    WebSocketError(String),

    #[error("API error: {code} - {message}")]
    ApiError { code: i64, message: String },

    #[error("Authentication error: {0}")]
    AuthenticationError(String),

    #[error("Invalid symbol: {0}")]
    InvalidSymbol(String),

    #[error("Invalid order: {0}")]
    InvalidOrder(String),

    #[error("Insufficient balance: {0}")]
    InsufficientBalance(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Signature error: {0}")]
    SignatureError(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Order not found: {0}")]
    OrderNotFound(String),

    #[error("Position not found: {0}")]
    PositionNotFound(String),

    #[error("Testnet not supported: {0}")]
    TestnetNotSupported(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Timeout error: {0}")]
    TimeoutError(String),

    #[error("Connection closed: {0}")]
    ConnectionClosed(String),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

impl From<reqwest::Error> for ExchangeError {
    fn from(err: reqwest::Error) -> Self {
        ExchangeError::NetworkError(err.to_string())
    }
}

impl From<serde_json::Error> for ExchangeError {
    fn from(err: serde_json::Error) -> Self {
        ExchangeError::ParseError(err.to_string())
    }
}

impl From<tokio_tungstenite::tungstenite::Error> for ExchangeError {
    fn from(err: tokio_tungstenite::tungstenite::Error) -> Self {
        ExchangeError::WebSocketError(err.to_string())
    }
}

impl From<redis::RedisError> for ExchangeError {
    fn from(err: redis::RedisError) -> Self {
        ExchangeError::NetworkError(format!("Redis error: {}", err))
    }
}
