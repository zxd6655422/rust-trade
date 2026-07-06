// PaperTradingContent.tsx
// Paper Trading 模拟交易界面
// 包含: 配置面板、状态概览、手动下单、持仓列表、交易记录

'use client';

import { useEffect, useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  Loader2, Play, Square, RotateCcw,
  DollarSign, BarChart3, Activity, AlertCircle,
  Plus, ArrowUpRight, ArrowDownRight, Clock, RefreshCw
} from 'lucide-react';
import { useLanguage } from '@/lib/i18n/context';

// ===== 类型定义 =====

interface PaperStatusResponse {
  running: boolean;
  initial_capital: string;
  cash: string;
  total_value: string;
  total_pnl: string;
  total_pnl_pct: string;
  realized_pnl: string;
  unrealized_pnl: string;
  total_commission: string;
  total_trades: number;
  win_rate: string;
  positions: PaperPositionResponse[];
  pending_orders: number;
  latest_prices: Record<string, string>;
  started_at: string | null;
}

interface PaperPositionResponse {
  symbol: string;
  side: string;
  quantity: string;
  avg_price: string;
  current_price: string;
  market_value: string;
  unrealized_pnl: string;
  unrealized_pnl_pct: string;
}

interface PaperTradeResponse {
  order_id: string;
  symbol: string;
  side: string;
  order_type: string;
  quantity: string;
  price: string | null;
  status: string;
  filled_price: string | null;
  commission: string;
  created_at: string;
  filled_at: string | null;
  reject_reason: string | null;
}

// ===== 主组件 =====

export default function PaperTradingContent() {
  const { t } = useLanguage();

  // 状态
  const [status, setStatus] = useState<PaperStatusResponse | null>(null);
  const [trades, setTrades] = useState<PaperTradeResponse[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 配置
  const [initialCapital, setInitialCapital] = useState('10000');
  const [symbols, setSymbols] = useState<string[]>([]);

  // 加载交易对列表
  useEffect(() => {
    invoke<{ symbol: string; enabled: boolean }[]>('get_symbols')
      .then(result => {
        const enabled = result.filter(s => s.enabled).map(s => s.symbol);
        setSymbols(enabled.length > 0 ? enabled : ['BTCUSDT', 'ETHUSDT', 'SOLUSDT']);
      })
      .catch(() => {
        setSymbols(['BTCUSDT', 'ETHUSDT', 'SOLUSDT']);
      });
  }, []);

  // 下单表单
  const [orderSymbol, setOrderSymbol] = useState('BTCUSDT');
  const [orderSide, setOrderSide] = useState<'buy' | 'sell'>('buy');
  const [orderQuantity, setOrderQuantity] = useState('0.001');
  const [orderType, setOrderType] = useState('market');
  const [orderPrice, setOrderPrice] = useState('');
  const [orderLoading, setOrderLoading] = useState(false);

  // 自动刷新
  const [autoRefresh, setAutoRefresh] = useState(true);

  // 获取状态
  const fetchStatus = useCallback(async () => {
    try {
      const result = await invoke<PaperStatusResponse>('get_paper_status');
      setStatus(result);
    } catch (err) {
      // Paper trader may not be initialized yet
      console.debug('Paper status not available:', err);
    }
  }, []);

  // 获取交易记录
  const fetchTrades = useCallback(async () => {
    try {
      const result = await invoke<PaperTradeResponse[]>('get_paper_trades');
      setTrades(result.reverse()); // 最新的在前
    } catch (err) {
      console.debug('Paper trades not available:', err);
    }
  }, []);

  // 自动刷新
  useEffect(() => {
    fetchStatus();
    fetchTrades();

    if (!autoRefresh) return;

    const interval = setInterval(() => {
      fetchStatus();
      fetchTrades();
    }, 5000); // 5秒刷新

    return () => clearInterval(interval);
  }, [autoRefresh, fetchStatus, fetchTrades]);

  // 启动模拟交易
  const handleStart = async () => {
    setLoading(true);
    setError(null);
    try {
      await invoke('start_paper_trading', {
        request: {
          initial_capital: initialCapital,
          symbols: symbols,
          commission_rate: '0.001',
          slippage_pct: '0.0001',
        }
      });
      await fetchStatus();
      await fetchTrades();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  // 停止模拟交易
  const handleStop = async () => {
    setLoading(true);
    try {
      await invoke('stop_paper_trading');
      await fetchStatus();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  // 重置模拟交易
  const handleReset = async () => {
    setLoading(true);
    setError(null);
    try {
      await invoke('reset_paper_trading');
      await fetchStatus();
      await fetchTrades();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  // 下单
  const handlePlaceOrder = async () => {
    if (!status?.running) return;

    setOrderLoading(true);
    setError(null);
    try {
      await invoke('place_paper_order', {
        request: {
          symbol: orderSymbol,
          side: orderSide,
          quantity: orderQuantity,
          order_type: orderType,
          price: orderType !== 'market' ? orderPrice : null,
        }
      });
      await fetchStatus();
      await fetchTrades();
      // 清空价格
      setOrderPrice('');
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setOrderLoading(false);
    }
  };

  // 格式化数字
  const fmt = (val: string, decimals = 2) => {
    const num = parseFloat(val);
    return isNaN(num) ? '0' : num.toFixed(decimals);
  };

  const fmtPct = (val: string) => {
    const num = parseFloat(val);
    return isNaN(num) ? '0.00' : num.toFixed(2);
  };

  const pnlColor = (val: string) => {
    const num = parseFloat(val);
    if (num > 0) return 'text-emerald-500';
    if (num < 0) return 'text-red-500';
    return 'text-muted-foreground';
  };

  // ===== 渲染 =====

  return (
    <div className="space-y-6">
      {/* 错误提示 */}
      {error && (
        <Card className="border-red-200 bg-red-50 dark:border-red-800 dark:bg-red-900/20">
          <CardContent className="py-3">
            <div className="flex items-center gap-2 text-red-800 dark:text-red-200">
              <AlertCircle className="w-4 h-4" />
              <span className="text-sm">{error}</span>
            </div>
          </CardContent>
        </Card>
      )}

      {/* 配置面板 + 状态概览 */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* 配置面板 */}
        <Card>
          <CardHeader className="pb-3">
            <CardTitle className="text-base flex items-center gap-2">
              <Activity className="w-4 h-4" />
              {t.paperTrading?.config || 'Configuration'}
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div>
              <label className="text-sm font-medium">{t.paperTrading?.initialCapital || 'Initial Capital'} (USDT)</label>
              <input
                type="number"
                value={initialCapital}
                onChange={(e) => setInitialCapital(e.target.value)}
                disabled={status?.running}
                className="w-full mt-1 px-3 py-2 border rounded-md text-sm dark:bg-gray-800 dark:border-gray-600 disabled:opacity-50"
                placeholder="10000"
              />
            </div>
            <div>
              <label className="text-sm font-medium">{t.paperTrading?.symbols || 'Symbols'}</label>
              <div className="flex flex-wrap gap-2 mt-1">
                {symbols.map(s => (
                  <button
                    key={s}
                    onClick={() => {
                      if (status?.running) return;
                      setSymbols(prev =>
                        prev.includes(s)
                          ? prev.filter(x => x !== s)
                          : [...prev, s]
                      );
                    }}
                    disabled={status?.running}
                    className={`px-3 py-1.5 rounded-md text-xs font-medium transition-all ${
                      symbols.includes(s)
                        ? 'bg-primary text-primary-foreground'
                        : 'bg-muted text-muted-foreground hover:bg-muted/80'
                    } disabled:opacity-50`}
                  >
                    {s}
                  </button>
                ))}
              </div>
            </div>
            <div className="flex gap-2 pt-2">
              {!status?.running ? (
                <Button onClick={handleStart} disabled={loading} className="flex-1">
                  {loading ? (
                    <Loader2 className="w-4 h-4 animate-spin mr-2" />
                  ) : (
                    <Play className="w-4 h-4 mr-2" />
                  )}
                  {t.paperTrading?.start || 'Start'}
                </Button>
              ) : (
                <Button onClick={handleStop} disabled={loading} variant="destructive" className="flex-1">
                  {loading ? (
                    <Loader2 className="w-4 h-4 animate-spin mr-2" />
                  ) : (
                    <Square className="w-4 h-4 mr-2" />
                  )}
                  {t.paperTrading?.stop || 'Stop'}
                </Button>
              )}
              <Button onClick={handleReset} disabled={loading || status?.running} variant="outline">
                <RotateCcw className="w-4 h-4" />
              </Button>
            </div>
          </CardContent>
        </Card>

        {/* 状态概览 */}
        <Card className="lg:col-span-2">
          <CardHeader className="pb-3">
            <div className="flex items-center justify-between">
              <CardTitle className="text-base flex items-center gap-2">
                <DollarSign className="w-4 h-4" />
                {t.paperTrading?.overview || 'Overview'}
              </CardTitle>
              <div className="flex items-center gap-2">
                {status?.running ? (
                  <Badge variant="default" className="bg-emerald-500">
                    <span className="w-2 h-2 rounded-full bg-white mr-1.5 animate-pulse" />
                    {t.paperTrading?.running || 'Running'}
                  </Badge>
                ) : (
                  <Badge variant="secondary">{t.paperTrading?.stopped || 'Stopped'}</Badge>
                )}
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setAutoRefresh(!autoRefresh)}
                  className={autoRefresh ? 'text-emerald-500' : 'text-muted-foreground'}
                >
                  <RefreshCw className={`w-3.5 h-3.5 ${autoRefresh ? 'animate-spin' : ''}`} />
                </Button>
              </div>
            </div>
          </CardHeader>
          <CardContent>
            {status ? (
              <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                <div>
                  <p className="text-xs text-muted-foreground">{t.paperTrading?.totalValue || 'Total Value'}</p>
                  <p className="text-xl font-bold">${fmt(status.total_value)}</p>
                </div>
                <div>
                  <p className="text-xs text-muted-foreground">{t.paperTrading?.totalPnl || 'Total PnL'}</p>
                  <p className={`text-xl font-bold ${pnlColor(status.total_pnl)}`}>
                    {parseFloat(status.total_pnl) >= 0 ? '+' : ''}${fmt(status.total_pnl)}
                    <span className="text-sm ml-1">({fmtPct(status.total_pnl_pct)}%)</span>
                  </p>
                </div>
                <div>
                  <p className="text-xs text-muted-foreground">{t.paperTrading?.cash || 'Cash'}</p>
                  <p className="text-xl font-bold">${fmt(status.cash)}</p>
                </div>
                <div>
                  <p className="text-xs text-muted-foreground">{t.paperTrading?.winRate || 'Win Rate'}</p>
                  <p className="text-xl font-bold">{fmtPct(status.win_rate)}%</p>
                </div>
                <div>
                  <p className="text-xs text-muted-foreground">{t.paperTrading?.realizedPnl || 'Realized PnL'}</p>
                  <p className={`text-sm font-medium ${pnlColor(status.realized_pnl)}`}>
                    ${fmt(status.realized_pnl)}
                  </p>
                </div>
                <div>
                  <p className="text-xs text-muted-foreground">{t.paperTrading?.unrealizedPnl || 'Unrealized PnL'}</p>
                  <p className={`text-sm font-medium ${pnlColor(status.unrealized_pnl)}`}>
                    ${fmt(status.unrealized_pnl)}
                  </p>
                </div>
                <div>
                  <p className="text-xs text-muted-foreground">{t.paperTrading?.trades || 'Trades'}</p>
                  <p className="text-sm font-medium">{status.total_trades}</p>
                </div>
                <div>
                  <p className="text-xs text-muted-foreground">{t.paperTrading?.commission || 'Commission'}</p>
                  <p className="text-sm font-medium">${fmt(status.total_commission)}</p>
                </div>
              </div>
            ) : (
              <div className="text-center py-8 text-muted-foreground">
                <p className="text-sm">{t.paperTrading?.notStarted || 'Configure and start paper trading'}</p>
              </div>
            )}
          </CardContent>
        </Card>
      </div>

      {/* 手动下单 + 持仓列表 */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* 手动下单面板 */}
        <Card>
          <CardHeader className="pb-3">
            <CardTitle className="text-base flex items-center gap-2">
              <Plus className="w-4 h-4" />
              {t.paperTrading?.placeOrder || 'Place Order'}
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            {/* 交易对 */}
            <div>
              <label className="text-sm font-medium">{t.paperTrading?.symbol || 'Symbol'}</label>
              <select
                value={orderSymbol}
                onChange={(e) => setOrderSymbol(e.target.value)}
                className="w-full mt-1 px-3 py-2 border rounded-md text-sm dark:bg-gray-800 dark:border-gray-600"
              >
                {symbols.map(s => (
                  <option key={s} value={s}>{s}</option>
                ))}
              </select>
            </div>

            {/* 买入/卖出 */}
            <div className="flex bg-muted rounded-lg p-1">
              <button
                onClick={() => setOrderSide('buy')}
                className={`flex-1 py-2 rounded-md text-sm font-medium transition-all ${
                  orderSide === 'buy'
                    ? 'bg-emerald-500 text-white'
                    : 'text-muted-foreground hover:text-foreground'
                }`}
              >
                {t.paperTrading?.buy || 'Buy'}
              </button>
              <button
                onClick={() => setOrderSide('sell')}
                className={`flex-1 py-2 rounded-md text-sm font-medium transition-all ${
                  orderSide === 'sell'
                    ? 'bg-red-500 text-white'
                    : 'text-muted-foreground hover:text-foreground'
                }`}
              >
                {t.paperTrading?.sell || 'Sell'}
              </button>
            </div>

            {/* 订单类型 */}
            <div>
              <label className="text-sm font-medium">{t.paperTrading?.orderType || 'Order Type'}</label>
              <select
                value={orderType}
                onChange={(e) => setOrderType(e.target.value)}
                className="w-full mt-1 px-3 py-2 border rounded-md text-sm dark:bg-gray-800 dark:border-gray-600"
              >
                <option value="market">{t.paperTrading?.market || 'Market'}</option>
                <option value="limit">{t.paperTrading?.limit || 'Limit'}</option>
                <option value="stop_loss">{t.paperTrading?.stopLoss || 'Stop Loss'}</option>
                <option value="take_profit">{t.paperTrading?.takeProfit || 'Take Profit'}</option>
              </select>
            </div>

            {/* 数量 */}
            <div>
              <label className="text-sm font-medium">{t.paperTrading?.quantity || 'Quantity'}</label>
              <input
                type="text"
                value={orderQuantity}
                onChange={(e) => setOrderQuantity(e.target.value)}
                className="w-full mt-1 px-3 py-2 border rounded-md text-sm dark:bg-gray-800 dark:border-gray-600"
                placeholder="0.001"
              />
            </div>

            {/* 价格 (限价/止损/止盈) */}
            {orderType !== 'market' && (
              <div>
                <label className="text-sm font-medium">{t.paperTrading?.price || 'Price'}</label>
                <input
                  type="text"
                  value={orderPrice}
                  onChange={(e) => setOrderPrice(e.target.value)}
                  className="w-full mt-1 px-3 py-2 border rounded-md text-sm dark:bg-gray-800 dark:border-gray-600"
                  placeholder={orderSymbol === 'BTCUSDT' ? '65000' : '3500'}
                />
              </div>
            )}

            {/* 当前价格参考 */}
            {status?.latest_prices?.[orderSymbol] && (
              <div className="text-xs text-muted-foreground">
                {t.paperTrading?.currentPrice || 'Current'}: ${fmt(status.latest_prices[orderSymbol], 2)}
              </div>
            )}

            {/* 下单按钮 */}
            <Button
              onClick={handlePlaceOrder}
              disabled={orderLoading || !status?.running}
              className={`w-full ${orderSide === 'buy' ? 'bg-emerald-500 hover:bg-emerald-600' : 'bg-red-500 hover:bg-red-600'}`}
            >
              {orderLoading ? (
                <Loader2 className="w-4 h-4 animate-spin mr-2" />
              ) : orderSide === 'buy' ? (
                <ArrowUpRight className="w-4 h-4 mr-2" />
              ) : (
                <ArrowDownRight className="w-4 h-4 mr-2" />
              )}
              {orderSide === 'buy'
                ? `${t.paperTrading?.buy || 'Buy'} ${orderSymbol}`
                : `${t.paperTrading?.sell || 'Sell'} ${orderSymbol}`
              }
            </Button>
          </CardContent>
        </Card>

        {/* 持仓列表 */}
        <Card className="lg:col-span-2">
          <CardHeader className="pb-3">
            <CardTitle className="text-base flex items-center gap-2">
              <BarChart3 className="w-4 h-4" />
              {t.paperTrading?.positions || 'Positions'}
              {status?.positions && status.positions.length > 0 && (
                <Badge variant="secondary" className="ml-auto">{status.positions.length}</Badge>
              )}
            </CardTitle>
          </CardHeader>
          <CardContent>
            {status?.positions && status.positions.length > 0 ? (
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b text-muted-foreground">
                      <th className="text-left py-2 font-medium">{t.paperTrading?.symbol || 'Symbol'}</th>
                      <th className="text-left py-2 font-medium">{t.paperTrading?.side || 'Side'}</th>
                      <th className="text-right py-2 font-medium">{t.paperTrading?.quantity || 'Qty'}</th>
                      <th className="text-right py-2 font-medium">{t.paperTrading?.avgPrice || 'Avg Price'}</th>
                      <th className="text-right py-2 font-medium">{t.paperTrading?.currentPrice || 'Current'}</th>
                      <th className="text-right py-2 font-medium">{t.paperTrading?.pnl || 'PnL'}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {status.positions.map((pos) => (
                      <tr key={pos.symbol} className="border-b">
                        <td className="py-2 font-medium">{pos.symbol}</td>
                        <td className="py-2">
                          <Badge variant={pos.side === 'Long' ? 'default' : 'destructive'} className="text-xs">
                            {pos.side}
                          </Badge>
                        </td>
                        <td className="py-2 text-right">{fmt(pos.quantity, 6)}</td>
                        <td className="py-2 text-right">${fmt(pos.avg_price, 2)}</td>
                        <td className="py-2 text-right">${fmt(pos.current_price, 2)}</td>
                        <td className={`py-2 text-right font-medium ${pnlColor(pos.unrealized_pnl)}`}>
                          ${fmt(pos.unrealized_pnl)}
                          <span className="text-xs ml-1">({fmtPct(pos.unrealized_pnl_pct)}%)</span>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            ) : (
              <div className="text-center py-8 text-muted-foreground">
                <BarChart3 className="w-8 h-8 mx-auto mb-2 opacity-30" />
                <p className="text-sm">{t.paperTrading?.noPositions || 'No open positions'}</p>
              </div>
            )}
          </CardContent>
        </Card>
      </div>

      {/* 交易记录 */}
      <Card>
        <CardHeader className="pb-3">
          <div className="flex items-center justify-between">
            <CardTitle className="text-base flex items-center gap-2">
              <Clock className="w-4 h-4" />
              {t.paperTrading?.tradeHistory || 'Trade History'}
              {trades.length > 0 && (
                <Badge variant="secondary" className="ml-2">{trades.length}</Badge>
              )}
            </CardTitle>
            <Button variant="ghost" size="sm" onClick={() => { fetchTrades(); fetchStatus(); }}>
              <RefreshCw className="w-3.5 h-3.5" />
            </Button>
          </div>
        </CardHeader>
        <CardContent>
          {trades.length > 0 ? (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b text-muted-foreground">
                    <th className="text-left py-2 font-medium">ID</th>
                    <th className="text-left py-2 font-medium">{t.paperTrading?.time || 'Time'}</th>
                    <th className="text-left py-2 font-medium">{t.paperTrading?.symbol || 'Symbol'}</th>
                    <th className="text-left py-2 font-medium">{t.paperTrading?.side || 'Side'}</th>
                    <th className="text-left py-2 font-medium">{t.paperTrading?.type || 'Type'}</th>
                    <th className="text-right py-2 font-medium">{t.paperTrading?.quantity || 'Qty'}</th>
                    <th className="text-right py-2 font-medium">{t.paperTrading?.price || 'Price'}</th>
                    <th className="text-right py-2 font-medium">{t.paperTrading?.commission || 'Fee'}</th>
                    <th className="text-left py-2 font-medium">{t.paperTrading?.status || 'Status'}</th>
                  </tr>
                </thead>
                <tbody>
                  {trades.slice(0, 50).map((trade) => (
                    <tr key={trade.order_id} className="border-b">
                      <td className="py-2 text-xs font-mono text-muted-foreground">{trade.order_id}</td>
                      <td className="py-2 text-xs">{new Date(trade.created_at).toLocaleString()}</td>
                      <td className="py-2 font-medium">{trade.symbol}</td>
                      <td className="py-2">
                        <Badge variant={trade.side === 'Buy' ? 'default' : 'destructive'} className="text-xs">
                          {trade.side}
                        </Badge>
                      </td>
                      <td className="py-2 text-xs">{trade.order_type}</td>
                      <td className="py-2 text-right">{fmt(trade.quantity, 6)}</td>
                      <td className="py-2 text-right">
                        {trade.filled_price ? `$${fmt(trade.filled_price, 2)}` : trade.price ? `$${fmt(trade.price, 2)}` : '-'}
                      </td>
                      <td className="py-2 text-right text-muted-foreground">${fmt(trade.commission, 4)}</td>
                      <td className="py-2">
                        <Badge
                          variant={
                            trade.status === 'Filled' ? 'default' :
                            trade.status === 'Rejected' ? 'destructive' :
                            'secondary'
                          }
                          className="text-xs"
                        >
                          {trade.status}
                        </Badge>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {trades.length > 50 && (
                <p className="text-xs text-muted-foreground mt-2 text-center">
                  Showing first 50 of {trades.length} trades
                </p>
              )}
            </div>
          ) : (
            <div className="text-center py-8 text-muted-foreground">
              <Clock className="w-8 h-8 mx-auto mb-2 opacity-30" />
              <p className="text-sm">{t.paperTrading?.noTrades || 'No trades yet'}</p>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
