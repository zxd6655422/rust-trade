'use client';

import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  TrendingUp, TrendingDown, Minus, RefreshCw,
  ArrowUpRight, ArrowDownRight, Loader2, Target, Shield
} from 'lucide-react';
import type { StrategyAnalysisResult, TimeframeAnalysis } from '@/types/backtest';

interface Props {
  symbol: string;
  autoRefreshInterval?: number; // ms, 0 = disabled
}

export default function StrategyAnalysisPanel({ symbol, autoRefreshInterval = 0 }: Props) {
  const [analysis, setAnalysis] = useState<StrategyAnalysisResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lastUpdate, setLastUpdate] = useState<Date | null>(null);

  const fetchAnalysis = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await invoke<StrategyAnalysisResult>('get_strategy_analysis', {
        request: { symbol, strategy_id: 'trend' }
      });
      setAnalysis(result);
      setLastUpdate(new Date());
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [symbol]);

  useEffect(() => {
    fetchAnalysis();
  }, [fetchAnalysis]);

  useEffect(() => {
    if (autoRefreshInterval <= 0) return;
    const timer = setInterval(fetchAnalysis, autoRefreshInterval);
    return () => clearInterval(timer);
  }, [fetchAnalysis, autoRefreshInterval]);

  const getDirectionIcon = (direction: string) => {
    switch (direction) {
      case 'bullish': return <TrendingUp className="w-5 h-5 text-green-500" />;
      case 'bearish': return <TrendingDown className="w-5 h-5 text-red-500" />;
      default: return <Minus className="w-5 h-5 text-gray-400" />;
    }
  };

  const getDirectionColor = (direction: string) => {
    switch (direction) {
      case 'bullish': return 'text-green-500';
      case 'bearish': return 'text-red-500';
      default: return 'text-gray-400';
    }
  };

  const getDirectionBg = (direction: string) => {
    switch (direction) {
      case 'bullish': return 'bg-green-500/10 border-green-500/20';
      case 'bearish': return 'bg-red-500/10 border-red-500/20';
      default: return 'bg-gray-500/10 border-gray-500/20';
    }
  };

  const getDirectionLabel = (direction: string) => {
    switch (direction) {
      case 'bullish': return '看涨';
      case 'bearish': return '看跌';
      default: return '中性';
    }
  };

  const formatConfidence = (confidence: string) => {
    const val = parseFloat(confidence);
    if (isNaN(val)) return '0%';
    // 如果是 0-1 范围就乘 100，如果是 0-100 范围就直接用
    if (val <= 1) return `${(val * 100).toFixed(0)}%`;
    return `${val.toFixed(0)}%`;
  };

  if (loading && !analysis) {
    return (
      <Card>
        <CardContent className="flex items-center justify-center py-12">
          <Loader2 className="w-6 h-6 animate-spin mr-2" />
          <span className="text-muted-foreground">分析中...</span>
        </CardContent>
      </Card>
    );
  }

  if (error) {
    return (
      <Card>
        <CardContent className="flex flex-col items-center justify-center py-8 gap-2">
          <span className="text-destructive text-sm">{error}</span>
          <Button variant="outline" size="sm" onClick={fetchAnalysis}>
            重试
          </Button>
        </CardContent>
      </Card>
    );
  }

  if (!analysis) return null;

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <CardTitle className="text-base flex items-center gap-2">
            <Target className="w-4 h-4" />
            策略分析
            <Badge variant="outline" className="text-xs font-normal">
              {analysis.strategy_name}
            </Badge>
          </CardTitle>
          <Button
            variant="ghost"
            size="sm"
            onClick={fetchAnalysis}
            disabled={loading}
            className="h-7 w-7 p-0"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
          </Button>
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        {/* 时间框架分析卡片 */}
        <div className="grid grid-cols-3 gap-2">
          {analysis.timeframes.map((tf) => (
            <TimeframeCard
              key={tf.timeframe}
              analysis={tf}
              getDirectionIcon={getDirectionIcon}
              getDirectionColor={getDirectionColor}
              getDirectionBg={getDirectionBg}
              getDirectionLabel={getDirectionLabel}
              formatConfidence={formatConfidence}
            />
          ))}
        </div>

        {/* 综合判断 */}
        <div className={`rounded-lg border p-3 ${getDirectionBg(analysis.overall_direction)}`}>
          <div className="flex items-center justify-between mb-2">
            <div className="flex items-center gap-2">
              <Shield className="w-4 h-4 text-muted-foreground" />
              <span className="text-sm font-medium">综合判断</span>
            </div>
            <div className="flex items-center gap-2">
              {getDirectionIcon(analysis.overall_direction)}
              <span className={`font-bold text-lg ${getDirectionColor(analysis.overall_direction)}`}>
                {getDirectionLabel(analysis.overall_direction)}
              </span>
            </div>
          </div>
          <div className="flex items-center justify-between text-sm">
            <span className="text-muted-foreground">置信度</span>
            <span className="font-medium">{formatConfidence(analysis.overall_confidence)}</span>
          </div>
          <div className="flex items-center justify-between text-sm mt-1">
            <span className="text-muted-foreground">入场建议</span>
            {analysis.entry_allowed ? (
              <span className="flex items-center gap-1 font-medium text-green-500">
                {analysis.entry_direction === 'long' ? (
                  <><ArrowUpRight className="w-4 h-4" /> 可做多</>
                ) : (
                  <><ArrowDownRight className="w-4 h-4" /> 可做空</>
                )}
              </span>
            ) : (
              <span className="text-muted-foreground">观望</span>
            )}
          </div>
        </div>

        {/* 更新时间 */}
        {lastUpdate && (
          <div className="text-xs text-muted-foreground text-right">
            更新: {lastUpdate.toLocaleTimeString()}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// 单个时间框架卡片
function TimeframeCard({
  analysis: tf,
  getDirectionIcon,
  getDirectionColor,
  getDirectionBg,
  getDirectionLabel,
  formatConfidence,
}: {
  analysis: TimeframeAnalysis;
  getDirectionIcon: (d: string) => React.ReactNode;
  getDirectionColor: (d: string) => string;
  getDirectionBg: (d: string) => string;
  getDirectionLabel: (d: string) => string;
  formatConfidence: (c: string) => string;
}) {
  return (
    <div className={`rounded-lg border p-2.5 text-center ${getDirectionBg(tf.direction)}`}>
      <div className="text-xs text-muted-foreground font-medium mb-1.5">
        {tf.timeframe.toUpperCase()}
      </div>
      <div className="flex justify-center mb-1">
        {getDirectionIcon(tf.direction)}
      </div>
      <div className={`text-sm font-bold ${getDirectionColor(tf.direction)}`}>
        {getDirectionLabel(tf.direction)}
      </div>
      <div className="text-xs text-muted-foreground mt-0.5">
        {formatConfidence(tf.confidence)}
      </div>
      {/* 策略描述 */}
      <div className="text-[10px] text-muted-foreground mt-1.5 leading-tight line-clamp-2">
        {tf.description}
      </div>
    </div>
  );
}
