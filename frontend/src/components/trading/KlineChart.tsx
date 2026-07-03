'use client';

import { useEffect, useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Loader2, CandlestickChart, RefreshCw } from 'lucide-react';
import {
  AreaChart, Area, XAxis, YAxis, CartesianGrid, Tooltip,
  ResponsiveContainer, BarChart, Bar
} from 'recharts';
import { KlineData } from '@/types/trading';

const TIMEFRAMES = [
  { value: '1m', label: '1m' },
  { value: '5m', label: '5m' },
  { value: '15m', label: '15m' },
  { value: '30m', label: '30m' },
  { value: '1h', label: '1H' },
  { value: '4h', label: '4H' },
  { value: '1d', label: '1D' },
];

interface KlineChartProps {
  symbol: string;
}

export default function KlineChart({ symbol }: KlineChartProps) {
  const [klines, setKlines] = useState<KlineData[]>([]);
  const [loading, setLoading] = useState(true);
  const [timeframe, setTimeframe] = useState('1h');

  const fetchKlines = useCallback(async () => {
    if (!symbol) return;
    try {
      setLoading(true);
      const result = await invoke<KlineData[]>('get_kline_history', {
        request: {
          symbol,
          timeframe,
          limit: 100
        }
      });
      setKlines(result);
    } catch (err) {
      console.error('Failed to fetch klines:', err);
      setKlines([]);
    } finally {
      setLoading(false);
    }
  }, [symbol, timeframe]);

  useEffect(() => {
    fetchKlines();
  }, [fetchKlines]);

  // 后端返回按时间倒序，图表需要正序（从左到右时间增大）
  const sortedKlines = [...klines].reverse();

  const chartData = sortedKlines.map((k, i) => ({
    time: formatTime(k.timestamp, timeframe),
    price: parseFloat(k.close),
    volume: parseFloat(k.volume),
    high: parseFloat(k.high),
    low: parseFloat(k.low),
    open: parseFloat(k.open),
    isUp: parseFloat(k.close) >= parseFloat(k.open),
    index: i,
  }));

  const currentPrice = klines.length > 0 ? parseFloat(klines[klines.length - 1].close) : 0;
  const prevPrice = klines.length > 1 ? parseFloat(klines[klines.length - 2].close) : currentPrice;
  const priceChange = prevPrice > 0 ? ((currentPrice - prevPrice) / prevPrice) * 100 : 0;

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <CandlestickChart className="w-5 h-5 text-muted-foreground" />
            <CardTitle className="text-lg">{symbol}</CardTitle>
            {currentPrice > 0 && (
              <div className="flex items-center gap-2">
                <span className="text-xl font-bold">${currentPrice.toLocaleString('en-US', { minimumFractionDigits: 2 })}</span>
                <Badge variant={priceChange >= 0 ? 'default' : 'destructive'} className="text-xs">
                  {priceChange >= 0 ? '+' : ''}{priceChange.toFixed(2)}%
                </Badge>
              </div>
            )}
          </div>
          <div className="flex items-center gap-2">
            <div className="flex bg-muted rounded-md p-0.5">
              {TIMEFRAMES.map((tf) => (
                <button
                  key={tf.value}
                  onClick={() => setTimeframe(tf.value)}
                  className={`px-2 py-1 text-xs font-medium rounded-sm transition-colors ${
                    timeframe === tf.value
                      ? 'bg-background text-foreground shadow-sm'
                      : 'text-muted-foreground hover:text-foreground'
                  }`}
                >
                  {tf.label}
                </button>
              ))}
            </div>
            <Button variant="ghost" size="sm" onClick={fetchKlines} disabled={loading}>
              <RefreshCw className={`w-3 h-3 ${loading ? 'animate-spin' : ''}`} />
            </Button>
          </div>
        </div>
      </CardHeader>
      <CardContent>
        {loading && klines.length === 0 ? (
          <div className="flex items-center justify-center h-64">
            <Loader2 className="w-5 h-5 animate-spin mr-2" />
            <span className="text-sm text-muted-foreground">Loading chart...</span>
          </div>
        ) : chartData.length > 0 ? (
          <div className="space-y-4">
            {/* Price Area Chart */}
            <div className="h-64">
              <ResponsiveContainer width="100%" height="100%">
                <AreaChart data={chartData}>
                  <defs>
                    <linearGradient id="priceGradient" x1="0" y1="0" x2="0" y2="1">
                      <stop offset="5%" stopColor={priceChange >= 0 ? '#10b981' : '#ef4444'} stopOpacity={0.3} />
                      <stop offset="95%" stopColor={priceChange >= 0 ? '#10b981' : '#ef4444'} stopOpacity={0} />
                    </linearGradient>
                  </defs>
                  <CartesianGrid strokeDasharray="3 3" className="opacity-30" />
                  <XAxis
                    dataKey="time"
                    tick={{ fontSize: 10 }}
                    interval="preserveStartEnd"
                  />
                  <YAxis
                    domain={['dataMin - 10', 'dataMax + 10']}
                    tick={{ fontSize: 10 }}
                    tickFormatter={(v) => `$${v.toLocaleString()}`}
                    width={80}
                  />
                  <Tooltip
                    content={({ active, payload }) => {
                      if (!active || !payload?.length) return null;
                      const d = payload[0].payload;
                      return (
                        <div className="bg-background border rounded-lg shadow-lg p-3 text-xs">
                          <p className="font-medium mb-1">{d.time}</p>
                          <div className="grid grid-cols-2 gap-x-4 gap-y-0.5">
                            <span className="text-muted-foreground">Open:</span>
                            <span className="font-mono">${d.open.toFixed(2)}</span>
                            <span className="text-muted-foreground">High:</span>
                            <span className="font-mono text-emerald-500">${d.high.toFixed(2)}</span>
                            <span className="text-muted-foreground">Low:</span>
                            <span className="font-mono text-red-500">${d.low.toFixed(2)}</span>
                            <span className="text-muted-foreground">Close:</span>
                            <span className="font-mono font-bold">${d.price.toFixed(2)}</span>
                            <span className="text-muted-foreground">Volume:</span>
                            <span className="font-mono">{d.volume.toFixed(2)}</span>
                          </div>
                        </div>
                      );
                    }}
                  />
                  <Area
                    type="monotone"
                    dataKey="price"
                    stroke={priceChange >= 0 ? '#10b981' : '#ef4444'}
                    strokeWidth={2}
                    fill="url(#priceGradient)"
                  />
                </AreaChart>
              </ResponsiveContainer>
            </div>

            {/* Volume Bar Chart */}
            <div className="h-20">
              <ResponsiveContainer width="100%" height="100%">
                <BarChart data={chartData}>
                  <XAxis dataKey="time" tick={false} axisLine={false} />
                  <YAxis hide />
                  <Tooltip
                    content={({ active, payload }) => {
                      if (!active || !payload?.length) return null;
                      const d = payload[0].payload;
                      return (
                        <div className="bg-background border rounded-lg shadow-lg p-2 text-xs">
                          <p>Volume: {d.volume.toFixed(2)}</p>
                        </div>
                      );
                    }}
                  />
                  <Bar
                    dataKey="volume"
                    fill={priceChange >= 0 ? '#10b981' : '#ef4444'}
                    opacity={0.5}
                  />
                </BarChart>
              </ResponsiveContainer>
            </div>
          </div>
        ) : (
          <div className="flex items-center justify-center h-64 text-muted-foreground">
            No data available
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function formatTime(timestamp: string, timeframe: string): string {
  const date = new Date(timestamp);
  switch (timeframe) {
    case '1m':
    case '5m':
    case '15m':
    case '30m':
      return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    case '1h':
    case '4h':
      return date.toLocaleDateString([], { month: 'short', day: 'numeric' }) + ' ' +
             date.toLocaleTimeString([], { hour: '2-digit' });
    case '1d':
      return date.toLocaleDateString([], { month: 'short', day: 'numeric' });
    default:
      return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }
}
