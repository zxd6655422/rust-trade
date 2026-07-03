'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Loader2, TrendingUp, RefreshCw } from 'lucide-react';
import { AreaChart, Area, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer } from 'recharts';
import { EquityCurvePoint } from '@/types/trading';
import { useLanguage } from '@/lib/i18n/context';

interface EquityCurveProps {
  symbol?: string;
  days?: number;
  period?: string;
}

export default function EquityCurve({ symbol, days = 90, period = 'daily' }: EquityCurveProps) {
  const [data, setData] = useState<EquityCurvePoint[]>([]);
  const [loading, setLoading] = useState(true);
  const { t } = useLanguage();

  const fetchData = async () => {
    try {
      setLoading(true);
      const result = await invoke<EquityCurvePoint[]>('get_equity_curve', {
        request: {
          symbol: symbol || null,
          period,
          days
        }
      });
      setData(result);
    } catch (err) {
      console.error('Failed to fetch equity curve:', err);
      setData([]);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchData();
  }, [symbol, days, period]);

  // 确保数据按日期正序（从左到右时间增大）
  const sortedData = [...data].sort((a, b) => a.date.localeCompare(b.date));

  // 计算累计权益
  const chartData = sortedData.map((point, i) => {
    const cumulative = sortedData
      .slice(0, i + 1)
      .reduce((sum, p) => sum + parseFloat(p.pnl || '0'), 0);
    return {
      date: point.date,
      pnl: parseFloat(point.pnl || '0'),
      cumulative,
    };
  });

  const totalReturn = chartData.length > 0 ? chartData[chartData.length - 1].cumulative : 0;

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <TrendingUp className="w-5 h-5 text-muted-foreground" />
            <CardTitle className="text-lg">{t.equityCurve.title}</CardTitle>
            {totalReturn !== 0 && (
              <span className={`text-sm font-bold ${totalReturn >= 0 ? 'text-emerald-500' : 'text-red-500'}`}>
                {totalReturn >= 0 ? '+' : ''}${totalReturn.toLocaleString('en-US', { minimumFractionDigits: 2 })}
              </span>
            )}
          </div>
          <Button variant="ghost" size="sm" onClick={fetchData} disabled={loading}>
            <RefreshCw className={`w-3 h-3 ${loading ? 'animate-spin' : ''}`} />
          </Button>
        </div>
      </CardHeader>
      <CardContent>
        {loading ? (
          <div className="flex items-center justify-center h-48">
            <Loader2 className="w-5 h-5 animate-spin mr-2" />
            <span className="text-sm text-muted-foreground">{t.common.loading}</span>
          </div>
        ) : chartData.length > 0 ? (
          <div className="h-48">
            <ResponsiveContainer width="100%" height="100%">
              <AreaChart data={chartData}>
                <defs>
                  <linearGradient id="equityGradient" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="5%" stopColor={totalReturn >= 0 ? '#10b981' : '#ef4444'} stopOpacity={0.3} />
                    <stop offset="95%" stopColor={totalReturn >= 0 ? '#10b981' : '#ef4444'} stopOpacity={0} />
                  </linearGradient>
                </defs>
                <CartesianGrid strokeDasharray="3 3" className="opacity-30" />
                <XAxis dataKey="date" tick={{ fontSize: 10 }} interval="preserveStartEnd" />
                <YAxis
                  tick={{ fontSize: 10 }}
                  tickFormatter={(v) => `$${v.toLocaleString()}`}
                  width={70}
                />
                <Tooltip
                  content={({ active, payload }) => {
                    if (!active || !payload?.length) return null;
                    const d = payload[0].payload;
                    return (
                      <div className="bg-background border rounded-lg shadow-lg p-3 text-xs">
                        <p className="font-medium mb-1">{d.date}</p>
                        <p>PnL: <span className={d.pnl >= 0 ? 'text-emerald-500' : 'text-red-500'}>${d.pnl.toFixed(2)}</span></p>
                        <p>{t.equityCurve.cumulative}: <span className="font-bold">${d.cumulative.toFixed(2)}</span></p>
                      </div>
                    );
                  }}
                />
                <Area
                  type="monotone"
                  dataKey="cumulative"
                  stroke={totalReturn >= 0 ? '#10b981' : '#ef4444'}
                  strokeWidth={2}
                  fill="url(#equityGradient)"
                />
              </AreaChart>
            </ResponsiveContainer>
          </div>
        ) : (
          <div className="flex items-center justify-center h-48 text-muted-foreground text-sm">
            {t.common.noData}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
