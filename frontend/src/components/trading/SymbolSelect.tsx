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
import { Loader2, Coins, Database, Eye } from 'lucide-react';

interface TradingPair {
  symbol: string;
  market_type: string;
  status: string;
}

interface MonitorConfig {
  symbol: string;
  enabled: boolean;
}

interface SymbolSelectProps {
  value: string;
  onChange: (symbol: string) => void;
  /** 占位符文本 */
  placeholder?: string;
  /** 禁用状态 */
  disabled?: boolean;
  /** 自定义 className */
  className?: string;
}

/**
 * 通用交易对下拉选择组件
 * 显示所有交易对（来自 trading_pairs 表）
 * 监控中的交易对会标记绿点
 */
export default function SymbolSelect({
  value,
  onChange,
  placeholder = '选择交易对',
  disabled = false,
  className,
}: SymbolSelectProps) {
  const [pairs, setPairs] = useState<TradingPair[]>([]);
  const [monitorList, setMonitorList] = useState<MonitorConfig[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const loadData = async () => {
      try {
        const [pairsResult, monitors] = await Promise.all([
          invoke<TradingPair[]>('get_trading_pairs'),
          invoke<MonitorConfig[]>('get_symbols'),
        ]);
        setPairs(pairsResult);
        setMonitorList(monitors);
      } catch (e) {
        console.error('Failed to load symbols:', e);
        // 降级：使用默认列表
        setPairs([
          { symbol: 'BTCUSDT', market_type: 'spot', status: 'active' },
          { symbol: 'ETHUSDT', market_type: 'spot', status: 'active' },
          { symbol: 'SOLUSDT', market_type: 'spot', status: 'active' },
        ]);
      } finally {
        setLoading(false);
      }
    };
    loadData();
  }, []);

  const isMonitoring = (symbol: string) => {
    return monitorList.some(m => m.symbol === symbol && m.enabled);
  };

  // 按监控状态排序：监控中的在前
  const sortedPairs = [...pairs].sort((a, b) => {
    const aMon = isMonitoring(a.symbol) ? 0 : 1;
    const bMon = isMonitoring(b.symbol) ? 0 : 1;
    return aMon - bMon;
  });

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
        {/* 监控中的交易对 */}
        {sortedPairs.some(p => isMonitoring(p.symbol)) && (
          <>
            <div className="px-2 py-1.5 text-xs font-medium text-muted-foreground flex items-center gap-1.5">
              <Eye className="w-3 h-3" />
              监控中
            </div>
            {sortedPairs.filter(p => isMonitoring(p.symbol)).map((p) => (
              <SelectItem key={p.symbol} value={p.symbol}>
                <div className="flex items-center gap-2">
                  <span>{p.symbol}</span>
                  <span className="text-[10px] text-muted-foreground">
                    {p.market_type === 'futures' ? '合约' : '现货'}
                  </span>
                </div>
              </SelectItem>
            ))}
          </>
        )}
        {/* 其他交易对 */}
        {sortedPairs.some(p => !isMonitoring(p.symbol)) && (
          <>
            <div className="px-2 py-1.5 text-xs font-medium text-muted-foreground flex items-center gap-1.5">
              <Database className="w-3 h-3" />
              全部交易对
            </div>
            {sortedPairs.filter(p => !isMonitoring(p.symbol)).map((p) => (
              <SelectItem key={p.symbol} value={p.symbol}>
                <div className="flex items-center gap-2">
                  <span>{p.symbol}</span>
                  <span className="text-[10px] text-muted-foreground">
                    {p.market_type === 'futures' ? '合约' : '现货'}
                  </span>
                </div>
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
    const result = await invoke<MonitorConfig[]>('get_symbols');
    return result.filter(s => s.enabled).map(s => s.symbol);
  } catch {
    return ['BTCUSDT', 'ETHUSDT', 'SOLUSDT'];
  }
}

/**
 * 获取所有交易对列表
 */
export async function fetchAllSymbols(): Promise<string[]> {
  try {
    const result = await invoke<TradingPair[]>('get_trading_pairs');
    return result.map(s => s.symbol);
  } catch {
    return ['BTCUSDT', 'ETHUSDT', 'SOLUSDT'];
  }
}
