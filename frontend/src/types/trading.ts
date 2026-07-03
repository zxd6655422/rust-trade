// ============ 实时行情 ============

export interface RealtimePrice {
  symbol: string;
  price: string;
  change_24h?: string;
  volume_24h?: string;
  high_24h?: string;
  low_24h?: string;
  updated_at: string;
}

export interface KlineData {
  timestamp: string;
  open: string;
  high: string;
  low: string;
  close: string;
  volume: string;
}

export interface PriceHistoryRequest {
  symbol: string;
  timeframe: string;
  limit?: number;
}

// ============ 持仓 ============

export interface PositionInfo {
  id: string;
  symbol: string;
  side: string;        // "Long" | "Short"
  quantity: string;
  avg_entry_price: string;
  current_price?: string;
  unrealized_pnl?: string;
  realized_pnl: string;
  opened_at: string;
  updated_at: string;
}

// ============ 交易记录 ============

export interface TradeRecord {
  id: string;
  order_id?: string;
  symbol: string;
  side: string;        // "Buy" | "Sell"
  price: string;
  quantity: string;
  commission: string;
  realized_pnl?: string;
  strategy_id?: string;
  trade_time: string;
  created_at: string;
}

export interface TradeHistoryRequest {
  symbol?: string;
  limit?: number;
  offset?: number;
}

// ============ 盈亏汇总 ============

export interface PnlSummaryRequest {
  symbol?: string;
  days?: number;
}

export interface PnlSummary {
  period_days: number;
  symbol?: string;
  total_trades: number;
  winning_trades: number;
  losing_trades: number;
  win_rate: string;
  total_pnl?: string;
  total_commission?: string;
  best_trade?: string;
  worst_trade?: string;
  avg_pnl?: string;
}

// ============ 资金曲线 ============

export interface EquityCurveRequest {
  symbol?: string;
  period?: string;   // "daily" | "weekly" | "monthly"
  days?: number;
}

export interface EquityCurvePoint {
  date: string;
  equity: string;
  pnl: string;
  cumulative_pnl: string;
}

// ============ 性能指标 ============

export interface PerformanceRequest {
  symbol?: string;
  days?: number;
}

export interface PerformanceMetrics {
  sharpe_ratio: string;
  sortino_ratio: string;
  max_drawdown: string;
  max_drawdown_duration_days: number;
  calmar_ratio: string;
  volatility: string;
  win_rate: string;
  profit_factor: string;
  avg_trade_duration_hours: number;
  total_trades: number;
  winning_trades: number;
  losing_trades: number;
  avg_win: string;
  avg_loss: string;
  largest_win: string;
  largest_loss: string;
  consecutive_wins: number;
  consecutive_losses: number;
}

// ============ 手续费统计 ============

export interface CommissionStats {
  total_commission: string;
  avg_commission_per_trade: string;
  commission_by_symbol: SymbolCommission[];
  commission_by_month: MonthlyCommission[];
}

export interface SymbolCommission {
  symbol: string;
  total_commission: string;
  trade_count: number;
}

export interface MonthlyCommission {
  month: string;
  total_commission: string;
  trade_count: number;
}

// ============ 策略胜率 ============

export interface StrategyPerformance {
  strategy_id: string;
  strategy_name: string;
  total_trades: number;
  winning_trades: number;
  losing_trades: number;
  win_rate: number;
  total_pnl: number;
  avg_pnl: number;
  best_trade: number;
  worst_trade: number;
  sharpe_ratio: number;
  max_drawdown: number;
}
