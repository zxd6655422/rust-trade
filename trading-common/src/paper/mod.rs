// paper/mod.rs
// Paper Trading 模拟交易模块
// 复用 backtest::Portfolio 实现虚拟交易，支持实时行情驱动

pub mod trader;

pub use trader::{
    PaperOrder, PaperOrderStatus, PaperOrderType, PaperTrader, PaperTraderConfig, PaperTraderStatus,
    SharedPaperTrader,
};
