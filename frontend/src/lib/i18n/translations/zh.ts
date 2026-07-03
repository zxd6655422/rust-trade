import { Translations } from './en';

export const zh: Translations = {
  // 通用
  common: {
    loading: '加载中...',
    error: '错误',
    refresh: '刷新',
    noData: '暂无数据',
    comingSoon: '即将推出',
    systemOnline: '系统运行中',
    connected: '已连接',
    disconnected: '未连接',
    selected: '已选择',
    page: '页',
    showing: '显示',
    trades: '笔交易',
  },

  // 侧边栏
  sidebar: {
    dashboard: '仪表盘',
    trading: '交易中心',
    settings: '设置',
    quantitativeTrading: '量化交易',
  },

  // 顶部栏
  header: {
    title: 'Rust 交易系统',
  },

  // 交易中心
  trading: {
    title: '交易中心',
    subtitle: '监控持仓、分析绩效、执行策略',
    liveData: '实时数据',
    liveTrading: '实盘交易',
    backtest: '回测',
    paperTrading: '模拟交易',
    spot: '现货',
    futures: '合约',
    spotMarket: '现货市场',
    futuresMarket: 'USDT 永续合约',
    paperTradingDesc: '使用真实市场数据模拟交易策略，无需承担真实资金风险。此功能将在后续更新中提供。',
    accountProfit: '账户盈亏',
  },

  // 价格行情
  priceTicker: {
    marketPrices: '市场行情',
    vol: '成交量',
  },

  // K线图
  klineChart: {
    price: '价格',
    volume: '成交量',
    open: '开盘',
    high: '最高',
    low: '最低',
    close: '收盘',
    time: '时间',
  },

  // 持仓列表
  positionTable: {
    title: '当前持仓',
    noPositions: '暂无持仓',
    noPositionsDesc: '执行交易后持仓将显示在此处',
    symbol: '交易对',
    side: '方向',
    quantity: '数量',
    entryPrice: '开仓价',
    current: '当前价',
    unrealizedPnl: '未实现盈亏',
    realizedPnl: '已实现盈亏',
    long: '做多',
    short: '做空',
  },

  // 交易历史
  tradeHistory: {
    title: '交易历史',
    noTrades: '暂无交易记录',
    noTradesDesc: '完成的交易将显示在此处',
    time: '时间',
    symbol: '交易对',
    side: '方向',
    price: '价格',
    quantity: '数量',
    commission: '手续费',
    pnl: '盈亏',
    buy: '买入',
    sell: '卖出',
  },

  // 盈亏汇总
  pnlSummary: {
    title: '盈亏汇总',
    totalPnl: '总盈亏',
    winRate: '胜率',
    bestTrade: '最佳交易',
    worstTrade: '最差交易',
    avgPnl: '平均盈亏',
    netPnl: '净盈亏',
    wins: '盈利',
    losses: '亏损',
  },

  // 资金曲线
  equityCurve: {
    title: '资金曲线',
    cumulative: '累计',
    daily: '日',
    weekly: '周',
    monthly: '月',
  },

  // 性能指标
  performancePanel: {
    title: '绩效指标',
    sharpeRatio: '夏普比率',
    sharpeDesc: '风险调整后收益',
    sortinoRatio: '索提诺比率',
    sortinoDesc: '下行风险调整后收益',
    maxDrawdown: '最大回撤',
    maxDrawdownDesc: '最大峰谷回撤',
    calmarRatio: '卡玛比率',
    calmarDesc: '收益 / 最大回撤',
    winRate: '胜率',
    profitFactor: '盈亏比',
    profitFactorDesc: '总盈利 / 总亏损',
    totalTrades: '总交易数',
    avgWin: '平均盈利',
    avgLoss: '平均亏损',
    volatility: '波动率',
    largestWin: '最大盈利',
    largestLoss: '最大亏损',
    consecWins: '连续盈利',
    consecLosses: '连续亏损',
  },

  // 手续费统计
  commissionStats: {
    title: '手续费统计',
    totalCommission: '总手续费',
    avgPerTrade: '平均每笔',
    bySymbol: '按交易对',
    monthlyTrend: '月度趋势',
  },

  // 策略胜率
  strategyWinRate: {
    title: '策略胜率 & 结算',
    totalTrades: '总交易数',
    winning: '盈利',
    losing: '亏损',
    winRate: '胜率',
    profitFactor: '盈亏比',
    byStrategy: '按策略',
    avgPnl: '平均盈亏',
    best: '最佳',
    worst: '最差',
    noData: '暂无策略数据',
    noDataDesc: '执行交易后策略绩效将显示在此处',
  },

  // 回测
  backtest: {
    title: '回测配置',
    strategy: '策略',
    selectStrategy: '选择策略',
    symbol: '交易对',
    selectSymbol: '选择交易对',
    dataPoints: '数据点数',
    initialCapital: '初始资金 ($)',
    commission: '手续费 (%)',
    shortPeriod: '短周期',
    longPeriod: '长周期',
    configValid: '配置有效 — 可以开始回测',
    configInvalid: '数据不足，无法使用此配置',
    runBacktest: '运行回测',
    running: '运行中...',
    results: '回测结果',
    returnValue: '收益率',
    finalValue: '最终价值',
    totalPnl: '盈亏',
    sharpe: '夏普',
    maxDd: '最大回撤',
    winRate: '胜率',
    trades: '交易数',
    wins: '盈利',
    losses: '亏损',
    profitFactor: '盈亏比',
    equityCurve: '资金曲线',
    tradesCount: '交易记录',
  },

  // 设置
  settings: {
    title: '设置',
    language: '语言',
    theme: '主题',
    darkMode: '深色模式',
    lightMode: '浅色模式',
  },
};
