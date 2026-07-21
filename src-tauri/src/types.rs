use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============ P11: 高级回测类型 ============

/// 多时间框架回测请求
#[derive(Debug, Serialize, Deserialize)]
pub struct MultiTimeframeBacktestRequest {
    pub strategy: String,
    pub symbol: String,
    pub capital: f64,
    pub data_count: i64,
    pub commission_rate: f64,
    pub strategy_params: Option<HashMap<String, String>>,
}

/// 滚动前进测试请求
#[derive(Debug, Serialize, Deserialize)]
pub struct WalkForwardRequest {
    pub strategy: String,
    pub symbol: String,
    pub capital: f64,
    pub commission_rate: f64,
    pub train_candles: usize,
    pub test_candles: usize,
    pub step_candles: usize,
    pub data_count: u32,
    pub strategy_params: Option<HashMap<String, String>>,
}

/// 样本外测试请求
#[derive(Debug, Serialize, Deserialize)]
pub struct OutOfSampleRequest {
    pub strategy: String,
    pub symbol: String,
    pub capital: f64,
    pub commission_rate: f64,
    pub train_ratio: f64,
    pub data_count: u32,
    pub strategy_params: Option<HashMap<String, String>>,
}

/// 多交易对回测请求
#[derive(Debug, Serialize, Deserialize)]
pub struct MultiSymbolBacktestRequest {
    pub strategy: String,
    pub symbols: Vec<String>,
    pub capital: f64,
    pub commission_rate: f64,
    pub data_count: u32,
    pub market_state_window: usize,
    pub strategy_params: Option<HashMap<String, String>>,
}

/// 市场状态分析请求
#[derive(Debug, Serialize, Deserialize)]
pub struct MarketStateAnalysisRequest {
    pub symbol: String,
    pub data_count: u32,
    pub window: usize,
}

/// 滚动前进测试轮次摘要
#[derive(Debug, Serialize, Deserialize)]
pub struct WalkForwardRoundSummary {
    pub round: usize,
    pub train_start: String,
    pub train_end: String,
    pub test_start: String,
    pub test_end: String,
    pub train_return_pct: String,
    pub train_sharpe: String,
    pub train_trades: usize,
    pub test_return_pct: String,
    pub test_sharpe: String,
    pub test_trades: usize,
    pub test_win_rate: String,
    pub test_max_drawdown: String,
    pub overfit_ratio: String,
}

/// 滚动前进测试结果
#[derive(Debug, Serialize, Deserialize)]
pub struct WalkForwardResult {
    pub total_rounds: usize,
    pub profitable_rounds: usize,
    pub overall_test_return_pct: String,
    pub overall_test_sharpe: String,
    pub overall_test_max_drawdown: String,
    pub overall_test_win_rate: String,
    pub avg_overfit_ratio: String,
    pub is_overfit: bool,
    pub rounds: Vec<WalkForwardRoundSummary>,
}

/// 样本外测试结果
#[derive(Debug, Serialize, Deserialize)]
pub struct OutOfSampleResult {
    pub train_return_pct: String,
    pub train_sharpe: String,
    pub train_max_drawdown: String,
    pub train_win_rate: String,
    pub train_trades: usize,
    pub train_profit_factor: String,
    pub test_return_pct: String,
    pub test_sharpe: String,
    pub test_max_drawdown: String,
    pub test_win_rate: String,
    pub test_trades: usize,
    pub test_profit_factor: String,
    pub overfit_ratio: String,
    pub is_overfit: bool,
}

/// 多交易对回测中单个 symbol 的结果
#[derive(Debug, Serialize, Deserialize)]
pub struct SymbolBacktestResultItem {
    pub symbol: String,
    pub return_pct: String,
    pub sharpe: String,
    pub win_rate: String,
    pub max_drawdown: String,
    pub total_trades: usize,
    pub profit_factor: String,
    pub market_state: String,
    pub data_quality: String,
}

/// 多交易对回测结果
#[derive(Debug, Serialize, Deserialize)]
pub struct MultiSymbolBacktestResult {
    pub total_symbols: usize,
    pub profitable_symbols: usize,
    pub losing_symbols: usize,
    pub avg_return_pct: String,
    pub avg_sharpe: String,
    pub avg_win_rate: String,
    pub avg_max_drawdown: String,
    pub total_trades: usize,
    pub best_symbol: String,
    pub best_return_pct: String,
    pub worst_symbol: String,
    pub worst_return_pct: String,
    pub cross_symbol_correlation: String,
    pub symbols: Vec<SymbolBacktestResultItem>,
}

/// 市场状态分析结果
#[derive(Debug, Serialize, Deserialize)]
pub struct MarketStateResult {
    pub symbol: String,
    pub total_candles: usize,
    pub analysis_window: usize,
    pub state_distribution: HashMap<String, String>,
    pub avg_volatility: String,
    pub avg_trend_strength: String,
    pub trend_ratio: String,
    pub ranging_ratio: String,
    pub data_quality_score: String,
    pub summary: String,
}

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
    pub total_volume_usd: String,
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

/// 现货资产余额（返回给前端）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetBalanceItem {
    pub asset: String,
    pub total: String,
    pub available: String,
    pub frozen: String,
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
    pub exchange: Option<String>,
    #[serde(alias = "marketType")]
    pub market_type: Option<String>,
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
    pub exchange: Option<String>,
    #[serde(alias = "marketType")]
    pub market_type: Option<String>,
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

// ===== Paper Trading 类型 =====

/// Paper Trading 启动配置
#[derive(Debug, Serialize, Deserialize)]
pub struct PaperStartRequest {
    /// 初始资金 (USDT)
    pub initial_capital: Option<String>,
    /// 手续费率 (如 "0.001" = 0.1%)
    pub commission_rate: Option<String>,
    /// 滑点百分比 (如 "0.0001" = 0.01%)
    pub slippage_pct: Option<String>,
    /// 监控的交易对
    pub symbols: Option<Vec<String>>,
}

/// Paper Trading 手动下单请求
#[derive(Debug, Serialize, Deserialize)]
pub struct PaperOrderRequest {
    /// 交易对
    pub symbol: String,
    /// 方向: "buy" / "sell"
    pub side: String,
    /// 数量
    pub quantity: String,
    /// 订单类型: "market" / "limit" / "stop_loss" / "take_profit"
    pub order_type: Option<String>,
    /// 价格 (限价/止损/止盈单)
    pub price: Option<String>,
}

/// Paper Trading 状态响应
#[derive(Debug, Serialize, Deserialize)]
pub struct PaperStatusResponse {
    pub running: bool,
    pub initial_capital: String,
    pub cash: String,
    pub total_value: String,
    pub total_pnl: String,
    pub total_pnl_pct: String,
    pub realized_pnl: String,
    pub unrealized_pnl: String,
    pub total_commission: String,
    pub total_trades: usize,
    pub win_rate: String,
    pub positions: Vec<PaperPositionResponse>,
    pub pending_orders: usize,
    pub latest_prices: std::collections::HashMap<String, String>,
    pub started_at: Option<String>,
}

/// Paper Trading 持仓响应
#[derive(Debug, Serialize, Deserialize)]
pub struct PaperPositionResponse {
    pub symbol: String,
    pub side: String,
    pub quantity: String,
    pub avg_price: String,
    pub current_price: String,
    pub market_value: String,
    pub unrealized_pnl: String,
    pub unrealized_pnl_pct: String,
}

/// Paper Trading 交易记录响应
#[derive(Debug, Serialize, Deserialize)]
pub struct PaperTradeResponse {
    pub order_id: String,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub quantity: String,
    pub price: Option<String>,
    pub status: String,
    pub filled_price: Option<String>,
    pub commission: String,
    pub created_at: String,
    pub filled_at: Option<String>,
    pub reject_reason: Option<String>,
}

/// Trading Core 服务状态响应
#[derive(Debug, Serialize, Deserialize)]
pub struct TradingCoreStatusResponse {
    pub status: String,
    pub database: bool,
}

// ============ 策略实时分析类型 ============

/// 策略分析请求
#[derive(Debug, Serialize, Deserialize)]
pub struct StrategyAnalysisRequest {
    pub symbol: String,
    pub strategy_id: Option<String>,  // 默认 "trend"
}

/// 单个时间框架的分析结果
#[derive(Debug, Serialize, Deserialize)]
pub struct TimeframeAnalysis {
    pub timeframe: String,       // "4h", "1h", "15m"
    pub direction: String,       // "bullish", "bearish", "neutral"
    pub confidence: String,      // "0.0" ~ "1.0"
    pub description: String,     // 人类可读说明
}

/// 策略分析结果
#[derive(Debug, Serialize, Deserialize)]
pub struct StrategyAnalysisResult {
    pub symbol: String,
    pub strategy_id: String,
    pub strategy_name: String,
    pub timeframes: Vec<TimeframeAnalysis>,
    pub overall_direction: String,   // "bullish", "bearish", "neutral"
    pub overall_confidence: String,  // "0.0" ~ "1.0"
    pub entry_allowed: bool,
    pub entry_direction: Option<String>,  // "long", "short"
    pub analysis_time: String,       // ISO timestamp
}

/// 信号历史请求
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalHistoryRequest {
    pub symbol: Option<String>,
    pub strategy_id: Option<String>,
    pub limit: Option<i32>,
}

/// 信号统计请求
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalStatsRequest {
    pub table: String,
    pub symbol: Option<String>,
    pub strategy_id: Option<String>,
}

/// 单条信号记录
#[derive(Debug, Serialize, Deserialize)]
pub struct SignalRecord {
    pub id: String,
    pub timestamp: String,
    pub symbol: String,
    pub direction: String,      // "buy", "sell"
    pub price: String,
    pub outcome: Option<String>, // "win", "loss", null(未平仓)
    pub pnl: Option<String>,
}

/// 信号统计
#[derive(Debug, Serialize, Deserialize)]
pub struct SignalStats {
    pub total_signals: i64,
    pub confirmed: i64,
    pub invalidated: i64,
    pub expired: i64,
    pub pending: i64,
    pub win_rate: String,
    pub avg_return: String,
}

/// 信号历史结果
#[derive(Debug, Serialize, Deserialize)]
pub struct SignalHistoryResult {
    pub signals: Vec<SignalRecord>,
    pub stats: SignalStats,
}

/// 交易对配置
#[derive(Debug, Serialize, Deserialize)]
pub struct SymbolConfig {
    pub symbol: String,
    pub enabled: bool,
}

/// 策略调度器状态
#[derive(Debug, Serialize, Deserialize)]
pub struct SchedulerStatus {
    pub is_running: bool,
    pub is_paused: bool,
    pub strategy_id: String,
}