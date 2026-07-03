'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Loader2, Crosshair, RefreshCw, ArrowUpRight, ArrowDownRight } from 'lucide-react';
import { PositionInfo } from '@/types/trading';
import { useLanguage } from '@/lib/i18n/context';

export default function PositionTable() {
  const [positions, setPositions] = useState<PositionInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const { t } = useLanguage();

  const fetchPositions = async () => {
    try {
      setLoading(true);
      const result = await invoke<PositionInfo[]>('get_positions');
      setPositions(result);
    } catch (err) {
      console.error('Failed to fetch positions:', err);
      setPositions([]);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchPositions();
    const interval = setInterval(fetchPositions, 15000);
    return () => clearInterval(interval);
  }, []);

  const formatPrice = (price?: string) => {
    if (!price) return '-';
    const num = parseFloat(price);
    return num.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
  };

  const formatPnl = (pnl?: string) => {
    if (!pnl) return null;
    const num = parseFloat(pnl);
    const isPositive = num >= 0;
    return (
      <span className={`flex items-center gap-1 font-medium ${isPositive ? 'text-emerald-500' : 'text-red-500'}`}>
        {isPositive ? <ArrowUpRight className="w-3 h-3" /> : <ArrowDownRight className="w-3 h-3" />}
        {isPositive ? '+' : ''}{formatPrice(pnl)}
      </span>
    );
  };

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Crosshair className="w-5 h-5 text-muted-foreground" />
            <CardTitle className="text-lg">{t.positionTable.title}</CardTitle>
            <Badge variant="secondary">{positions.length}</Badge>
          </div>
          <Button variant="ghost" size="sm" onClick={fetchPositions} disabled={loading}>
            <RefreshCw className={`w-3 h-3 ${loading ? 'animate-spin' : ''}`} />
          </Button>
        </div>
      </CardHeader>
      <CardContent>
        {loading ? (
          <div className="flex items-center justify-center py-8">
            <Loader2 className="w-5 h-5 animate-spin mr-2" />
            <span className="text-sm text-muted-foreground">{t.common.loading}</span>
          </div>
        ) : positions.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-8 text-muted-foreground">
            <Crosshair className="w-8 h-8 mb-2 opacity-50" />
            <p className="text-sm">{t.positionTable.noPositions}</p>
            <p className="text-xs mt-1">{t.positionTable.noPositionsDesc}</p>
          </div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b text-left">
                  <th className="pb-2 font-medium text-muted-foreground">{t.positionTable.symbol}</th>
                  <th className="pb-2 font-medium text-muted-foreground">{t.positionTable.side}</th>
                  <th className="pb-2 font-medium text-muted-foreground text-right">{t.positionTable.quantity}</th>
                  <th className="pb-2 font-medium text-muted-foreground text-right">{t.positionTable.entryPrice}</th>
                  <th className="pb-2 font-medium text-muted-foreground text-right">{t.positionTable.current}</th>
                  <th className="pb-2 font-medium text-muted-foreground text-right">{t.positionTable.unrealizedPnl}</th>
                  <th className="pb-2 font-medium text-muted-foreground text-right">{t.positionTable.realizedPnl}</th>
                </tr>
              </thead>
              <tbody>
                {positions.map((pos) => (
                  <tr key={pos.id} className="border-b last:border-0 hover:bg-muted/50 transition-colors">
                    <td className="py-3 font-medium">{pos.symbol}</td>
                    <td className="py-3">
                      <Badge variant={pos.side === 'Long' ? 'default' : 'destructive'} className="text-xs">
                        {pos.side === 'Long' ? t.positionTable.long : t.positionTable.short}
                      </Badge>
                    </td>
                    <td className="py-3 text-right font-mono">{parseFloat(pos.quantity).toFixed(4)}</td>
                    <td className="py-3 text-right font-mono">${formatPrice(pos.avg_entry_price)}</td>
                    <td className="py-3 text-right font-mono">${formatPrice(pos.current_price)}</td>
                    <td className="py-3 text-right">{formatPnl(pos.unrealized_pnl)}</td>
                    <td className="py-3 text-right">{formatPnl(pos.realized_pnl)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
