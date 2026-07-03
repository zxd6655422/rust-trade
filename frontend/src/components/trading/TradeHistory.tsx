'use client';

import { useEffect, useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Loader2, History, RefreshCw, ChevronLeft, ChevronRight } from 'lucide-react';
import { TradeRecord } from '@/types/trading';
import { useLanguage } from '@/lib/i18n/context';

interface TradeHistoryProps {
  symbol?: string;
}

const PAGE_SIZE = 20;

export default function TradeHistory({ symbol }: TradeHistoryProps) {
  const [trades, setTrades] = useState<TradeRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [page, setPage] = useState(0);
  const [hasMore, setHasMore] = useState(true);
  const { t } = useLanguage();

  const fetchTrades = useCallback(async (pageNum: number) => {
    try {
      setLoading(true);
      const result = await invoke<TradeRecord[]>('get_trade_history', {
        request: {
          symbol: symbol || null,
          limit: PAGE_SIZE,
          offset: pageNum * PAGE_SIZE
        }
      });
      setTrades(result);
      setHasMore(result.length === PAGE_SIZE);
    } catch (err) {
      console.error('Failed to fetch trades:', err);
      setTrades([]);
    } finally {
      setLoading(false);
    }
  }, [symbol]);

  useEffect(() => {
    setPage(0);
    fetchTrades(0);
  }, [fetchTrades]);

  const handlePageChange = (newPage: number) => {
    setPage(newPage);
    fetchTrades(newPage);
  };

  const formatPrice = (price: string) => {
    const num = parseFloat(price);
    return num.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
  };

  const formatTime = (timeStr: string) => {
    const date = new Date(timeStr);
    return date.toLocaleString();
  };

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <History className="w-5 h-5 text-muted-foreground" />
            <CardTitle className="text-lg">{t.tradeHistory.title}</CardTitle>
          </div>
          <Button variant="ghost" size="sm" onClick={() => fetchTrades(page)} disabled={loading}>
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
        ) : trades.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-8 text-muted-foreground">
            <History className="w-8 h-8 mb-2 opacity-50" />
            <p className="text-sm">{t.tradeHistory.noTrades}</p>
            <p className="text-xs mt-1">{t.tradeHistory.noTradesDesc}</p>
          </div>
        ) : (
          <>
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b text-left">
                    <th className="pb-2 font-medium text-muted-foreground">{t.tradeHistory.time}</th>
                    <th className="pb-2 font-medium text-muted-foreground">{t.tradeHistory.symbol}</th>
                    <th className="pb-2 font-medium text-muted-foreground">{t.tradeHistory.side}</th>
                    <th className="pb-2 font-medium text-muted-foreground text-right">{t.tradeHistory.price}</th>
                    <th className="pb-2 font-medium text-muted-foreground text-right">{t.tradeHistory.quantity}</th>
                    <th className="pb-2 font-medium text-muted-foreground text-right">{t.tradeHistory.commission}</th>
                    <th className="pb-2 font-medium text-muted-foreground text-right">{t.tradeHistory.pnl}</th>
                  </tr>
                </thead>
                <tbody>
                  {trades.map((trade) => (
                    <tr key={trade.id} className="border-b last:border-0 hover:bg-muted/50 transition-colors">
                      <td className="py-2.5 text-xs text-muted-foreground font-mono">
                        {formatTime(trade.trade_time)}
                      </td>
                      <td className="py-2.5 font-medium">{trade.symbol}</td>
                      <td className="py-2.5">
                        <Badge
                          variant={trade.side === 'Buy' ? 'default' : 'destructive'}
                          className="text-xs"
                        >
                          {trade.side === 'Buy' ? t.tradeHistory.buy : t.tradeHistory.sell}
                        </Badge>
                      </td>
                      <td className="py-2.5 text-right font-mono">${formatPrice(trade.price)}</td>
                      <td className="py-2.5 text-right font-mono">{parseFloat(trade.quantity).toFixed(4)}</td>
                      <td className="py-2.5 text-right font-mono text-muted-foreground">
                        ${parseFloat(trade.commission).toFixed(4)}
                      </td>
                      <td className="py-2.5 text-right">
                        {trade.realized_pnl ? (
                          <span className={`font-mono font-medium ${
                            parseFloat(trade.realized_pnl) >= 0 ? 'text-emerald-500' : 'text-red-500'
                          }`}>
                            {parseFloat(trade.realized_pnl) >= 0 ? '+' : ''}
                            ${formatPrice(trade.realized_pnl)}
                          </span>
                        ) : (
                          <span className="text-muted-foreground">-</span>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>

            {/* Pagination */}
            <div className="flex items-center justify-between mt-4 pt-3 border-t">
              <p className="text-xs text-muted-foreground">
                {t.common.page} {page + 1} • {t.common.showing} {trades.length} {t.common.trades}
              </p>
              <div className="flex gap-1">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => handlePageChange(page - 1)}
                  disabled={page === 0 || loading}
                >
                  <ChevronLeft className="w-3 h-3" />
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => handlePageChange(page + 1)}
                  disabled={!hasMore || loading}
                >
                  <ChevronRight className="w-3 h-3" />
                </Button>
              </div>
            </div>
          </>
        )}
      </CardContent>
    </Card>
  );
}
