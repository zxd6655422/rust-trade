// src/types/backtest.ts
export interface DataInfoResponse {
  total_records: number;
  symbols_count: number;
  earliest_time?: string;
  latest_time?: string;
  symbol_info: SymbolInfo[];
}

export interface SymbolInfo {
  symbol: string;
  records_count: number;
  earliest_time?: string;
  latest_time?: string;
  min_price?: string;
  max_price?: string;
}

export interface StrategyInfo {
  id: string;
  name: string;
  description: string;
}

export interface BacktestRequest {
  strategy_id: string;
  symbol: string;
  data_count: number;
  initial_capital: string;
  commission_rate: string;
  strategy_params: Record<string, string>;
}

export interface BacktestResponse {
  strategy_name: string;
  initial_capital: string;
  final_value: string;
  total_pnl: string;
  return_percentage: string;
  total_trades: number;
  winning_trades: number;
  losing_trades: number;
  max_drawdown: string;
  sharpe_ratio: string;
  volatility: string;
  win_rate: string;
  profit_factor: string;
  total_commission: string;
  trades: TradeInfo[];
  equity_curve: string[];
  data_source: string;
}

export interface TradeInfo {
  timestamp: string;
  symbol: string;
  side: string;
  quantity: string;
  price: string;
  realized_pnl?: string;
  commission: string;
}

export interface HistoricalDataRequest {
  symbol: string;
  limit?: number;
}

export interface TickDataResponse {
  timestamp: string;
  symbol: string;
  price: string;
  quantity: string;
  side: string;
}

// ============ P11: 高级回测类型 ============

export interface MultiTimeframeBacktestRequest {
  strategy: string;
  symbol: string;
  capital: number;
  data_count: number;
  commission_rate: number;
  strategy_params?: Record<string, string>;
}

export interface WalkForwardRequest {
  strategy: string;
  symbol: string;
  capital: number;
  commission_rate: number;
  train_candles: number;
  test_candles: number;
  step_candles: number;
  data_count: number;
  strategy_params?: Record<string, string>;
}

export interface OutOfSampleRequest {
  strategy: string;
  symbol: string;
  capital: number;
  commission_rate: number;
  train_ratio: number;
  data_count: number;
  strategy_params?: Record<string, string>;
}

export interface MultiSymbolBacktestRequest {
  strategy: string;
  symbols: string[];
  capital: number;
  commission_rate: number;
  data_count: number;
  market_state_window: number;
  strategy_params?: Record<string, string>;
}

export interface MarketStateAnalysisRequest {
  symbol: string;
  data_count: number;
  window: number;
}

export interface WalkForwardRoundSummary {
  round: number;
  train_start: string;
  train_end: string;
  test_start: string;
  test_end: string;
  train_return_pct: string;
  train_sharpe: string;
  train_trades: number;
  test_return_pct: string;
  test_sharpe: string;
  test_trades: number;
  test_win_rate: string;
  test_max_drawdown: string;
  overfit_ratio: string;
}

export interface WalkForwardResult {
  total_rounds: number;
  profitable_rounds: number;
  overall_test_return_pct: string;
  overall_test_sharpe: string;
  overall_test_max_drawdown: string;
  overall_test_win_rate: string;
  avg_overfit_ratio: string;
  is_overfit: boolean;
  rounds: WalkForwardRoundSummary[];
}

export interface OutOfSampleResult {
  train_return_pct: string;
  train_sharpe: string;
  train_max_drawdown: string;
  train_win_rate: string;
  train_trades: number;
  train_profit_factor: string;
  test_return_pct: string;
  test_sharpe: string;
  test_max_drawdown: string;
  test_win_rate: string;
  test_trades: number;
  test_profit_factor: string;
  overfit_ratio: string;
  is_overfit: boolean;
}

export interface SymbolBacktestResultItem {
  symbol: string;
  return_pct: string;
  sharpe: string;
  win_rate: string;
  max_drawdown: string;
  total_trades: number;
  profit_factor: string;
  market_state: string;
  data_quality: string;
}

export interface MultiSymbolBacktestResult {
  total_symbols: number;
  profitable_symbols: number;
  losing_symbols: number;
  avg_return_pct: string;
  avg_sharpe: string;
  avg_win_rate: string;
  avg_max_drawdown: string;
  total_trades: number;
  best_symbol: string;
  best_return_pct: string;
  worst_symbol: string;
  worst_return_pct: string;
  cross_symbol_correlation: string;
  symbols: SymbolBacktestResultItem[];
}

export interface MarketStateResult {
  symbol: string;
  total_candles: number;
  analysis_window: number;
  state_distribution: Record<string, string>;
  avg_volatility: string;
  avg_trend_strength: string;
  trend_ratio: string;
  ranging_ratio: string;
  data_quality_score: string;
  summary: string;
}

// ============ 策略实时分析类型 ============

export interface StrategyAnalysisRequest {
  symbol: string;
  strategy_id?: string;
}

export interface TimeframeAnalysis {
  timeframe: string;      // "4h", "1h", "15m"
  direction: string;      // "bullish", "bearish", "neutral"
  confidence: string;     // "0.0" ~ "1.0"
  description: string;
}

export interface StrategyAnalysisResult {
  symbol: string;
  strategy_id: string;
  strategy_name: string;
  timeframes: TimeframeAnalysis[];
  overall_direction: string;   // "bullish", "bearish", "neutral"
  overall_confidence: string;
  entry_allowed: boolean;
  entry_direction: string | null;  // "long", "short"
  analysis_time: string;
}

// ============ 信号历史类型 ============

export interface SignalHistoryRequest {
  symbol?: string;
  strategy_id?: string;
  limit?: number;
}

export interface SignalRecord {
  id: string;
  timestamp: string;
  symbol: string;
  direction: string;      // "bullish", "bearish", "neutral"
  price: string;
  outcome: string | null; // "confirmed", "invalidated", "expired", "superseded", "pending", null
  pnl: string | null;     // "+1.6%" or "-0.8%"
}

export interface SignalStats {
  total_signals: number;
  win_count: number;
  loss_count: number;
  win_rate: string;
  avg_win_pnl: string;
  avg_loss_pnl: string;
  best_signal_pnl: string;
  worst_signal_pnl: string;
}

export interface SignalHistoryResult {
  signals: SignalRecord[];
  stats: SignalStats;
}