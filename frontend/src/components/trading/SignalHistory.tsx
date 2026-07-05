'use client';

import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  History, Loader2, RefreshCw, Trophy, Target, ArrowUp, ArrowDown, Clock
} from 'lucide-react';
import type { SignalHistoryResult, SignalRecord } from '@/types/backtest';

interface Props {
  symbol?: string;
  limit?: number;
}

export default function SignalHistory({ symbol, limit = 50 }: Props) {
  const [data, setData] = useState<SignalHistoryResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchData = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await invoke<SignalHistoryResult>('get_signal_history', {
        request: { symbol, limit }
      });
      setData(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [symbol, limit]);

  useEffect(() => { fetchData(); }, [fetchData]);

  if (loading && !data) {
    return (
      <Card>
        <CardContent className="flex items-center justify-center py-12">
          <Loader2 className="w-6 h-6 animate-spin mr-2" />
          <span className="text-muted-foreground">加载信号历史...</span>
        </CardContent>
      </Card>
    );
  }

  if (error) {
    return (
      <Card>
        <CardContent className="flex flex-col items-center justify-center py-8 gap-2">
          <span className="text-destructive text-sm">{error}</span>
          <Button variant="outline" size="sm" onClick={fetchData}>重试</Button>
        </CardContent>
      </Card>
    );
  }

  if (!data) return null;

  const { stats, signals } = data;
  const winRate = parseFloat(stats.win_rate) || 0;

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <CardTitle className="text-base flex items-center gap-2">
            <History className="w-4 h-4" />
            信号历史
            {symbol && <Badge variant="outline" className="text-xs">{symbol}</Badge>}
          </CardTitle>
          <Button variant="ghost" size="sm" onClick={fetchData} disabled={loading} className="h-7 w-7 p-0">
            <RefreshCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
          </Button>
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        {/* 统计卡片 */}
        <div className="grid grid-cols-4 gap-2">
          <StatCard icon={<Target className="w-4 h-4" />} label="总信号" value={String(stats.total_signals)} />
          <StatCard icon={<Trophy className="w-4 h-4" />} label="确认率" value={`${winRate.toFixed(1)}%`}
            highlight={winRate >= 60 ? 'green' : winRate >= 40 ? 'yellow' : 'red'} />
          <StatCard icon={<ArrowUp className="w-4 h-4" />} label="平均收益" value={stats.avg_win_pnl} highlight="green" />
          <StatCard icon={<ArrowDown className="w-4 h-4" />} label="平均亏损" value={stats.avg_loss_pnl} highlight="red" />
        </div>

        {/* 信号列表 */}
        {signals.length > 0 ? (
          <div className="space-y-1 max-h-[300px] overflow-y-auto">
            {signals.map((s) => <SignalRow key={s.id} signal={s} />)}
          </div>
        ) : (
          <div className="text-center py-8 text-muted-foreground text-sm">暂无信号记录</div>
        )}
      </CardContent>
    </Card>
  );
}

function StatCard({ icon, label, value, highlight }: {
  icon: React.ReactNode; label: string; value: string;
  highlight?: 'green' | 'red' | 'yellow';
}) {
  const cls = highlight === 'green' ? 'text-green-500'
    : highlight === 'red' ? 'text-red-500'
    : highlight === 'yellow' ? 'text-yellow-500' : '';
  return (
    <div className="rounded-lg border bg-muted/30 p-2 text-center">
      <div className="flex justify-center text-muted-foreground mb-1">{icon}</div>
      <div className={`text-sm font-bold ${cls}`}>{value}</div>
      <div className="text-[10px] text-muted-foreground">{label}</div>
    </div>
  );
}

function SignalRow({ signal }: { signal: SignalRecord }) {
  const isBullish = signal.direction === 'bullish';

  // 状态颜色
  const statusConfig: Record<string, { label: string; color: string }> = {
    confirmed:   { label: '已确认', color: 'bg-green-500/10 text-green-500 border-green-500/20' },
    invalidated: { label: '已失效', color: 'bg-red-500/10 text-red-500 border-red-500/20' },
    expired:     { label: '已过期', color: 'bg-gray-500/10 text-gray-400 border-gray-500/20' },
    superseded:  { label: '被取代', color: 'bg-yellow-500/10 text-yellow-500 border-yellow-500/20' },
    pending:     { label: '待验证', color: 'bg-blue-500/10 text-blue-500 border-blue-500/20' },
  };
  const cfg = statusConfig[signal.outcome || ''] || { label: signal.outcome || '-', color: '' };

  return (
    <div className="flex items-center justify-between py-1.5 px-2 rounded hover:bg-muted/50 text-sm gap-2">
      {/* 时间 */}
      <span className="text-xs text-muted-foreground w-[90px] shrink-0">
        {new Date(signal.timestamp).toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })}
      </span>

      {/* 交易对 */}
      <span className="font-medium w-[80px] shrink-0">{signal.symbol}</span>

      {/* 方向 */}
      <Badge variant="outline" className={`w-[50px] justify-center text-xs shrink-0 ${
        isBullish ? 'border-green-500/50 text-green-500' : 'border-red-500/50 text-red-500'
      }`}>
        {isBullish ? '看涨' : '看跌'}
      </Badge>

      {/* 价格 */}
      <span className="text-right w-[80px] shrink-0">${parseFloat(signal.price).toFixed(2)}</span>

      {/* 状态 */}
      <Badge variant="outline" className={`w-[60px] justify-center text-xs shrink-0 ${cfg.color}`}>
        {cfg.label}
      </Badge>

      {/* 收益 */}
      <span className={`text-right w-[70px] shrink-0 font-medium ${
        signal.pnl?.startsWith('+') ? 'text-green-500'
          : signal.pnl?.startsWith('-') ? 'text-red-500'
          : 'text-muted-foreground'
      }`}>
        {signal.pnl || '-'}
      </span>
    </div>
  );
}
