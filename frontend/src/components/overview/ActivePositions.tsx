'use client';

import React, { useEffect, useState, useCallback } from 'react';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { ArrowUpRight, ArrowDownRight, BarChart3, AlertTriangle, Coins } from 'lucide-react';
import SpotBalances from './SpotBalances';
import { invoke } from '@tauri-apps/api/core';
import { useLanguage } from '@/lib/i18n/context';

interface AccountPosition {
  exchange: string;
  symbol: string;
  position_side: string;
  position_amt: string;
  entry_price: string;
  mark_price: string;
  unrealized_pnl: string;
  leverage: number;
  margin_type: string;
  initial_margin: string;
  maint_margin: string;
  notional: string;
  liquidation_price: string | null;
  break_even_price: string | null;
}

function getExchange(): string {
  try {
    const raw = localStorage.getItem('exchange_configs');
    if (raw) {
      const configs = JSON.parse(raw);
      if (configs.length > 0) return configs[0].id;
    }
  } catch {}
  return 'binance';
}

const ActivePositions: React.FC = () => {
  const { t } = useLanguage();
  const [tab, setTab] = useState<'futures' | 'spot'>('futures');
  const [positions, setPositions] = useState<AccountPosition[]>([]);
  const [loading, setLoading] = useState(true);

  const fetchPositions = useCallback(async () => {
    try {
      const exchange = getExchange();
      const result = await invoke<AccountPosition[]>('get_account_positions', { exchange });
      setPositions(result);
    } catch (err) {
      console.error('Failed to fetch positions:', err);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchPositions();
    const interval = setInterval(fetchPositions, 15000);
    return () => clearInterval(interval);
  }, [fetchPositions]);

  const fmt = (v: string | null | undefined, decimals = 2) => {
    if (!v) return '--';
    const n = parseFloat(v);
    return isNaN(n) ? '--' : n.toFixed(decimals);
  };

  const fmtK = (v: string | null | undefined) => {
    if (!v) return '--';
    const n = parseFloat(v);
    if (isNaN(n)) return '--';
    if (Math.abs(n) >= 1000) return `$${(n / 1000).toFixed(1)}K`;
    return `$${n.toFixed(2)}`;
  };

  return (
    <Card className="h-full">
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-1">
            <button
              onClick={() => setTab('futures')}
              className={`flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-medium transition-colors ${
                tab === 'futures'
                  ? 'bg-primary text-primary-foreground'
                  : 'text-muted-foreground hover:text-foreground hover:bg-muted'
              }`}
            >
              <BarChart3 className="w-3.5 h-3.5" />
              {t.overview.futuresPositions}
              {positions.length > 0 && (
                <Badge variant={tab === 'futures' ? 'secondary' : 'outline'} className="ml-1 text-[10px] px-1 py-0">
                  {positions.length}
                </Badge>
              )}
            </button>
            <button
              onClick={() => setTab('spot')}
              className={`flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-medium transition-colors ${
                tab === 'spot'
                  ? 'bg-primary text-primary-foreground'
                  : 'text-muted-foreground hover:text-foreground hover:bg-muted'
              }`}
            >
              <Coins className="w-3.5 h-3.5" />
              {t.overview.spotPositions}
            </button>
          </div>
        </div>
      </CardHeader>
      <CardContent>
        {tab === 'spot' ? (
          <SpotBalances />
        ) : loading ? (
          <div className="flex items-center justify-center py-8 text-muted-foreground text-sm">
            {t.common.loading}
          </div>
        ) : positions.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-8 text-muted-foreground">
            <BarChart3 className="w-8 h-8 mb-2 opacity-50" />
            <p className="text-sm">{t.overview.noPositions}</p>
            <p className="text-xs mt-1">{t.overview.noPositionsDesc}</p>
          </div>
        ) : (
          <div className="space-y-2">
            {/* 表头 */}
            <div className="grid grid-cols-[100px_60px_1fr_1fr_1fr_1fr_60px_1fr] gap-2 px-2 py-1 text-[10px] text-muted-foreground uppercase tracking-wider border-b">
              <span>交易对</span>
              <span>方向</span>
              <span className="text-right">开仓价</span>
              <span className="text-right">数量</span>
              <span className="text-right">保证金</span>
              <span className="text-right">未实现盈亏</span>
              <span className="text-right">杠杆</span>
              <span className="text-right">爆仓价</span>
            </div>

            {/* 持仓行 */}
            {positions.map((pos, idx) => {
              const pnl = parseFloat(pos.unrealized_pnl || '0');
              const isPositive = pnl >= 0;
              const isLong = pos.position_side === 'LONG';
              const amt = parseFloat(pos.position_amt || '0');

              // 计算盈亏比例
              const entryVal = parseFloat(pos.entry_price || '0') * Math.abs(amt);
              const pnlPct = entryVal > 0 ? ((pnl / entryVal) * 100).toFixed(2) : '0.00';

              return (
                <div
                  key={idx}
                  className="grid grid-cols-[100px_60px_1fr_1fr_1fr_1fr_60px_1fr] gap-2 items-center px-2 py-2 rounded-md hover:bg-muted/50 transition-colors border-b border-border/30 last:border-0"
                >
                  {/* 交易对 */}
                  <span className="text-sm font-semibold font-mono truncate">
                    {pos.symbol}
                  </span>

                  {/* 方向 */}
                  <Badge
                    variant={isLong ? 'default' : 'destructive'}
                    className="text-[10px] px-1.5 py-0 justify-center w-fit"
                  >
                    {isLong ? '多' : '空'}
                  </Badge>

                  {/* 开仓价 */}
                  <span className="text-sm font-mono text-right">
                    ${fmt(pos.entry_price)}
                  </span>

                  {/* 数量 */}
                  <span className="text-sm font-mono text-right">
                    {fmt(pos.position_amt, 4)}
                  </span>

                  {/* 保证金 */}
                  <span className="text-sm font-mono text-right text-muted-foreground">
                    {fmtK(pos.initial_margin)}
                  </span>

                  {/* 未实现盈亏 */}
                  <div className="flex items-center justify-end gap-1">
                    {isPositive ? (
                      <ArrowUpRight className="w-3 h-3 text-emerald-500 shrink-0" />
                    ) : (
                      <ArrowDownRight className="w-3 h-3 text-red-500 shrink-0" />
                    )}
                    <div className="text-right">
                      <span
                        className={`text-sm font-mono font-medium block leading-tight ${
                          isPositive
                            ? 'text-emerald-600 dark:text-emerald-400'
                            : 'text-red-600 dark:text-red-400'
                        }`}
                      >
                        {isPositive ? '+' : ''}${fmt(pos.unrealized_pnl)}
                      </span>
                      <span
                        className={`text-[10px] font-mono block leading-tight ${
                          isPositive
                            ? 'text-emerald-500/70'
                            : 'text-red-500/70'
                        }`}
                      >
                        {isPositive ? '+' : ''}{pnlPct}%
                      </span>
                    </div>
                  </div>

                  {/* 杠杆 */}
                  <span className="text-sm font-mono text-right text-amber-500 font-medium">
                    {pos.leverage}x
                  </span>

                  {/* 爆仓价 */}
                  <span className={`text-sm font-mono text-right ${
                    pos.liquidation_price
                      ? 'text-red-500/80'
                      : 'text-muted-foreground'
                  }`}>
                    {pos.liquidation_price ? `$${fmt(pos.liquidation_price)}` : '--'}
                  </span>
                </div>
              );
            })}
          </div>
        )}
      </CardContent>
    </Card>
  );
};

export default ActivePositions;
