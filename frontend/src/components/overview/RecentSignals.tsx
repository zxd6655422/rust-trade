'use client';

import React, { useEffect, useState, useCallback } from 'react';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Radio, CheckCircle2, XCircle, Clock, AlertCircle } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { useLanguage } from '@/lib/i18n/context';

interface SignalRecord {
  id: string;
  timestamp: string;
  symbol: string;
  direction: string;
  price: string;
  outcome: string;
  pnl?: string;
}

interface SignalHistoryResult {
  signals: SignalRecord[];
  stats: {
    total_signals: number;
    win_count: number;
    loss_count: number;
    win_rate: number;
    avg_win_pnl: string;
    avg_loss_pnl: string;
  };
}

const statusConfig: Record<string, { icon: React.ElementType; color: string; bgColor: string }> = {
  confirmed: { icon: CheckCircle2, color: 'text-emerald-500', bgColor: 'bg-emerald-500/10' },
  invalidated: { icon: XCircle, color: 'text-red-500', bgColor: 'bg-red-500/10' },
  expired: { icon: Clock, color: 'text-slate-400', bgColor: 'bg-slate-400/10' },
  superseded: { icon: AlertCircle, color: 'text-amber-500', bgColor: 'bg-amber-500/10' },
  pending: { icon: Clock, color: 'text-blue-500', bgColor: 'bg-blue-500/10' },
};

const RecentSignals: React.FC = () => {
  const { t } = useLanguage();
  const [signals, setSignals] = useState<SignalRecord[]>([]);
  const [loading, setLoading] = useState(true);

  const fetchSignals = useCallback(async () => {
    try {
      const result = await invoke<SignalHistoryResult>('get_signal_history', {
        request: { symbol: null, strategy_id: null, limit: 50 },
      });
      setSignals(result.signals);
    } catch (err) {
      console.error('Failed to fetch signals:', err);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchSignals();
    const interval = setInterval(fetchSignals, 30000);
    return () => clearInterval(interval);
  }, [fetchSignals]);

  const displaySignals = signals.slice(0, 5);

  const getDirectionLabel = (direction: string) => {
    return direction === 'long' ? t.autoTrading.directionLong : t.autoTrading.directionShort;
  };

  const getStatusLabel = (outcome: string) => {
    const key = `status${outcome.charAt(0).toUpperCase() + outcome.slice(1)}` as keyof typeof t.autoTrading;
    return (t.autoTrading as Record<string, string>)[key] || outcome;
  };

  return (
    <Card className="h-full">
      <CardHeader className="pb-3">
        <CardTitle className="text-sm font-medium flex items-center gap-2">
          <Radio className="w-4 h-4" />
          {t.overview.recentSignals}
        </CardTitle>
      </CardHeader>
      <CardContent>
        {loading ? (
          <div className="flex items-center justify-center py-8 text-muted-foreground text-sm">
            {t.common.loading}
          </div>
        ) : displaySignals.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-8 text-muted-foreground">
            <Radio className="w-8 h-8 mb-2 opacity-50" />
            <p className="text-sm">{t.overview.noSignals}</p>
            <p className="text-xs mt-1">{t.overview.noSignalsDesc}</p>
          </div>
        ) : (
          <div className="space-y-3">
            {displaySignals.map((signal) => {
              const config = statusConfig[signal.outcome] || statusConfig.pending;
              const Icon = config.icon;
              const pnlValue = signal.pnl ? parseFloat(signal.pnl.replace(/[^-\d.]/g, '')) : null;

              return (
                <div
                  key={signal.id}
                  className="flex items-center justify-between py-1.5 border-b border-border/50 last:border-0"
                >
                  <div className="flex items-center gap-2 min-w-0">
                    <div className={`p-1 rounded ${config.bgColor}`}>
                      <Icon className={`w-3 h-3 ${config.color}`} />
                    </div>
                    <span className="text-sm font-medium font-mono truncate">
                      {signal.symbol}
                    </span>
                    <Badge
                      variant={signal.direction === 'long' ? 'default' : 'destructive'}
                      className="text-[10px] px-1.5 py-0 shrink-0"
                    >
                      {getDirectionLabel(signal.direction)}
                    </Badge>
                  </div>
                  <div className="flex items-center gap-2 shrink-0">
                    {pnlValue !== null && (
                      <span
                        className={`text-xs font-mono ${
                          pnlValue >= 0
                            ? 'text-emerald-600 dark:text-emerald-400'
                            : 'text-red-600 dark:text-red-400'
                        }`}
                      >
                        {pnlValue >= 0 ? '+' : ''}{signal.pnl}
                      </span>
                    )}
                    <span className="text-[10px] text-muted-foreground">
                      {getStatusLabel(signal.outcome)}
                    </span>
                  </div>
                </div>
              );
            })}
            {signals.length > 5 && (
              <p className="text-xs text-muted-foreground text-center pt-1">
                {t.overview.viewAll} →
              </p>
            )}
          </div>
        )}
      </CardContent>
    </Card>
  );
};

export default RecentSignals;
