'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Card, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Loader2, TrendingUp, TrendingDown, RefreshCw } from 'lucide-react';
import { RealtimePrice } from '@/types/trading';
import { Button } from '@/components/ui/button';
import { useLanguage } from '@/lib/i18n/context';

interface PriceTickerProps {
  symbols?: string[];
  onSymbolSelect?: (symbol: string) => void;
  selectedSymbol?: string;
}

export default function PriceTicker({ symbols, onSymbolSelect, selectedSymbol }: PriceTickerProps) {
  const [prices, setPrices] = useState<RealtimePrice[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const { t } = useLanguage();

  const fetchPrices = async () => {
    try {
      setLoading(true);
      setError(null);
      const result = await invoke<RealtimePrice[]>('get_realtime_prices', {
        symbols: symbols || null
      });
      setPrices(result);
    } catch (err) {
      console.error('Failed to fetch prices:', err);
      setError(err instanceof Error ? err.message : t.common.error);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchPrices();
    const interval = setInterval(fetchPrices, 10000);
    return () => clearInterval(interval);
  }, [symbols?.join(',')]);

  const formatPrice = (price: string) => {
    const num = parseFloat(price);
    if (num >= 1000) return `$${num.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
    if (num >= 1) return `$${num.toFixed(2)}`;
    return `$${num.toFixed(4)}`;
  };

  const formatChange = (change?: string) => {
    if (!change) return null;
    const num = parseFloat(change);
    const isPositive = num >= 0;
    return (
      <span className={`flex items-center gap-1 text-xs font-medium ${isPositive ? 'text-emerald-500' : 'text-red-500'}`}>
        {isPositive ? <TrendingUp className="w-3 h-3" /> : <TrendingDown className="w-3 h-3" />}
        {isPositive ? '+' : ''}{num.toFixed(2)}%
      </span>
    );
  };

  if (loading && prices.length === 0) {
    return (
      <div className="flex items-center justify-center py-8">
        <Loader2 className="w-5 h-5 animate-spin mr-2" />
        <span className="text-sm text-muted-foreground">{t.common.loading}</span>
      </div>
    );
  }

  if (error) {
    return (
      <Card className="border-destructive/50 bg-destructive/5">
        <CardContent className="pt-6">
          <p className="text-sm text-destructive">{error}</p>
        </CardContent>
      </Card>
    );
  }

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium text-muted-foreground">{t.priceTicker.marketPrices}</h3>
        <Button variant="ghost" size="sm" onClick={fetchPrices} disabled={loading}>
          <RefreshCw className={`w-3 h-3 ${loading ? 'animate-spin' : ''}`} />
        </Button>
      </div>
      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        {prices.map((price) => {
          const isSelected = selectedSymbol === price.symbol;
          return (
            <Card
              key={price.symbol}
              className={`cursor-pointer transition-all hover:shadow-md ${
                isSelected
                  ? 'border-primary bg-primary/5 shadow-sm'
                  : 'hover:border-primary/50'
              }`}
              onClick={() => onSymbolSelect?.(price.symbol)}
            >
              <CardContent className="p-4">
                <div className="flex items-center justify-between mb-2">
                  <span className="text-sm font-semibold">{price.symbol}</span>
                  {isSelected && (
                    <Badge variant="default" className="text-[10px] px-1.5 py-0">
                      {t.common.selected}
                    </Badge>
                  )}
                </div>
                <div className="text-xl font-bold mb-1">
                  {formatPrice(price.price)}
                </div>
                {formatChange(price.change_24h)}
                {price.volume_24h && (
                  <p className="text-[10px] text-muted-foreground mt-1">
                    {t.priceTicker.vol}: {parseFloat(price.volume_24h).toLocaleString('en-US', { maximumFractionDigits: 0 })}
                  </p>
                )}
              </CardContent>
            </Card>
          );
        })}
      </div>
    </div>
  );
}
