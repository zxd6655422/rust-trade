'use client';

import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Loader2, Play, Pause, CheckCircle, XCircle, Clock,
  TrendingUp, TrendingDown, Zap, ArrowUpRight, ArrowDownRight
} from 'lucide-react';
import { useLanguage } from '@/lib/i18n/context';

interface SignalRecord {
  id: string;
  symbol: string;
  strategy_id: string;
  direction: string;
  entry_price: string;
  overall_confidence: string;
  status: string;
  closed_reason: string | null;
  actual_return_pct: string | null;
  created_at: string;
  closed_at: string | null;
}

interface SignalStats {
  total_signals: number;
  confirmed: number;
  invalidated: number;
  expired: number;
  pending: number;
  win_rate: string;
  avg_return: string;
}

interface AutoTradingStatusProps {
  symbol?: string;
}

export default function AutoTradingStatus({ symbol }: AutoTradingStatusProps) {
  const [signals, setSignals] = useState<SignalRecord[]>([]);
  const [stats, setStats] = useState<SignalStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [isRunning, setIsRunning] = useState(true);
  const { t } = useLanguage();

  const fetchSchedulerStatus = async () => {
    try {
      const status = await invoke<{ is_running: boolean; is_paused: boolean }>('get_scheduler_status');
      setIsRunning(status.is_running);
    } catch (err) {
      console.error('Failed to fetch scheduler status:', err);
    }
  };

  const fetchData = async () => {
    try {
      setLoading(true);
      await fetchSchedulerStatus();
      const [signalsResult, statsResult] = await Promise.all([
        invoke<{ signals: SignalRecord[] }>('get_signal_history', {
          request: {
            symbol: symbol || null,
            strategyId: 'trend',
            limit: 10,
          }
        }),
        invoke<{ stats: SignalStats }>('get_signal_stats', {
          request: {
            table: 'strategy_analysis_log',
            symbol: symbol || null,
            strategyId: 'trend',
          }
        })
      ]);
      setSignals(signalsResult.signals || []);
      setStats(statsResult.stats || null);
    } catch (err) {
      console.error('Failed to fetch auto trading status:', err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, 30000);
    return () => clearInterval(interval);
  }, [symbol]);

  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'confirmed':
        return <CheckCircle className="w-4 h-4 text-emerald-500" />;
      case 'invalidated':
        return <XCircle className="w-4 h-4 text-red-500" />;
      case 'expired':
        return <Clock className="w-4 h-4 text-gray-500" />;
      case 'superseded':
        return <ArrowUpRight className="w-4 h-4 text-yellow-500" />;
      case 'pending':
        return <Loader2 className="w-4 h-4 text-blue-500 animate-spin" />;
      default:
        return null;
    }
  };

  const getStatusLabel = (status: string, reason: string | null) => {
    const labels: Record<string, string> = {
      confirmed: '已确认',
      invalidated: '已止损',
      expired: '已过期',
      superseded: '已替代',
      pending: '验证中',
    };
    const label = labels[status] || status;
    if (reason) {
      const reasonLabels: Record<string, string> = {
        take_profit: '止盈',
        stop_loss: '止损',
        price_confirmed: '价格确认',
        direction_changed: '方向反转',
        timeout: '超时',
      };
      return `${label} (${reasonLabels[reason] || reason})`;
    }
    return label;
  };

  const getStatusColor = (status: string) => {
    switch (status) {
      case 'confirmed': return 'text-emerald-500';
      case 'invalidated': return 'text-red-500';
      case 'pending': return 'text-blue-500';
      default: return 'text-gray-500';
    }
  };

  const formatReturn = (returnStr: string | null) => {
    if (!returnStr) return null;
    const val = parseFloat(returnStr);
    if (isNaN(val)) return null;
    return (
      <span className={val >= 0 ? 'text-emerald-500' : 'text-red-500'}>
        {val >= 0 ? '+' : ''}{val.toFixed(2)}%
      </span>
    );
  };

  const pendingCount = signals.filter(s => s.status === 'pending').length;
  const recentConfirmed = signals.filter(s => s.status === 'confirmed').slice(0, 3);

  const handleToggleScheduler = async () => {
    try {
      if (isRunning) {
        await invoke('pause_scheduler');
        setIsRunning(false);
      } else {
        await invoke('resume_scheduler');
        setIsRunning(true);
      }
    } catch (err) {
      console.error('Failed to toggle scheduler:', err);
    }
  };

  if (loading && !stats) {
    return (
      <Card>
        <CardContent className="py-8">
          <div className="flex items-center justify-center">
            <Loader2 className="w-5 h-5 animate-spin mr-2" />
            <span className="text-sm text-muted-foreground">加载中...</span>
          </div>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <CardTitle className="text-base flex items-center gap-2">
            <Zap className="w-4 h-4" />
            自动交易状态
            <Badge variant={isRunning ? 'default' : 'secondary'} className="text-xs">
              {isRunning ? '运行中' : '已暂停'}
            </Badge>
          </CardTitle>
          <div className="flex items-center gap-2">
            {pendingCount > 0 && (
              <Badge variant="outline" className="text-xs text-blue-500">
                {pendingCount} 个信号验证中
              </Badge>
            )}
            <Button
              variant="ghost"
              size="sm"
              onClick={handleToggleScheduler}
              className="h-7"
            >
              {isRunning ? (
                <><Pause className="w-3.5 h-3.5 mr-1" /> 暂停</>
              ) : (
                <><Play className="w-3.5 h-3.5 mr-1" /> 启动</>
              )}
            </Button>
          </div>
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        {/* 统计概览 */}
        {stats && (
          <div className="grid grid-cols-4 gap-3">
            <div className="p-2 rounded-lg bg-muted/30 text-center">
              <div className="text-2xl font-bold">{stats.total_signals}</div>
              <div className="text-[10px] text-muted-foreground">总信号</div>
            </div>
            <div className="p-2 rounded-lg bg-emerald-500/10 text-center">
              <div className="text-2xl font-bold text-emerald-500">{stats.confirmed}</div>
              <div className="text-[10px] text-muted-foreground">已确认</div>
            </div>
            <div className="p-2 rounded-lg bg-red-500/10 text-center">
              <div className="text-2xl font-bold text-red-500">{stats.invalidated}</div>
              <div className="text-[10px] text-muted-foreground">已止损</div>
            </div>
            <div className="p-2 rounded-lg bg-blue-500/10 text-center">
              <div className="text-2xl font-bold text-blue-500">{stats.pending}</div>
              <div className="text-[10px] text-muted-foreground">验证中</div>
            </div>
          </div>
        )}

        {/* 胜率和平均收益 */}
        {stats && (
          <div className="grid grid-cols-2 gap-3">
            <div className="flex items-center justify-between p-2 rounded-lg bg-muted/30">
              <span className="text-xs text-muted-foreground">胜率</span>
              <span className={`font-bold ${
                parseFloat(stats.win_rate) >= 50 ? 'text-emerald-500' : 'text-red-500'
              }`}>
                {stats.win_rate}%
              </span>
            </div>
            <div className="flex items-center justify-between p-2 rounded-lg bg-muted/30">
              <span className="text-xs text-muted-foreground">平均收益</span>
              <span className={`font-bold ${
                parseFloat(stats.avg_return) >= 0 ? 'text-emerald-500' : 'text-red-500'
              }`}>
                {stats.avg_return}%
              </span>
            </div>
          </div>
        )}

        {/* 最近信号 */}
        <div>
          <div className="text-xs font-medium text-muted-foreground mb-2">最近信号</div>
          <div className="space-y-1.5 max-h-[200px] overflow-y-auto">
            {signals.length === 0 ? (
              <div className="text-center py-4 text-sm text-muted-foreground">
                暂无信号记录
              </div>
            ) : (
              signals.map((signal) => (
                <div
                  key={signal.id}
                  className="flex items-center justify-between p-2 rounded-md bg-muted/20 hover:bg-muted/40 transition-colors"
                >
                  <div className="flex items-center gap-2">
                    {getStatusIcon(signal.status)}
                    <div>
                      <div className="flex items-center gap-1.5">
                        <span className="text-sm font-medium">{signal.symbol}</span>
                        <Badge
                          variant="outline"
                          className={`text-[10px] px-1 py-0 ${
                            signal.direction === 'bullish'
                              ? 'border-emerald-500/30 text-emerald-500'
                              : 'border-red-500/30 text-red-500'
                          }`}
                        >
                          {signal.direction === 'bullish' ? '多' : '空'}
                        </Badge>
                      </div>
                      <div className="text-[10px] text-muted-foreground">
                        {getStatusLabel(signal.status, signal.closed_reason)}
                      </div>
                    </div>
                  </div>
                  <div className="text-right">
                    {formatReturn(signal.actual_return_pct)}
                    <div className="text-[10px] text-muted-foreground">
                      {new Date(signal.created_at).toLocaleTimeString('zh-CN', {
                        hour: '2-digit',
                        minute: '2-digit'
                      })}
                    </div>
                  </div>
                </div>
              ))
            )}
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
