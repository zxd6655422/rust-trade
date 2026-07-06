'use client';

import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Loader2, Coins, Database, Search } from 'lucide-react';

interface SymbolConfig {
  symbol: string;
  enabled: boolean;
}

interface SymbolSelectProps {
  value: string;
  onChange: (symbol: string) => void;
  /** 是否只显示启用的交易对，默认 true */
  enabledOnly?: boolean;
  /** 占位符文本 */
  placeholder?: string;
  /** 禁用状态 */
  disabled?: boolean;
  /** 自定义 className */
  className?: string;
}

/**
 * 通用交易对下拉选择组件
 * 分两部分：
 * 1. 数据库已有的交易对（来自 kline_1m 表）
 * 2. 新增交易对（手动输入）
 */
export default function SymbolSelect({
  value,
  onChange,
  enabledOnly = true,
  placeholder = '选择交易对',
  disabled = false,
  className,
}: SymbolSelectProps) {
  const [symbols, setSymbols] = useState<SymbolConfig[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const loadSymbols = async () => {
      try {
        const result = await invoke<SymbolConfig[]>('get_symbols');
        setSymbols(result);
      } catch (e) {
        console.error('Failed to load symbols:', e);
        // 降级：使用默认列表
        setSymbols([
          { symbol: 'BTCUSDT', enabled: true },
          { symbol: 'ETHUSDT', enabled: true },
          { symbol: 'SOLUSDT', enabled: true },
        ]);
      } finally {
        setLoading(false);
      }
    };
    loadSymbols();
  }, []);

  const displaySymbols = enabledOnly
    ? symbols.filter(s => s.enabled)
    : symbols;

  if (loading) {
    return (
      <div className={`flex items-center gap-2 ${className}`}>
        <Loader2 className="w-4 h-4 animate-spin" />
        <span className="text-sm text-muted-foreground">加载中...</span>
      </div>
    );
  }

  return (
    <Select value={value} onValueChange={onChange} disabled={disabled}>
      <SelectTrigger className={`w-[140px] ${className || ''}`}>
        <div className="flex items-center gap-1.5">
          <Coins className="w-3.5 h-3.5 text-muted-foreground" />
          <SelectValue placeholder={placeholder} />
        </div>
      </SelectTrigger>
      <SelectContent>
        {/* 已有交易对 */}
        {displaySymbols.length > 0 && (
          <>
            <div className="px-2 py-1.5 text-xs font-medium text-muted-foreground flex items-center gap-1.5">
              <Database className="w-3 h-3" />
              数据库已有
            </div>
            {displaySymbols.map((s) => (
              <SelectItem key={s.symbol} value={s.symbol}>
                {s.symbol}
              </SelectItem>
            ))}
          </>
        )}
      </SelectContent>
    </Select>
  );
}

/**
 * 获取启用的交易对列表（供其他组件使用）
 */
export async function fetchEnabledSymbols(): Promise<string[]> {
  try {
    const result = await invoke<SymbolConfig[]>('get_symbols');
    return result.filter(s => s.enabled).map(s => s.symbol);
  } catch {
    return ['BTCUSDT', 'ETHUSDT', 'SOLUSDT'];
  }
}

/**
 * 获取数据库中已有的交易对列表
 */
export async function fetchExistingSymbols(): Promise<string[]> {
  try {
    // 从 kline_1m 表获取所有有数据的交易对
    const result = await invoke<{ symbol: string; records_count: number }[]>('get_data_info');
    return result
      .sort((a, b) => b.records_count - a.records_count)
      .map(s => s.symbol);
  } catch {
    return ['BTCUSDT', 'ETHUSDT', 'SOLUSDT'];
  }
}
