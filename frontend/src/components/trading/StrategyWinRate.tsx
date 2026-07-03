'use client';

import { useEffect, useState, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Loader2, Target, RefreshCw, TrendingUp, TrendingDown,
  CheckCircle2, XCircle
} from 'lucide-react';
import { TradeRecord, PerformanceMetrics } from '@/types/trading';
import { useLanguage } from '@/lib/i18n/context';

interface StrategyWinRateProps {
  symbol?: string;
}

interface StrategyStats {
  strategyId: string;
  totalTrades: number;
  winningTrades: number;
  losingTrades: number;
  winRate: number;
  totalPnl: number;
  avgPnl: number;
  bestTrade: number;
  worstTrade: number;
  totalCommission: number;
  netPnl: number;
}

export default function StrategyWinRate({ symbol }: StrategyWinRateProps) {
  const [trades, setTrades] = useState<TradeRecord[]>([]);
  const [metrics, setMetrics] = useState<PerformanceMetrics | null>(null);
  const [loading, setLoading] = useState(true);
  const { t } = useLanguage();

  const fetchData = async () => {
    try {
      setLoading(true);
      const [tradesResult, metricsResult] = await Promise.all([
        invoke<TradeRecord[]>('get_trade_history', {
          request: {
            symbol: symbol || null,
            limit: 1000,
            offset: 0
          }
        }),
        invoke<PerformanceMetrics>('get_performance_metrics', {
          request: {
            symbol: symbol || null,
            days: 365
          }
        })
      ]);
      setTrades(tradesResult);
      setMetrics(metricsResult);
    } catch (err) {
      console.error('Failed to fetch strategy data:', err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchData();
  }, [symbol]);

  // 按策略分组统计
  const strategyStats = useMemo(() => {
    const grouped: Record<string, TradeRecord[]> = {};
    trades.forEach(trade => {
      const key = trade.strategy_id || 'manual';
      if (!grouped[key]) grouped[key] = [];
      grouped[key].push(trade);
    });

    return Object.entries(grouped).map(([strategyId, strategyTrades]) => {
      const withPnl = strategyTrades.filter(t => t.realized_pnl);
      const winning = withPnl.filter(t => parseFloat(t.realized_pnl!) > 0);
      const losing = withPnl.filter(t => parseFloat(t.realized_pnl!) < 0);

      const pnls = withPnl.map(t => parseFloat(t.realized_pnl!));
      const totalPnl = pnls.reduce((sum, p) => sum + p, 0);
      const commissions = strategyTrades.reduce((sum, t) => sum + parseFloat(t.commission), 0);

      return {
        strategyId,
        totalTrades: strategyTrades.length,
        winningTrades: winning.length,
        losingTrades: losing.length,
        winRate: withPnl.length > 0 ? (winning.length / withPnl.length) * 100 : 0,
        totalPnl,
        avgPnl: pnls.length > 0 ? totalPnl / pnls.length : 0,
        bestTrade: pnls.length > 0 ? Math.max(...pnls) : 0,
        worstTrade: pnls.length > 0 ? Math.min(...pnls) : 0,
        totalCommission: commissions,
        netPnl: totalPnl - commissions,
      } as StrategyStats;
    }).sort((a, b) => b.totalTrades - a.totalTrades);
  }, [trades]);

  if (loading) {
    return (
      <div className="flex items-center justify-center py-8">
        <Loader2 className="w-5 h-5 animate-spin mr-2" />
        <span className="text-sm text-muted-foreground">{t.common.loading}</span>
      </div>
    );
  }

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Target className="w-5 h-5 text-muted-foreground" />
            <CardTitle className="text-lg">{t.strategyWinRate.title}</CardTitle>
          </div>
          <Button variant="ghost" size="sm" onClick={fetchData} disabled={loading}>
            <RefreshCw className={`w-3 h-3 ${loading ? 'animate-spin' : ''}`} />
          </Button>
        </div>
      </CardHeader>
      <CardContent>
        {/* Overall Metrics */}
        {metrics && (
          <div className="grid grid-cols-2 md:grid-cols-5 gap-3 mb-6">
            <div className="p-3 rounded-lg border bg-card text-center">
              <p className="text-[10px] text-muted-foreground">{t.strategyWinRate.totalTrades}</p>
              <p className="text-2xl font-bold">{metrics.total_trades}</p>
            </div>
            <div className="p-3 rounded-lg border bg-emerald-50/50 dark:bg-emerald-950/20 dark:border-emerald-800 text-center">
              <p className="text-[10px] text-muted-foreground">{t.strategyWinRate.winning}</p>
              <p className="text-2xl font-bold text-emerald-600">{metrics.winning_trades}</p>
            </div>
            <div className="p-3 rounded-lg border bg-red-50/50 dark:bg-red-950/20 dark:border-red-800 text-center">
              <p className="text-[10px] text-muted-foreground">{t.strategyWinRate.losing}</p>
              <p className="text-2xl font-bold text-red-600">{metrics.losing_trades}</p>
            </div>
            <div className="p-3 rounded-lg border text-center">
              <p className="text-[10px] text-muted-foreground">{t.strategyWinRate.winRate}</p>
              <p className={`text-2xl font-bold ${parseFloat(metrics.win_rate) >= 50 ? 'text-emerald-600' : 'text-red-600'}`}>
                {parseFloat(metrics.win_rate).toFixed(1)}%
              </p>
            </div>
            <div className="p-3 rounded-lg border text-center">
              <p className="text-[10px] text-muted-foreground">{t.strategyWinRate.profitFactor}</p>
              <p className={`text-2xl font-bold ${parseFloat(metrics.profit_factor) >= 1 ? 'text-emerald-600' : 'text-red-600'}`}>
                {parseFloat(metrics.profit_factor).toFixed(2)}
              </p>
            </div>
          </div>
        )}

        {/* Win Rate Visual Bar */}
        {metrics && metrics.total_trades > 0 && (
          <div className="mb-6">
            <div className="flex items-center justify-between text-xs mb-1">
              <span className="text-emerald-500 font-medium flex items-center gap-1">
                <CheckCircle2 className="w-3 h-3" />
                {t.strategyWinRate.winning} {metrics.winning_trades}
              </span>
              <span className="text-red-500 font-medium flex items-center gap-1">
                {t.strategyWinRate.losing} {metrics.losing_trades}
                <XCircle className="w-3 h-3" />
              </span>
            </div>
            <div className="h-3 rounded-full overflow-hidden bg-muted flex">
              <div
                className="bg-emerald-500 transition-all duration-500"
                style={{ width: `${(metrics.winning_trades / metrics.total_trades) * 100}%` }}
              />
              <div
                className="bg-red-500 transition-all duration-500"
                style={{ width: `${(metrics.losing_trades / metrics.total_trades) * 100}%` }}
              />
            </div>
          </div>
        )}

        {/* Strategy Breakdown */}
        {strategyStats.length > 0 && (
          <div>
            <h4 className="text-sm font-medium text-muted-foreground mb-3">{t.strategyWinRate.byStrategy}</h4>
            <div className="space-y-3">
              {strategyStats.map((stat) => (
                <div
                  key={stat.strategyId}
                  className="p-4 rounded-lg border hover:bg-muted/50 transition-colors"
                >
                  <div className="flex items-center justify-between mb-3">
                    <div className="flex items-center gap-2">
                      <Badge variant="outline" className="font-mono">
                        {stat.strategyId}
                      </Badge>
                      <span className="text-sm text-muted-foreground">
                        {stat.totalTrades} {t.common.trades}
                      </span>
                    </div>
                    <div className={`flex items-center gap-1 font-bold ${
                      stat.netPnl >= 0 ? 'text-emerald-500' : 'text-red-500'
                    }`}>
                      {stat.netPnl >= 0 ? <TrendingUp className="w-4 h-4" /> : <TrendingDown className="w-4 h-4" />}
                      {stat.netPnl >= 0 ? '+' : ''}${stat.netPnl.toFixed(2)}
                    </div>
                  </div>

                  <div className="grid grid-cols-2 md:grid-cols-5 gap-3 text-center">
                    <div>
                      <p className="text-[10px] text-muted-foreground">{t.strategyWinRate.winRate}</p>
                      <p className={`text-sm font-bold ${stat.winRate >= 50 ? 'text-emerald-500' : 'text-red-500'}`}>
                        {stat.winRate.toFixed(1)}%
                      </p>
                    </div>
                    <div>
                      <p className="text-[10px] text-muted-foreground">W / L</p>
                      <p className="text-sm">
                        <span className="text-emerald-500">{stat.winningTrades}</span>
                        <span className="text-muted-foreground"> / </span>
                        <span className="text-red-500">{stat.losingTrades}</span>
                      </p>
                    </div>
                    <div>
                      <p className="text-[10px] text-muted-foreground">{t.strategyWinRate.avgPnl}</p>
                      <p className={`text-sm font-bold ${stat.avgPnl >= 0 ? 'text-emerald-500' : 'text-red-500'}`}>
                        {stat.avgPnl >= 0 ? '+' : ''}${stat.avgPnl.toFixed(2)}
                      </p>
                    </div>
                    <div>
                      <p className="text-[10px] text-muted-foreground">{t.strategyWinRate.best}</p>
                      <p className="text-sm font-bold text-emerald-500">+${stat.bestTrade.toFixed(2)}</p>
                    </div>
                    <div>
                      <p className="text-[10px] text-muted-foreground">{t.strategyWinRate.worst}</p>
                      <p className="text-sm font-bold text-red-500">${stat.worstTrade.toFixed(2)}</p>
                    </div>
                  </div>

                  {/* Win Rate Bar */}
                  <div className="mt-2">
                    <div className="h-1.5 rounded-full overflow-hidden bg-muted flex">
                      <div
                        className="bg-emerald-500"
                        style={{ width: `${stat.winRate}%` }}
                      />
                      <div
                        className="bg-red-500"
                        style={{ width: `${100 - stat.winRate}%` }}
                      />
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}

        {strategyStats.length === 0 && !loading && (
          <div className="flex flex-col items-center justify-center py-8 text-muted-foreground">
            <Target className="w-8 h-8 mb-2 opacity-50" />
            <p className="text-sm">{t.strategyWinRate.noData}</p>
            <p className="text-xs mt-1">{t.strategyWinRate.noDataDesc}</p>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
