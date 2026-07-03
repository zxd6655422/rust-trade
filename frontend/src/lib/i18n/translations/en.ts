export const en = {
  // Common
  common: {
    loading: 'Loading...',
    error: 'Error',
    refresh: 'Refresh',
    noData: 'No data available',
    comingSoon: 'Coming Soon',
    systemOnline: 'System Online',
    connected: 'Connected',
    disconnected: 'Disconnected',
    selected: 'Selected',
    page: 'Page',
    showing: 'Showing',
    trades: 'trades',
  },

  // Sidebar
  sidebar: {
    dashboard: 'Dashboard',
    trading: 'Trading',
    settings: 'Settings',
    quantitativeTrading: 'Quantitative Trading',
  },

  // Header
  header: {
    title: 'Rust Trading System',
  },

  // Trading Page
  trading: {
    title: 'Trading Center',
    subtitle: 'Monitor positions, analyze performance, and execute strategies',
    liveData: 'Live Data',
    liveTrading: 'Live Trading',
    backtest: 'Backtest',
    paperTrading: 'Paper Trading',
    spot: 'Spot',
    futures: 'Futures',
    spotMarket: 'Spot Market',
    futuresMarket: 'USDT-M Futures',
    paperTradingDesc: 'Simulate trading strategies with real market data but without risking real capital. This feature will be available in a future update.',
    accountProfit: 'Account Profit',
  },

  // Price Ticker
  priceTicker: {
    marketPrices: 'Market Prices',
    vol: 'Vol',
  },

  // Kline Chart
  klineChart: {
    price: 'Price',
    volume: 'Volume',
    open: 'Open',
    high: 'High',
    low: 'Low',
    close: 'Close',
    time: 'Time',
  },

  // Position Table
  positionTable: {
    title: 'Open Positions',
    noPositions: 'No open positions',
    noPositionsDesc: 'Positions will appear here when trades are executed',
    symbol: 'Symbol',
    side: 'Side',
    quantity: 'Quantity',
    entryPrice: 'Entry Price',
    current: 'Current',
    unrealizedPnl: 'Unrealized PnL',
    realizedPnl: 'Realized PnL',
    long: 'Long',
    short: 'Short',
  },

  // Trade History
  tradeHistory: {
    title: 'Trade History',
    noTrades: 'No trade history',
    noTradesDesc: 'Completed trades will appear here',
    time: 'Time',
    symbol: 'Symbol',
    side: 'Side',
    price: 'Price',
    quantity: 'Quantity',
    commission: 'Commission',
    pnl: 'PnL',
    buy: 'Buy',
    sell: 'Sell',
  },

  // PnL Summary
  pnlSummary: {
    title: 'PnL Summary',
    totalPnl: 'Total PnL',
    winRate: 'Win Rate',
    bestTrade: 'Best Trade',
    worstTrade: 'Worst Trade',
    avgPnl: 'Avg PnL',
    netPnl: 'Net PnL',
    wins: 'W',
    losses: 'L',
  },

  // Equity Curve
  equityCurve: {
    title: 'Equity Curve',
    cumulative: 'Cumulative',
    daily: 'Daily',
    weekly: 'Weekly',
    monthly: 'Monthly',
  },

  // Performance Panel
  performancePanel: {
    title: 'Performance Metrics',
    sharpeRatio: 'Sharpe Ratio',
    sharpeDesc: 'Risk-adjusted return',
    sortinoRatio: 'Sortino Ratio',
    sortinoDesc: 'Downside risk-adjusted',
    maxDrawdown: 'Max Drawdown',
    maxDrawdownDesc: 'Maximum peak-to-trough',
    calmarRatio: 'Calmar Ratio',
    calmarDesc: 'Return / Max Drawdown',
    winRate: 'Win Rate',
    profitFactor: 'Profit Factor',
    profitFactorDesc: 'Gross profit / Gross loss',
    totalTrades: 'Total Trades',
    avgWin: 'Avg Win',
    avgLoss: 'Avg Loss',
    volatility: 'Volatility',
    largestWin: 'Largest Win',
    largestLoss: 'Largest Loss',
    consecWins: 'Consec. Wins',
    consecLosses: 'Consec. Losses',
  },

  // Commission Stats
  commissionStats: {
    title: 'Commission Stats',
    totalCommission: 'Total Commission',
    avgPerTrade: 'Avg per Trade',
    bySymbol: 'By Symbol',
    monthlyTrend: 'Monthly Trend',
  },

  // Strategy Win Rate
  strategyWinRate: {
    title: 'Strategy Win Rate & Settlement',
    totalTrades: 'Total Trades',
    winning: 'Winning',
    losing: 'Losing',
    winRate: 'Win Rate',
    profitFactor: 'Profit Factor',
    byStrategy: 'By Strategy',
    avgPnl: 'Avg PnL',
    best: 'Best',
    worst: 'Worst',
    noData: 'No strategy data available',
    noDataDesc: 'Strategy performance will appear after trades are executed',
  },

  // Backtest
  backtest: {
    title: 'Backtest Configuration',
    strategy: 'Strategy',
    selectStrategy: 'Select Strategy',
    symbol: 'Symbol',
    selectSymbol: 'Select Symbol',
    dataPoints: 'Data Points',
    initialCapital: 'Initial Capital ($)',
    commission: 'Commission (%)',
    shortPeriod: 'Short Period',
    longPeriod: 'Long Period',
    configValid: 'Configuration valid — ready to backtest',
    configInvalid: 'Insufficient data for this configuration',
    runBacktest: 'Run Backtest',
    running: 'Running...',
    results: 'Results',
    returnValue: 'Return',
    finalValue: 'Final Value',
    totalPnl: 'P&L',
    sharpe: 'Sharpe',
    maxDd: 'Max DD',
    winRate: 'Win Rate',
    trades: 'Trades',
    wins: 'Wins',
    losses: 'Losses',
    profitFactor: 'Profit Factor',
    equityCurve: 'Equity Curve',
    tradesCount: 'Trades',
  },

  // Settings
  settings: {
    title: 'Settings',
    language: 'Language',
    theme: 'Theme',
    darkMode: 'Dark Mode',
    lightMode: 'Light Mode',
  },
};

export type Translations = typeof en;
