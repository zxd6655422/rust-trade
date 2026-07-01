use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct DataInfoResponse {
    pub total_records: u64,
    pub symbols_count: u64,
    pub earliest_time: Option<String>,
    pub latest_time: Option<String>,
    pub symbol_info: Vec<SymbolInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SymbolInfo {
    pub symbol: String,
    pub records_count: u64,
    pub earliest_time: Option<String>,
    pub latest_time: Option<String>,
    pub min_price: Option<String>,
    pub max_price: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StrategyInfo {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BacktestRequest {
    pub strategy_id: String,
    pub symbol: String,
    pub data_count: i64,
    pub initial_capital: String,
    pub commission_rate: String,
    pub strategy_params: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BacktestResponse {
    pub strategy_name: String,
    pub initial_capital: String,
    pub final_value: String,
    pub total_pnl: String,
    pub return_percentage: String,
    pub total_trades: usize,
    pub winning_trades: usize,
    pub losing_trades: usize,
    pub max_drawdown: String,
    pub sharpe_ratio: String,
    pub volatility: String,
    pub win_rate: String,
    pub profit_factor: String,
    pub total_commission: String,
    pub trades: Vec<TradeInfo>,
    pub equity_curve: Vec<String>,
    pub data_source: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TradeInfo {
    pub timestamp: String,
    pub symbol: String,
    pub side: String,
    pub quantity: String,
    pub price: String,
    pub realized_pnl: Option<String>,
    pub commission: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HistoricalDataRequest {
    pub symbol: String,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TickDataResponse {
    pub timestamp: String,
    pub symbol: String,
    pub price: String,
    pub quantity: String,
    pub side: String,
}


#[derive(Debug, Serialize, Deserialize)]
pub struct StrategyCapability {
    pub id: String,
    pub name: String,
    pub description: String,
    pub supports_ohlc: bool,
    pub preferred_timeframe: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OHLCPreview {
    pub timestamp: String,
    pub symbol: String,
    pub open: String,
    pub high: String,
    pub low: String,
    pub close: String,
    pub volume: String,
    pub trade_count: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OHLCRequest {
    pub symbol: String,
    pub timeframe: String,
    pub count: u32,
}

// ============ P8: 实时行情类型 ============

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RealtimePrice {
    pub symbol: String,
    pub price: String,
    pub change_24h: Option<String>,
    pub volume_24h: Option<String>,
    pub high_24h: Option<String>,
    pub low_24h: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KlineData {
    pub timestamp: String,
    pub open: String,
    pub high: String,
    pub low: String,
    pub close: String,
    pub volume: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PriceHistoryRequest {
    pub symbol: String,
    pub timeframe: String,
    pub limit: Option<u32>,
}

// ============ P9: 持仓和交易记录类型 ============

#[derive(Debug, Serialize, Deserialize)]
pub struct PositionInfo {
    pub id: String,
    pub symbol: String,
    pub side: String,
    pub quantity: String,
    pub avg_entry_price: String,
    pub current_price: Option<String>,
    pub unrealized_pnl: Option<String>,
    pub realized_pnl: String,
    pub opened_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TradeRecord {
    pub id: String,
    pub order_id: Option<String>,
    pub symbol: String,
    pub side: String,
    pub price: String,
    pub quantity: String,
    pub commission: String,
    pub realized_pnl: Option<String>,
    pub strategy_id: Option<String>,
    pub trade_time: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TradeHistoryRequest {
    pub symbol: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PnlSummaryRequest {
    pub symbol: Option<String>,
    pub days: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PnlSummary {
    pub period_days: i32,
    pub symbol: Option<String>,
    pub total_trades: i64,
    pub winning_trades: i64,
    pub losing_trades: i64,
    pub win_rate: String,
    pub total_pnl: Option<String>,
    pub total_commission: Option<String>,
    pub best_trade: Option<String>,
    pub worst_trade: Option<String>,
    pub avg_pnl: Option<String>,
}

// ============ P10: 统计分析类型 ============

#[derive(Debug, Serialize, Deserialize)]
pub struct EquityCurvePoint {
    pub date: String,
    pub equity: String,
    pub pnl: String,
    pub cumulative_pnl: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EquityCurveRequest {
    pub symbol: Option<String>,
    pub period: Option<String>,  // "daily", "weekly", "monthly"
    pub days: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub sharpe_ratio: String,
    pub sortino_ratio: String,
    pub max_drawdown: String,
    pub max_drawdown_duration_days: i64,
    pub calmar_ratio: String,
    pub volatility: String,
    pub win_rate: String,
    pub profit_factor: String,
    pub avg_trade_duration_hours: f64,
    pub total_trades: i64,
    pub winning_trades: i64,
    pub losing_trades: i64,
    pub avg_win: String,
    pub avg_loss: String,
    pub largest_win: String,
    pub largest_loss: String,
    pub consecutive_wins: i32,
    pub consecutive_losses: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PerformanceRequest {
    pub symbol: Option<String>,
    pub days: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CommissionStats {
    pub total_commission: String,
    pub avg_commission_per_trade: String,
    pub commission_by_symbol: Vec<SymbolCommission>,
    pub commission_by_month: Vec<MonthlyCommission>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SymbolCommission {
    pub symbol: String,
    pub total_commission: String,
    pub trade_count: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MonthlyCommission {
    pub month: String,
    pub total_commission: String,
    pub trade_count: i64,
}