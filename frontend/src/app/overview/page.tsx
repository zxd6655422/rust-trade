'use client';

import React, { useEffect, useState, useCallback } from 'react';
import { Loader2 } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { useLanguage } from '@/lib/i18n/context';
import StatCards from '@/components/overview/StatCards';
import ActivePositions from '@/components/overview/ActivePositions';
import RecentSignals from '@/components/overview/RecentSignals';
import SystemStatus from '@/components/overview/SystemStatus';
import EquityCurve from '@/components/trading/EquityCurve';

// 账户快照（来自 account_snapshot 表）
interface AccountSnapshot {
  exchange: string;
  market_type: string;
  snapshot_at: string;
  total_equity: string;
  total_balance: string;
  available_balance: string;
  frozen_balance: string;
  unrealized_pnl: string;
  initial_margin: string | null;
  maint_margin: string | null;
  margin_ratio: string | null;
  position_count: number;
}

// 持仓快照（来自 position_snapshot 表）
interface AccountPosition {
  exchange: string;
  symbol: string;
  raw_symbol: string;
  snapshot_at: string;
  position_side: string;
  position_amt: string;
  entry_price: string;
  mark_price: string;
  unrealized_pnl: string;
  leverage: number;
  margin_type: string;
  notional: string;
  liquidation_price: string | null;
}

interface SignalStats {
  total_signals: number;
  confirmed: number;
  invalidated: number;
  expired: number;
  pending: number;
  win_rate: number;
  avg_return: number;
}

// 从 localStorage 读取交易所配置
function getExchangeConfig(): string {
  try {
    const raw = localStorage.getItem('exchange_configs');
    if (raw) {
      const configs = JSON.parse(raw);
      if (configs.length > 0) return configs[0].id;
    }
  } catch {}
  return 'binance';
}

export default function OverviewPage() {
  const { t } = useLanguage();
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [lastUpdate, setLastUpdate] = useState<string>('');

  // Data states — 使用快照数据
  const [snapshot, setSnapshot] = useState<AccountSnapshot | null>(null);
  const [positions, setPositions] = useState<AccountPosition[]>([]);
  const [signalStats, setSignalStats] = useState<SignalStats | null>(null);

  const fetchData = useCallback(async () => {
    const exchange = getExchangeConfig();
    const errors: string[] = [];

    // 1. 账户快照（总资产、未实现盈亏等）
    try {
      const result = await invoke<AccountSnapshot | null>('get_account_snapshot', {
        exchange,
        marketType: 'futures',
      });
      if (result) setSnapshot(result);
    } catch (err) {
      console.error('get_account_snapshot failed:', err);
      errors.push(`Snapshot: ${err}`);
    }

    // 2. 持仓快照
    try {
      const result = await invoke<AccountPosition[]>('get_account_positions', { exchange });
      setPositions(result);
    } catch (err) {
      console.error('get_account_positions failed:', err);
      errors.push(`Positions: ${err}`);
    }

    // 3. 信号统计
    try {
      const result = await invoke<{ stats: SignalStats }>('get_signal_stats', {
        request: {
          table: 'strategy_analysis_log',
          symbol: null,
          strategyId: null,
        }
      });
      if (result?.stats) setSignalStats(result.stats);
    } catch (err) {
      console.error('get_signal_stats failed:', err);
      // 信号表可能不存在，不报错
    }

    setLastUpdate(new Date().toLocaleTimeString());
    if (errors.length > 0) {
      setError(errors.join(' | '));
    } else {
      setError(null);
    }
    setLoading(false);
  }, []);

  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, 30000);
    return () => clearInterval(interval);
  }, [fetchData]);

  // Compute stat values from snapshot
  const totalAssets = snapshot?.total_equity
    ? `$${parseFloat(snapshot.total_equity).toLocaleString()}`
    : '$0';
  const unrealizedPnl = snapshot?.unrealized_pnl
    ? parseFloat(snapshot.unrealized_pnl)
    : 0;
  const todayPnl = snapshot?.unrealized_pnl || '$0';
  const todayPnlCount = snapshot
    ? `${snapshot.position_count} 持仓`
    : '';
  const winRateValue = signalStats?.win_rate
    ? `${signalStats.win_rate}%`
    : '--';
  const positionSymbols = positions.length > 0
    ? positions.map(p => p.symbol).join(', ')
    : '';
  const signalCount = signalStats?.total_signals || 0;
  const pendingCount = signalStats?.pending || 0;

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="flex items-center gap-2 text-muted-foreground">
          <Loader2 className="w-5 h-5 animate-spin" />
          <span>{t.common.loading}</span>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold">{t.overview.title}</h1>
          <p className="text-sm text-muted-foreground">{t.overview.subtitle}</p>
        </div>
        {lastUpdate && (
          <span className="text-xs text-muted-foreground">
            {t.overview.lastUpdate}: {lastUpdate}
          </span>
        )}
      </div>

      {/* Error Banner */}
      {error && (
        <div className="p-3 rounded-lg border border-red-200 bg-red-50 dark:bg-red-950/20 dark:border-red-800">
          <p className="text-sm text-red-600 dark:text-red-400">{error}</p>
        </div>
      )}

      {/* Stat Cards */}
      <StatCards
        totalAssets={totalAssets}
        todayPnl={todayPnl}
        todayPnlCount={todayPnlCount}
        positionCount={positions.length}
        positionSymbols={positionSymbols}
        winRate={winRateValue}
        signalCount={signalCount}
        pendingCount={pendingCount}
      />

      {/* Active Positions — 整行 */}
      <ActivePositions />

      {/* Equity Curve + Signals + System Status */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        <div className="lg:col-span-2">
          <EquityCurve />
        </div>
        <div className="space-y-6">
          <RecentSignals />
          <SystemStatus />
        </div>
      </div>
    </div>
  );
}
