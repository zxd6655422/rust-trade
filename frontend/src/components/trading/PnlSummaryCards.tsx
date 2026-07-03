'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import {
  Loader2, TrendingUp, TrendingDown, Trophy, Skull,
  Percent, RefreshCw
} from 'lucide-react';
import { PnlSummary } from '@/types/trading';
import { useLanguage } from '@/lib/i18n/context';

interface PnlSummaryCardsProps {
  symbol?: string;
  days?: number;
}

export default function PnlSummaryCards({ symbol, days = 30 }: PnlSummaryCardsProps) {
  const [summary, setSummary] = useState<PnlSummary | null>(null);
  const [loading, setLoading] = useState(true);
  const { t } = useLanguage();

  const fetchSummary = async () => {
    try {
      setLoading(true);
      const result = await invoke<PnlSummary>('get_pnl_summary', {
        request: {
          symbol: symbol || null,
          days
        }
      });
      setSummary(result);
    } catch (err) {
      console.error('Failed to fetch PnL summary:', err);
      setSummary(null);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchSummary();
  }, [symbol, days]);

  const formatValue = (val?: string) => {
    if (!val) return '$0.00';
    const num = parseFloat(val);
    return `$${num.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center py-8">
        <Loader2 className="w-5 h-5 animate-spin mr-2" />
        <span className="text-sm text-muted-foreground">{t.common.loading}</span>
      </div>
    );
  }

  if (!summary) return null;

  const totalPnl = parseFloat(summary.total_pnl || '0');
  const winRate = parseFloat(summary.win_rate || '0');

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium text-muted-foreground">
          {t.pnlSummary.title} ({days}D)
        </h3>
        <Button variant="ghost" size="sm" onClick={fetchSummary} disabled={loading}>
          <RefreshCw className={`w-3 h-3 ${loading ? 'animate-spin' : ''}`} />
        </Button>
      </div>

      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        {/* Total PnL */}
        <Card className={totalPnl >= 0 ? 'border-emerald-200 bg-emerald-50/50 dark:border-emerald-800 dark:bg-emerald-950/20' : 'border-red-200 bg-red-50/50 dark:border-red-800 dark:bg-red-950/20'}>
          <CardContent className="p-4">
            <div className="flex items-center gap-2 mb-1">
              {totalPnl >= 0 ? (
                <TrendingUp className="w-4 h-4 text-emerald-500" />
              ) : (
                <TrendingDown className="w-4 h-4 text-red-500" />
              )}
              <span className="text-xs text-muted-foreground">{t.pnlSummary.totalPnl}</span>
            </div>
            <div className={`text-xl font-bold ${totalPnl >= 0 ? 'text-emerald-600' : 'text-red-600'}`}>
              {totalPnl >= 0 ? '+' : ''}{formatValue(summary.total_pnl)}
            </div>
            <p className="text-[10px] text-muted-foreground mt-1">
              {summary.total_trades} {t.common.trades}
            </p>
          </CardContent>
        </Card>

        {/* Win Rate */}
        <Card>
          <CardContent className="p-4">
            <div className="flex items-center gap-2 mb-1">
              <Percent className="w-4 h-4 text-blue-500" />
              <span className="text-xs text-muted-foreground">{t.pnlSummary.winRate}</span>
            </div>
            <div className="text-xl font-bold text-blue-600">
              {winRate.toFixed(1)}%
            </div>
            <div className="flex gap-2 mt-1">
              <span className="text-[10px] text-emerald-500">{t.pnlSummary.wins}: {summary.winning_trades}</span>
              <span className="text-[10px] text-red-500">{t.pnlSummary.losses}: {summary.losing_trades}</span>
            </div>
          </CardContent>
        </Card>

        {/* Best Trade */}
        <Card className="border-emerald-200 dark:border-emerald-800">
          <CardContent className="p-4">
            <div className="flex items-center gap-2 mb-1">
              <Trophy className="w-4 h-4 text-emerald-500" />
              <span className="text-xs text-muted-foreground">{t.pnlSummary.bestTrade}</span>
            </div>
            <div className="text-xl font-bold text-emerald-600">
              +{formatValue(summary.best_trade)}
            </div>
          </CardContent>
        </Card>

        {/* Worst Trade */}
        <Card className="border-red-200 dark:border-red-800">
          <CardContent className="p-4">
            <div className="flex items-center gap-2 mb-1">
              <Skull className="w-4 h-4 text-red-500" />
              <span className="text-xs text-muted-foreground">{t.pnlSummary.worstTrade}</span>
            </div>
            <div className="text-xl font-bold text-red-600">
              {formatValue(summary.worst_trade)}
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Additional Stats Row */}
      <div className="grid grid-cols-3 gap-3">
        <Card>
          <CardContent className="p-3 text-center">
            <p className="text-[10px] text-muted-foreground">{t.pnlSummary.avgPnl}</p>
            <p className="text-sm font-bold">{formatValue(summary.avg_pnl)}</p>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="p-3 text-center">
            <p className="text-[10px] text-muted-foreground">{t.tradeHistory.commission}</p>
            <p className="text-sm font-bold text-orange-500">{formatValue(summary.total_commission)}</p>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="p-3 text-center">
            <p className="text-[10px] text-muted-foreground">{t.pnlSummary.netPnl}</p>
            <p className={`text-sm font-bold ${totalPnl >= 0 ? 'text-emerald-600' : 'text-red-600'}`}>
              {formatValue(summary.total_pnl)}
            </p>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
