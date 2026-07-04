'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer } from 'recharts';
import {
  Loader2, AlertCircle, CheckCircle,
  Play, BarChart3
} from 'lucide-react';
import {
  DataInfoResponse,
  StrategyInfo,
  BacktestRequest,
  BacktestResponse,
} from '@/types/backtest';
import { useLanguage } from '@/lib/i18n/context';

interface BacktestParams {
  strategy_id: string;
  symbol: string;
  data_count: number;
  initial_capital: string;
  commission_rate: string;
  short_period: string;
  long_period: string;
  [key: string]: string | number;
}

export default function BacktestContent() {
  const { t } = useLanguage();
  const [dataInfo, setDataInfo] = useState<DataInfoResponse | null>(null);
  const [strategies, setStrategies] = useState<StrategyInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const [params, setParams] = useState<BacktestParams>({
    strategy_id: '',
    symbol: '',
    data_count: 10000,
    initial_capital: '10000',
    commission_rate: '0.001',
    short_period: '5',
    long_period: '20',
  });

  const [configValid, setConfigValid] = useState<boolean | null>(null);
  const [isRunning, setIsRunning] = useState(false);
  const [result, setResult] = useState<BacktestResponse | null>(null);

  useEffect(() => {
    initializeData();
  }, []);

  useEffect(() => {
    if (params.symbol && params.data_count > 0) {
      validateConfiguration();
    }
  }, [params.symbol, params.data_count]);

  const initializeData = async () => {
    try {
      setLoading(true);
      setError(null);
      const [dataInfoResult, strategiesResult] = await Promise.all([
        invoke<DataInfoResponse>('get_data_info'),
        invoke<StrategyInfo[]>('get_available_strategies')
      ]);
      setDataInfo(dataInfoResult);
      setStrategies(strategiesResult);
      if (strategiesResult.length > 0) {
        setParams(prev => ({ ...prev, strategy_id: strategiesResult[0].id }));
      }
      if (dataInfoResult.symbol_info.length > 0) {
        setParams(prev => ({ ...prev, symbol: dataInfoResult.symbol_info[0].symbol }));
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load data');
    } finally {
      setLoading(false);
    }
  };

  const validateConfiguration = async () => {
    try {
      const isValid = await invoke<boolean>('validate_backtest_config', {
        symbol: params.symbol,
        dataCount: params.data_count
      });
      setConfigValid(isValid);
    } catch {
      setConfigValid(false);
    }
  };

  const runBacktest = async () => {
    if (!configValid) return;
    try {
      setIsRunning(true);
      setError(null);
      setResult(null);
      const request: BacktestRequest = {
        strategy_id: params.strategy_id,
        symbol: params.symbol,
        data_count: params.data_count,
        initial_capital: params.initial_capital,
        commission_rate: params.commission_rate,
        strategy_params: {
          short_period: params.short_period,
          long_period: params.long_period,
        }
      };
      const response = await invoke<BacktestResponse>('run_backtest', { request });
      setResult(response);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Backtest failed');
    } finally {
      setIsRunning(false);
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center py-20">
        <Loader2 className="w-6 h-6 animate-spin mr-2" />
        <span>{t.backtestContent.loading}</span>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Configuration */}
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="flex items-center gap-2 text-lg">
            <BarChart3 className="w-5 h-5" />
            {t.backtestContent.title}
            {configValid !== null && (
              configValid ? (
                <CheckCircle className="w-4 h-4 text-emerald-500" />
              ) : (
                <AlertCircle className="w-4 h-4 text-red-500" />
              )
            )}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            <div>
              <label className="block text-sm font-medium mb-1.5">{t.backtestContent.strategy}</label>
              <select
                value={params.strategy_id}
                onChange={(e) => setParams({ ...params, strategy_id: e.target.value })}
                className="w-full p-2.5 border rounded-md bg-background text-sm"
              >
                <option value="">{t.backtestContent.selectStrategy}</option>
                {strategies.map((s) => (
                  <option key={s.id} value={s.id}>{s.name}</option>
                ))}
              </select>
              {params.strategy_id && (
                <p className="text-xs text-muted-foreground mt-1">
                  {strategies.find(s => s.id === params.strategy_id)?.description}
                </p>
              )}
            </div>

            <div>
              <label className="block text-sm font-medium mb-1.5">{t.backtestContent.symbol}</label>
              <select
                value={params.symbol}
                onChange={(e) => setParams({ ...params, symbol: e.target.value })}
                className="w-full p-2.5 border rounded-md bg-background text-sm"
              >
                <option value="">{t.backtestContent.selectSymbol}</option>
                {dataInfo?.symbol_info.map((s) => (
                  <option key={s.symbol} value={s.symbol}>
                    {s.symbol} ({s.records_count.toLocaleString()})
                  </option>
                ))}
              </select>
            </div>

            <div>
              <label className="block text-sm font-medium mb-1.5">{t.backtestContent.dataPoints}</label>
              <input
                type="number"
                value={params.data_count}
                onChange={(e) => setParams({ ...params, data_count: parseInt(e.target.value) || 0 })}
                className="w-full p-2.5 border rounded-md bg-background text-sm"
                min="100"
                max="100000"
              />
            </div>

            <div>
              <label className="block text-sm font-medium mb-1.5">{t.backtestContent.initialCapital}</label>
              <input
                type="text"
                value={params.initial_capital}
                onChange={(e) => setParams({ ...params, initial_capital: e.target.value })}
                className="w-full p-2.5 border rounded-md bg-background text-sm"
              />
            </div>

            <div>
              <label className="block text-sm font-medium mb-1.5">{t.backtestContent.commission}</label>
              <input
                type="text"
                value={(parseFloat(params.commission_rate) * 100).toString()}
                onChange={(e) => {
                  const pct = parseFloat(e.target.value) || 0;
                  setParams({ ...params, commission_rate: (pct / 100).toString() });
                }}
                className="w-full p-2.5 border rounded-md bg-background text-sm"
              />
            </div>

            {params.strategy_id === 'sma' && (
              <>
                <div>
                  <label className="block text-sm font-medium mb-1.5">{t.backtestContent.shortPeriod}</label>
                  <input
                    type="number"
                    value={params.short_period}
                    onChange={(e) => setParams({ ...params, short_period: e.target.value })}
                    className="w-full p-2.5 border rounded-md bg-background text-sm"
                    min="1"
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium mb-1.5">{t.backtestContent.longPeriod}</label>
                  <input
                    type="number"
                    value={params.long_period}
                    onChange={(e) => setParams({ ...params, long_period: e.target.value })}
                    className="w-full p-2.5 border rounded-md bg-background text-sm"
                    min="1"
                  />
                </div>
              </>
            )}
          </div>

          {configValid !== null && (
            <div className={`mt-4 p-3 rounded-md text-sm ${
              configValid
                ? 'bg-emerald-50 text-emerald-700 dark:bg-emerald-950/30 dark:text-emerald-300'
                : 'bg-red-50 text-red-700 dark:bg-red-950/30 dark:text-red-300'
            }`}>
              {configValid ? (
                <span className="flex items-center gap-2">
                  <CheckCircle className="w-4 h-4" />
                  {t.backtestContent.configValid}
                </span>
              ) : (
                <span className="flex items-center gap-2">
                  <AlertCircle className="w-4 h-4" />
                  {t.backtestContent.configInvalid}
                </span>
              )}
            </div>
          )}

          <Button
            onClick={runBacktest}
            disabled={!configValid || isRunning || !params.strategy_id || !params.symbol}
            className="mt-4"
          >
            {isRunning ? (
              <span className="flex items-center gap-2">
                <Loader2 className="w-4 h-4 animate-spin" />
                {t.backtestContent.running}
              </span>
            ) : (
              <span className="flex items-center gap-2">
                <Play className="w-4 h-4" />
                {t.backtestContent.runBacktest}
              </span>
            )}
          </Button>
        </CardContent>
      </Card>

      {/* Error */}
      {error && (
        <Card className="border-destructive/50 bg-destructive/5">
          <CardContent className="pt-6">
            <div className="flex items-center gap-2 text-destructive">
              <AlertCircle className="w-5 h-5" />
              <span>{error}</span>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Results */}
      {result && (
        <div className="space-y-6">
          {/* Summary */}
          <Card>
            <CardHeader className="pb-3">
              <div className="flex items-center justify-between">
                <CardTitle className="text-lg">{result.strategy_name} {t.backtestContent.results}</CardTitle>
                <Badge variant={result.data_source.startsWith('OHLC') ? 'default' : 'secondary'}>
                  {result.data_source}
                </Badge>
              </div>
            </CardHeader>
            <CardContent>
              <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-5 gap-4">
                <div>
                  <p className="text-xs text-muted-foreground">{t.backtestContent.returnValue}</p>
                  <p className={`text-xl font-bold ${parseFloat(result.return_percentage) >= 0 ? 'text-emerald-500' : 'text-red-500'}`}>
                    {parseFloat(result.return_percentage).toFixed(2)}%
                  </p>
                </div>
                <div>
                  <p className="text-xs text-muted-foreground">{t.backtestContent.finalValue}</p>
                  <p className="text-xl font-bold">${parseFloat(result.final_value).toFixed(2)}</p>
                </div>
                <div>
                  <p className="text-xs text-muted-foreground">{t.backtestContent.totalPnl}</p>
                  <p className={`text-xl font-bold ${parseFloat(result.total_pnl) >= 0 ? 'text-emerald-500' : 'text-red-500'}`}>
                    ${parseFloat(result.total_pnl).toFixed(2)}
                  </p>
                </div>
                <div>
                  <p className="text-xs text-muted-foreground">{t.backtestContent.sharpe}</p>
                  <p className="text-xl font-bold">{parseFloat(result.sharpe_ratio).toFixed(2)}</p>
                </div>
                <div>
                  <p className="text-xs text-muted-foreground">{t.backtestContent.maxDd}</p>
                  <p className="text-xl font-bold text-red-500">{parseFloat(result.max_drawdown).toFixed(2)}%</p>
                </div>
                <div>
                  <p className="text-xs text-muted-foreground">{t.backtestContent.winRate}</p>
                  <p className="text-xl font-bold">{parseFloat(result.win_rate).toFixed(1)}%</p>
                </div>
                <div>
                  <p className="text-xs text-muted-foreground">{t.backtestContent.trades}</p>
                  <p className="text-xl font-bold">{result.total_trades}</p>
                </div>
                <div>
                  <p className="text-xs text-muted-foreground">{t.backtestContent.wins}</p>
                  <p className="text-xl font-bold text-emerald-500">{result.winning_trades}</p>
                </div>
                <div>
                  <p className="text-xs text-muted-foreground">{t.backtestContent.losses}</p>
                  <p className="text-xl font-bold text-red-500">{result.losing_trades}</p>
                </div>
                <div>
                  <p className="text-xs text-muted-foreground">{t.backtestContent.profitFactor}</p>
                  <p className="text-xl font-bold">{parseFloat(result.profit_factor).toFixed(2)}</p>
                </div>
              </div>
            </CardContent>
          </Card>

          {/* Equity Curve */}
          {result.equity_curve?.length > 0 && (
            <Card>
              <CardHeader className="pb-3">
                <CardTitle className="text-lg">{t.backtestContent.equityCurve}</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="h-72">
                  <ResponsiveContainer width="100%" height="100%">
                    <LineChart
                      data={result.equity_curve.map((v, i) => ({
                        index: i,
                        value: parseFloat(v),
                      }))}
                    >
                      <CartesianGrid strokeDasharray="3 3" className="opacity-30" />
                      <XAxis dataKey="index" tick={{ fontSize: 10 }} />
                      <YAxis
                        domain={['auto', 'auto']}
                        tick={{ fontSize: 10 }}
                        tickFormatter={(v) => `$${v.toFixed(0)}`}
                        width={70}
                      />
                      <Tooltip
                        formatter={(v: number) => [`$${v.toFixed(2)}`, 'Value']}
                        labelFormatter={(i) => `Trade #${i}`}
                      />
                      <Line
                        type="monotone"
                        dataKey="value"
                        stroke="#2563eb"
                        dot={false}
                        strokeWidth={2}
                      />
                    </LineChart>
                  </ResponsiveContainer>
                </div>
              </CardContent>
            </Card>
          )}

          {/* Trades Table */}
          {result.trades?.length > 0 && (
            <Card>
              <CardHeader className="pb-3">
                <CardTitle className="text-lg">
                  {t.backtestContent.tradesCount} ({result.trades.length})
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="overflow-x-auto">
                  <table className="w-full text-sm">
                    <thead>
                      <tr className="border-b text-left">
                        <th className="pb-2 font-medium text-muted-foreground">#</th>
                        <th className="pb-2 font-medium text-muted-foreground">Time</th>
                        <th className="pb-2 font-medium text-muted-foreground">Side</th>
                        <th className="pb-2 font-medium text-muted-foreground">Symbol</th>
                        <th className="pb-2 font-medium text-muted-foreground text-right">Qty</th>
                        <th className="pb-2 font-medium text-muted-foreground text-right">Price</th>
                        <th className="pb-2 font-medium text-muted-foreground text-right">PnL</th>
                        <th className="pb-2 font-medium text-muted-foreground text-right">Commission</th>
                      </tr>
                    </thead>
                    <tbody>
                      {result.trades.slice(0, 50).map((t, i) => (
                        <tr key={i} className="border-b last:border-0 hover:bg-muted/50">
                          <td className="py-2 text-muted-foreground">{i + 1}</td>
                          <td className="py-2 text-xs font-mono">{new Date(t.timestamp).toLocaleString()}</td>
                          <td className="py-2">
                            <Badge variant={t.side === 'Buy' ? 'default' : 'destructive'} className="text-xs">
                              {t.side}
                            </Badge>
                          </td>
                          <td className="py-2">{t.symbol}</td>
                          <td className="py-2 text-right font-mono">{parseFloat(t.quantity).toFixed(6)}</td>
                          <td className="py-2 text-right font-mono">${parseFloat(t.price).toFixed(2)}</td>
                          <td className="py-2 text-right">
                            {t.realized_pnl ? (
                              <span className={`font-mono ${parseFloat(t.realized_pnl) >= 0 ? 'text-emerald-500' : 'text-red-500'}`}>
                                ${parseFloat(t.realized_pnl).toFixed(2)}
                              </span>
                            ) : '-'}
                          </td>
                          <td className="py-2 text-right font-mono text-muted-foreground">
                            ${parseFloat(t.commission).toFixed(4)}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                  {result.trades.length > 50 && (
                    <p className="text-xs text-muted-foreground mt-3">
                      Showing first 50 of {result.trades.length} trades
                    </p>
                  )}
                </div>
              </CardContent>
            </Card>
          )}
        </div>
      )}
    </div>
  );
}
