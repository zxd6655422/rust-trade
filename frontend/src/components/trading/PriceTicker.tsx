'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Card, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Loader2, TrendingUp, TrendingDown, RefreshCw, Wifi, WifiOff, Radio } from 'lucide-react';
import { RealtimePrice } from '@/types/trading';
import { Button } from '@/components/ui/button';
import { useLanguage } from '@/lib/i18n/context';
import { useRealtimeData, DataSource } from '@/lib/useRealtimeData';
import SymbolSelect from './SymbolSelect';

interface PriceTickerProps {
  symbols?: string[];
  onSymbolSelect?: (symbol: string) => void;
  selectedSymbol?: string;
  /** 显示交易对选择器，默认 true */
  showSymbolSelect?: boolean;
}

/** 数据源状态指示器 */
function DataSourceIndicator({ source }: { source: DataSource }) {
  const { t } = useLanguage();
  if (source === 'websocket') {
    return (
      <Badge variant="default" className="text-[10px] px-1.5 py-0 gap-1 bg-emerald-500">
        <Radio className="w-2.5 h-2.5" />
        {t.priceTicker.live}
      </Badge>
    );
  }
  if (source === 'polling') {
    return (
      <Badge variant="secondary" className="text-[10px] px-1.5 py-0 gap-1">
        <Wifi className="w-2.5 h-2.5" />
        {t.priceTicker.polling}
      </Badge>
    );
  }
  return (
    <Badge variant="destructive" className="text-[10px] px-1.5 py-0 gap-1">
      <WifiOff className="w-2.5 h-2.5" />
      {t.priceTicker.offline}
    </Badge>
  );
}

export default function PriceTicker({ symbols, onSymbolSelect, selectedSymbol, showSymbolSelect = true }: PriceTickerProps) {
  const { t } = useLanguage();
  // 如果未指定 symbols，从数据库加载
  const [defaultSymbols, setDefaultSymbols] = useState<string[]>(symbols || []);

  useEffect(() => {
    if (!symbols) {
      // 从数据库加载交易对列表
      invoke<{ symbol: string; enabled: boolean }[]>('get_symbols')
        .then(result => {
          const enabled = result.filter(s => s.enabled).map(s => s.symbol);
          setDefaultSymbols(enabled.length > 0 ? enabled : ['BTCUSDT', 'ETHUSDT', 'SOLUSDT']);
        })
        .catch(() => {
          setDefaultSymbols(['BTCUSDT', 'ETHUSDT', 'SOLUSDT']);
        });
    }
  }, [symbols]);

  // 使用实时数据 hook（自动选择 WebSocket 或轮询）
  const { prices: realtimePrices, dataSource, isConnected, reconnect } = useRealtimeData({
    symbols: defaultSymbols,
  });

  // 降级：如果实时数据为空，用 Tauri 命令加载初始数据
  const [fallbackPrices, setFallbackPrices] = useState<RealtimePrice[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const loadInitial = async () => {
      try {
        const result = await invoke<RealtimePrice[]>('get_realtime_prices', {
          symbols: symbols || null,
        });
        setFallbackPrices(result);
      } catch {
        // 静默失败
      } finally {
        setLoading(false);
      }
    };
    loadInitial();
  }, [symbols?.join(',')]);

  // 合并实时数据和降级数据
  const prices: RealtimePrice[] =
    realtimePrices.size > 0
      ? Array.from(realtimePrices.values())
      : fallbackPrices;

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

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <h3 className="text-sm font-medium text-muted-foreground">{t.priceTicker.marketPrices}</h3>
          <DataSourceIndicator source={dataSource} />
          {/* 交易对选择器 */}
          {showSymbolSelect && selectedSymbol && onSymbolSelect && (
            <SymbolSelect
              value={selectedSymbol}
              onChange={onSymbolSelect}
              className="ml-2"
            />
          )}
        </div>
        <div className="flex items-center gap-1">
          {!isConnected && dataSource !== 'polling' && (
            <Button variant="ghost" size="sm" onClick={reconnect} title="Reconnect">
              <RefreshCw className="w-3 h-3" />
            </Button>
          )}
        </div>
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
                <div className="flex items-center gap-2 mt-1 text-[10px] text-muted-foreground">
                  {price.high_24h && (
                    <span>H: {formatPrice(price.high_24h)}</span>
                  )}
                  {price.low_24h && (
                    <span>L: {formatPrice(price.low_24h)}</span>
                  )}
                </div>
                {price.volume_24h && (
                  <p className="text-[10px] text-muted-foreground mt-0.5">
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
