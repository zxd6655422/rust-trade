'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Loader2, BarChart3, RefreshCw, TrendingUp, TrendingDown,
  Activity, Target, Zap, Award
} from 'lucide-react';
import { PerformanceMetrics } from '@/types/trading';
import { useLanguage } from '@/lib/i18n/context';

interface PerformancePanelProps {
  symbol?: string;
  days?: number;
}

export default function PerformancePanel({ symbol, days = 30 }: PerformancePanelProps) {
  const [metrics, setMetrics] = useState<PerformanceMetrics | null>(null);
  const [loading, setLoading] = useState(true);
  const { t } = useLanguage();

  const fetchMetrics = async () => {
    try {
      setLoading(true);
      const result = await invoke<PerformanceMetrics>('get_performance_metrics', {
        request: {
          symbol: symbol || null,
          days
        }
      });
      setMetrics(result);
    } catch (err) {
      console.error('Failed to fetch performance metrics:', err);
      setMetrics(null);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchMetrics();
  }, [symbol, days]);

  if (loading) {
    return (
      <div className="flex items-center justify-center py-8">
        <Loader2 className="w-5 h-5 animate-spin mr-2" />
        <span className="text-sm text-muted-foreground">{t.common.loading}</span>
      </div>
    );
  }

  if (!metrics) return null;

  const metricItems = [
    {
      label: t.performancePanel.sharpeRatio,
      value: parseFloat(metrics.sharpe_ratio).toFixed(2),
      icon: Activity,
      color: parseFloat(metrics.sharpe_ratio) > 1 ? 'text-emerald-500' : parseFloat(metrics.sharpe_ratio) > 0 ? 'text-yellow-500' : 'text-red-500',
      desc: t.performancePanel.sharpeDesc
    },
    {
      label: t.performancePanel.sortinoRatio,
      value: parseFloat(metrics.sortino_ratio).toFixed(2),
      icon: Target,
      color: parseFloat(metrics.sortino_ratio) > 1 ? 'text-emerald-500' : parseFloat(metrics.sortino_ratio) > 0 ? 'text-yellow-500' : 'text-red-500',
      desc: t.performancePanel.sortinoDesc
    },
    {
      label: t.performancePanel.maxDrawdown,
      value: `${parseFloat(metrics.max_drawdown).toFixed(2)}%`,
      icon: TrendingDown,
      color: 'text-red-500',
      desc: t.performancePanel.maxDrawdownDesc
    },
    {
      label: t.performancePanel.calmarRatio,
      value: parseFloat(metrics.calmar_ratio).toFixed(2),
      icon: Zap,
      color: parseFloat(metrics.calmar_ratio) > 1 ? 'text-emerald-500' : 'text-yellow-500',
      desc: t.performancePanel.calmarDesc
    },
    {
      label: t.performancePanel.winRate,
      value: `${parseFloat(metrics.win_rate).toFixed(1)}%`,
      icon: Award,
      color: parseFloat(metrics.win_rate) > 50 ? 'text-emerald-500' : 'text-red-500',
      desc: `${metrics.winning_trades}W / ${metrics.losing_trades}L`
    },
    {
      label: t.performancePanel.profitFactor,
      value: parseFloat(metrics.profit_factor).toFixed(2),
      icon: TrendingUp,
      color: parseFloat(metrics.profit_factor) > 1.5 ? 'text-emerald-500' : parseFloat(metrics.profit_factor) > 1 ? 'text-yellow-500' : 'text-red-500',
      desc: t.performancePanel.profitFactorDesc
    },
  ];

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <BarChart3 className="w-5 h-5 text-muted-foreground" />
            <CardTitle className="text-lg">{t.performancePanel.title}</CardTitle>
            <Badge variant="outline">{days}D</Badge>
          </div>
          <Button variant="ghost" size="sm" onClick={fetchMetrics} disabled={loading}>
            <RefreshCw className={`w-3 h-3 ${loading ? 'animate-spin' : ''}`} />
          </Button>
        </div>
      </CardHeader>
      <CardContent>
        <div className="grid grid-cols-2 md:grid-cols-3 gap-4">
          {metricItems.map((item) => {
            const Icon = item.icon;
            return (
              <div key={item.label} className="p-3 rounded-lg border bg-card hover:bg-muted/50 transition-colors">
                <div className="flex items-center gap-2 mb-2">
                  <Icon className={`w-4 h-4 ${item.color}`} />
                  <span className="text-xs text-muted-foreground">{item.label}</span>
                </div>
                <p className={`text-lg font-bold ${item.color}`}>{item.value}</p>
                <p className="text-[10px] text-muted-foreground mt-1">{item.desc}</p>
              </div>
            );
          })}
        </div>

        {/* Detailed Stats */}
        <div className="mt-4 pt-4 border-t">
          <div className="grid grid-cols-2 md:grid-cols-4 gap-3 text-center">
            <div>
              <p className="text-[10px] text-muted-foreground">{t.performancePanel.totalTrades}</p>
              <p className="text-sm font-bold">{metrics.total_trades}</p>
            </div>
            <div>
              <p className="text-[10px] text-muted-foreground">{t.performancePanel.avgWin}</p>
              <p className="text-sm font-bold text-emerald-500">${parseFloat(metrics.avg_win).toFixed(2)}</p>
            </div>
            <div>
              <p className="text-[10px] text-muted-foreground">{t.performancePanel.avgLoss}</p>
              <p className="text-sm font-bold text-red-500">${parseFloat(metrics.avg_loss).toFixed(2)}</p>
            </div>
            <div>
              <p className="text-[10px] text-muted-foreground">{t.performancePanel.volatility}</p>
              <p className="text-sm font-bold">{parseFloat(metrics.volatility).toFixed(2)}%</p>
            </div>
          </div>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-3 text-center mt-3">
            <div>
              <p className="text-[10px] text-muted-foreground">{t.performancePanel.largestWin}</p>
              <p className="text-sm font-bold text-emerald-500">${parseFloat(metrics.largest_win).toFixed(2)}</p>
            </div>
            <div>
              <p className="text-[10px] text-muted-foreground">{t.performancePanel.largestLoss}</p>
              <p className="text-sm font-bold text-red-500">${parseFloat(metrics.largest_loss).toFixed(2)}</p>
            </div>
            <div>
              <p className="text-[10px] text-muted-foreground">{t.performancePanel.consecWins}</p>
              <p className="text-sm font-bold">{metrics.consecutive_wins}</p>
            </div>
            <div>
              <p className="text-[10px] text-muted-foreground">{t.performancePanel.consecLosses}</p>
              <p className="text-sm font-bold">{metrics.consecutive_losses}</p>
            </div>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
