'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Card, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Loader2, TrendingUp, DollarSign, Percent,
  RefreshCw, ArrowUpRight, ArrowDownRight, Trophy, Target
} from 'lucide-react';
import { PnlSummary, PerformanceMetrics } from '@/types/trading';
import { useLanguage } from '@/lib/i18n/context';

interface AccountProfitDashboardProps {
  symbol?: string;
}

export default function AccountProfitDashboard({ symbol }: AccountProfitDashboardProps) {
  const [pnl, setPnl] = useState<PnlSummary | null>(null);
  const [metrics, setMetrics] = useState<PerformanceMetrics | null>(null);
  const [loading, setLoading] = useState(true);
  const { t } = useLanguage();

  const fetchData = async () => {
    try {
      setLoading(true);
      const [pnlResult, metricsResult] = await Promise.all([
        invoke<PnlSummary>('get_pnl_summary', {
          request: { symbol: symbol || null, days: 30 }
        }),
        invoke<PerformanceMetrics>('get_performance_metrics', {
          request: { symbol: symbol || null, days: 30 }
        })
      ]);
      setPnl(pnlResult);
      setMetrics(metricsResult);
    } catch (err) {
      console.error('Failed to fetch profit data:', err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchData();
    // 每 30 秒刷新
    const interval = setInterval(fetchData, 30000);
    return () => clearInterval(interval);
  }, [symbol]);

  if (loading) {
    return (
      <Card className="bg-gradient-to-r from-slate-50 to-slate-100 dark:from-slate-900 dark:to-slate-800">
        <CardContent className="py-8">
          <div className="flex items-center justify-center">
            <Loader2 className="w-6 h-6 animate-spin mr-2" />
            <span className="text-muted-foreground">{t.common.loading}</span>
          </div>
        </CardContent>
      </Card>
    );
  }

  if (!pnl || !metrics) return null;

  const totalPnl = parseFloat(pnl.total_pnl || '0');
  const winRate = parseFloat(metrics.win_rate || '0');
  const sharpe = parseFloat(metrics.sharpe_ratio || '0');
  const maxDd = parseFloat(metrics.max_drawdown || '0');
  const profitFactor = parseFloat(metrics.profit_factor || '0');

  return (
    <Card className="bg-gradient-to-br from-slate-900 via-blue-950 to-slate-900 border-blue-800/50 shadow-lg">
      <CardContent className="p-6">
        <div className="flex items-center justify-between mb-4">
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 bg-blue-600 rounded-xl flex items-center justify-center">
              <DollarSign className="w-5 h-5 text-white" />
            </div>
            <div>
              <h2 className="text-lg font-bold text-white">{t.trading.accountProfit}</h2>
              <p className="text-xs text-blue-300">30 {t.common.trades}</p>
            </div>
          </div>
          <Button variant="ghost" size="sm" onClick={fetchData} className="text-blue-300 hover:text-white">
            <RefreshCw className={`w-4 h-4 ${loading ? 'animate-spin' : ''}`} />
          </Button>
        </div>

        <div className="grid grid-cols-2 md:grid-cols-5 gap-4">
          {/* 总盈亏 - 最醒目 */}
          <div className="col-span-2 md:col-span-1">
            <div className={`p-4 rounded-xl ${
              totalPnl >= 0
                ? 'bg-emerald-500/20 border border-emerald-500/30'
                : 'bg-red-500/20 border border-red-500/30'
            }`}>
              <div className="flex items-center gap-2 mb-1">
                {totalPnl >= 0 ? (
                  <ArrowUpRight className="w-4 h-4 text-emerald-400" />
                ) : (
                  <ArrowDownRight className="w-4 h-4 text-red-400" />
                )}
                <span className="text-xs text-gray-300">{t.pnlSummary.totalPnl}</span>
              </div>
              <div className={`text-2xl font-bold ${
                totalPnl >= 0 ? 'text-emerald-400' : 'text-red-400'
              }`}>
                {totalPnl >= 0 ? '+' : ''}${Math.abs(totalPnl).toLocaleString('en-US', { minimumFractionDigits: 2 })}
              </div>
            </div>
          </div>

          {/* 胜率 */}
          <div className="p-3 rounded-xl bg-white/5 border border-white/10">
            <div className="flex items-center gap-2 mb-1">
              <Target className="w-4 h-4 text-blue-400" />
              <span className="text-xs text-gray-400">{t.pnlSummary.winRate}</span>
            </div>
            <div className={`text-xl font-bold ${winRate >= 50 ? 'text-emerald-400' : 'text-red-400'}`}>
              {winRate.toFixed(1)}%
            </div>
            <div className="flex gap-2 mt-1">
              <span className="text-[10px] text-emerald-400">W:{pnl.winning_trades}</span>
              <span className="text-[10px] text-red-400">L:{pnl.losing_trades}</span>
            </div>
          </div>

          {/* 夏普比率 */}
          <div className="p-3 rounded-xl bg-white/5 border border-white/10">
            <div className="flex items-center gap-2 mb-1">
              <TrendingUp className="w-4 h-4 text-purple-400" />
              <span className="text-xs text-gray-400">{t.performancePanel.sharpeRatio}</span>
            </div>
            <div className={`text-xl font-bold ${
              sharpe > 1 ? 'text-emerald-400' : sharpe > 0 ? 'text-yellow-400' : 'text-red-400'
            }`}>
              {sharpe.toFixed(2)}
            </div>
          </div>

          {/* 盈亏比 */}
          <div className="p-3 rounded-xl bg-white/5 border border-white/10">
            <div className="flex items-center gap-2 mb-1">
              <Trophy className="w-4 h-4 text-yellow-400" />
              <span className="text-xs text-gray-400">{t.performancePanel.profitFactor}</span>
            </div>
            <div className={`text-xl font-bold ${
              profitFactor > 1.5 ? 'text-emerald-400' : profitFactor > 1 ? 'text-yellow-400' : 'text-red-400'
            }`}>
              {profitFactor.toFixed(2)}
            </div>
          </div>

          {/* 最大回撤 */}
          <div className="p-3 rounded-xl bg-white/5 border border-white/10">
            <div className="flex items-center gap-2 mb-1">
              <Percent className="w-4 h-4 text-red-400" />
              <span className="text-xs text-gray-400">{t.performancePanel.maxDrawdown}</span>
            </div>
            <div className="text-xl font-bold text-red-400">
              {maxDd.toFixed(2)}%
            </div>
          </div>
        </div>

        {/* 底部统计条 */}
        <div className="mt-4 pt-3 border-t border-white/10 flex items-center justify-between text-xs">
          <div className="flex items-center gap-4">
            <span className="text-gray-400">
              {t.strategyWinRate.totalTrades}: <span className="text-white font-bold">{pnl.total_trades}</span>
            </span>
            <span className="text-gray-400">
              {t.pnlSummary.bestTrade}: <span className="text-emerald-400 font-bold">+${parseFloat(pnl.best_trade || '0').toFixed(2)}</span>
            </span>
            <span className="text-gray-400">
              {t.pnlSummary.worstTrade}: <span className="text-red-400 font-bold">${parseFloat(pnl.worst_trade || '0').toFixed(2)}</span>
            </span>
          </div>
          <Badge variant="outline" className="text-blue-300 border-blue-500/50">
            {t.trading.liveData}
          </Badge>
        </div>
      </CardContent>
    </Card>
  );
}
