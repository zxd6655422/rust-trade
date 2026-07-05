'use client';

import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import {
  Plus, Trash2, Loader2, RefreshCw, Coins, ToggleLeft, ToggleRight
} from 'lucide-react';

interface SymbolConfig {
  symbol: string;
  enabled: boolean;
}

interface Props {
  onSymbolsChange?: (symbols: string[]) => void;
}

const STORAGE_KEY = 'trading_symbols';

export default function SymbolManager({ onSymbolsChange }: Props) {
  const [symbols, setSymbols] = useState<SymbolConfig[]>([]);
  const [loading, setLoading] = useState(false);
  const [newSymbol, setNewSymbol] = useState('');
  const [error, setError] = useState<string | null>(null);

  const loadSymbols = useCallback(async () => {
    setLoading(true);
    try {
      const result = await invoke<SymbolConfig[]>('get_symbols');
      setSymbols(result);
      // 同步到 localStorage
      localStorage.setItem(STORAGE_KEY, JSON.stringify(result.map(s => s.symbol)));
      // 通知父组件
      onSymbolsChange?.(result.filter(s => s.enabled).map(s => s.symbol));
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [onSymbolsChange]);

  useEffect(() => { loadSymbols(); }, [loadSymbols]);

  const handleAdd = async () => {
    const sym = newSymbol.trim().toUpperCase();
    if (!sym) return;
    if (symbols.some(s => s.symbol === sym)) {
      setError(`${sym} 已存在`);
      return;
    }
    try {
      await invoke('add_symbol', { symbol: sym });
      setNewSymbol('');
      setError(null);
      await loadSymbols();
    } catch (e) {
      setError(String(e));
    }
  };

  const handleRemove = async (symbol: string) => {
    try {
      await invoke('remove_symbol', { symbol });
      await loadSymbols();
    } catch (e) {
      setError(String(e));
    }
  };

  const handleToggle = async (symbol: string, currentEnabled: boolean) => {
    try {
      await invoke('toggle_symbol', { symbol, enabled: !currentEnabled });
      await loadSymbols();
    } catch (e) {
      setError(String(e));
    }
  };

  const enabledCount = symbols.filter(s => s.enabled).length;

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <CardTitle className="text-base flex items-center gap-2">
            <Coins className="w-4 h-4" />
            交易对管理
            <Badge variant="secondary" className="text-xs">{enabledCount} 监控中</Badge>
          </CardTitle>
          <Button variant="ghost" size="sm" onClick={loadSymbols} disabled={loading} className="h-7 w-7 p-0">
            <RefreshCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
          </Button>
        </div>
      </CardHeader>
      <CardContent className="space-y-3">
        {/* 交易对列表 */}
        {loading && symbols.length === 0 ? (
          <div className="flex items-center justify-center py-6">
            <Loader2 className="w-5 h-5 animate-spin mr-2" />
            <span className="text-sm text-muted-foreground">加载中...</span>
          </div>
        ) : (
          <div className="space-y-1.5">
            {symbols.map((s) => (
              <div key={s.symbol} className="flex items-center justify-between py-1.5 px-2 rounded border bg-muted/30">
                <div className="flex items-center gap-2">
                  <span className={`font-medium text-sm ${s.enabled ? '' : 'text-muted-foreground line-through'}`}>
                    {s.symbol}
                  </span>
                  {s.enabled && (
                    <Badge variant="outline" className="text-[10px] px-1 py-0 border-green-500/30 text-green-500">
                      监控中
                    </Badge>
                  )}
                </div>
                <div className="flex items-center gap-1">
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-6 w-6 p-0"
                    onClick={() => handleToggle(s.symbol, s.enabled)}
                    title={s.enabled ? '禁用' : '启用'}
                  >
                    {s.enabled ? (
                      <ToggleRight className="w-4 h-4 text-green-500" />
                    ) : (
                      <ToggleLeft className="w-4 h-4 text-muted-foreground" />
                    )}
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-6 w-6 p-0 text-destructive hover:text-destructive"
                    onClick={() => handleRemove(s.symbol)}
                    title="删除"
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                  </Button>
                </div>
              </div>
            ))}
          </div>
        )}

        {/* 添加交易对 */}
        <div className="flex gap-2 pt-1">
          <Input
            placeholder="输入交易对，如 DOGEUSDT"
            value={newSymbol}
            onChange={(e) => { setNewSymbol(e.target.value); setError(null); }}
            onKeyDown={(e) => e.key === 'Enter' && handleAdd()}
            className="h-8 text-sm"
          />
          <Button size="sm" className="h-8 px-3" onClick={handleAdd} disabled={!newSymbol.trim()}>
            <Plus className="w-3.5 h-3.5 mr-1" />
            添加
          </Button>
        </div>

        {error && (
          <p className="text-xs text-destructive">{error}</p>
        )}
      </CardContent>
    </Card>
  );
}

/// 从 localStorage 或 DB 加载交易对列表（供其他组件使用）
export async function loadSymbols(): Promise<string[]> {
  try {
    const result = await invoke<SymbolConfig[]>('get_symbols');
    const enabled = result.filter(s => s.enabled).map(s => s.symbol);
    localStorage.setItem(STORAGE_KEY, JSON.stringify(enabled));
    return enabled;
  } catch {
    // 降级：从 localStorage 读取
    const cached = localStorage.getItem(STORAGE_KEY);
    if (cached) {
      try { return JSON.parse(cached); } catch {}
    }
    return ['BTCUSDT', 'ETHUSDT', 'SOLUSDT'];
  }
}
