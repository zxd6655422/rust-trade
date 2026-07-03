'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Loader2, Receipt, RefreshCw } from 'lucide-react';
import { BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer } from 'recharts';
import { CommissionStats as CommissionStatsType } from '@/types/trading';
import { useLanguage } from '@/lib/i18n/context';

interface CommissionStatsProps {
  symbol?: string;
  days?: number;
}

export default function CommissionStats({ symbol, days = 30 }: CommissionStatsProps) {
  const [stats, setStats] = useState<CommissionStatsType | null>(null);
  const [loading, setLoading] = useState(true);
  const { t } = useLanguage();

  const fetchStats = async () => {
    try {
      setLoading(true);
      const result = await invoke<CommissionStatsType>('get_commission_stats', {
        request: {
          symbol: symbol || null,
          days
        }
      });
      setStats(result);
    } catch (err) {
      console.error('Failed to fetch commission stats:', err);
      setStats(null);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchStats();
  }, [symbol, days]);

  if (loading) {
    return (
      <div className="flex items-center justify-center py-8">
        <Loader2 className="w-5 h-5 animate-spin mr-2" />
        <span className="text-sm text-muted-foreground">{t.common.loading}</span>
      </div>
    );
  }

  if (!stats) return null;

  const monthlyData = stats.commission_by_month.map(m => ({
    month: m.month,
    commission: parseFloat(m.total_commission),
    trades: m.trade_count,
  }));

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Receipt className="w-5 h-5 text-muted-foreground" />
            <CardTitle className="text-lg">{t.commissionStats.title}</CardTitle>
          </div>
          <Button variant="ghost" size="sm" onClick={fetchStats} disabled={loading}>
            <RefreshCw className={`w-3 h-3 ${loading ? 'animate-spin' : ''}`} />
          </Button>
        </div>
      </CardHeader>
      <CardContent>
        {/* Summary */}
        <div className="grid grid-cols-2 gap-3 mb-4">
          <div className="p-3 rounded-lg border bg-orange-50/50 dark:bg-orange-950/20 dark:border-orange-800">
            <p className="text-xs text-muted-foreground">{t.commissionStats.totalCommission}</p>
            <p className="text-xl font-bold text-orange-600">
              ${parseFloat(stats.total_commission).toFixed(2)}
            </p>
          </div>
          <div className="p-3 rounded-lg border">
            <p className="text-xs text-muted-foreground">{t.commissionStats.avgPerTrade}</p>
            <p className="text-xl font-bold">
              ${parseFloat(stats.avg_commission_per_trade).toFixed(4)}
            </p>
          </div>
        </div>

        {/* By Symbol */}
        {stats.commission_by_symbol.length > 0 && (
          <div className="mb-4">
            <h4 className="text-xs font-medium text-muted-foreground mb-2">{t.commissionStats.bySymbol}</h4>
            <div className="space-y-1.5">
              {stats.commission_by_symbol.map((s) => (
                <div key={s.symbol} className="flex items-center justify-between text-sm">
                  <span className="font-medium">{s.symbol}</span>
                  <div className="flex items-center gap-3">
                    <span className="text-xs text-muted-foreground">{s.trade_count} {t.common.trades}</span>
                    <span className="font-mono">${parseFloat(s.total_commission).toFixed(2)}</span>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Monthly Chart */}
        {monthlyData.length > 0 && (
          <div>
            <h4 className="text-xs font-medium text-muted-foreground mb-2">{t.commissionStats.monthlyTrend}</h4>
            <div className="h-32">
              <ResponsiveContainer width="100%" height="100%">
                <BarChart data={monthlyData}>
                  <CartesianGrid strokeDasharray="3 3" className="opacity-30" />
                  <XAxis dataKey="month" tick={{ fontSize: 10 }} />
                  <YAxis tick={{ fontSize: 10 }} tickFormatter={(v) => `$${v}`} width={50} />
                  <Tooltip
                    content={({ active, payload }) => {
                      if (!active || !payload?.length) return null;
                      const d = payload[0].payload;
                      return (
                        <div className="bg-background border rounded-lg shadow-lg p-2 text-xs">
                          <p className="font-medium">{d.month}</p>
                          <p>{t.commissionStats.title}: ${d.commission.toFixed(2)}</p>
                          <p>{t.common.trades}: {d.trades}</p>
                        </div>
                      );
                    }}
                  />
                  <Bar dataKey="commission" fill="#f97316" opacity={0.8} radius={[4, 4, 0, 0]} />
                </BarChart>
              </ResponsiveContainer>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
