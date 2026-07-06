'use client';

import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import {
  Database, Archive, RefreshCw, Loader2, CheckCircle,
  AlertCircle, Plus, Trash2, HardDrive, Clock
} from 'lucide-react';

interface CollectionStatus {
  symbol: string;
  collecting: boolean;
  record_count: number;
  earliest_time: string | null;
  latest_time: string | null;
}

interface ArchiveResult {
  symbol: string;
  archived_count: number;
  file_size_mb: number;
  success: boolean;
  error: string | null;
}

interface Props {
  onSymbolsChange?: (symbols: string[]) => void;
}

export default function DataManager({ onSymbolsChange }: Props) {
  const [statuses, setStatuses] = useState<CollectionStatus[]>([]);
  const [loading, setLoading] = useState(true);
  const [newSymbol, setNewSymbol] = useState('');
  const [backfillDays, setBackfillDays] = useState('7');
  const [archiving, setArchiving] = useState(false);
  const [archiveResults, setArchiveResults] = useState<ArchiveResult[]>([]);
  const [error, setError] = useState<string | null>(null);

  // 加载所有交易对状态
  const loadStatuses = async () => {
    setLoading(true);
    try {
      const result = await invoke<CollectionStatus[]>('get_all_collection_status');
      setStatuses(result);
      // 通知父组件
      onSymbolsChange?.(result.filter(s => s.collecting).map(s => s.symbol));
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadStatuses();
  }, []);

  // 添加交易对并开始采集
  const handleAddSymbol = async () => {
    const symbol = newSymbol.trim().toUpperCase();
    if (!symbol) return;

    if (statuses.some(s => s.symbol === symbol)) {
      setError(`${symbol} 已存在`);
      return;
    }

    try {
      setLoading(true);
      await invoke('add_symbol_with_collection', {
        symbol,
        backfillDays: parseInt(backfillDays) || 0,
      });
      setNewSymbol('');
      setError(null);
      await loadStatuses();
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  // 删除交易对
  const handleRemoveSymbol = async (symbol: string) => {
    try {
      await invoke('remove_symbol', { symbol });
      await loadStatuses();
    } catch (e) {
      setError(String(e));
    }
  };

  // 执行归档
  const handleArchive = async (symbol?: string) => {
    setArchiving(true);
    setArchiveResults([]);
    setError(null);

    try {
      const days = parseInt(backfillDays) || 7;
      let results: ArchiveResult[];

      if (symbol) {
        const result = await invoke<ArchiveResult>('archive_symbol_data', {
          symbol,
          daysToKeep: days,
        });
        results = [result];
      } else {
        results = await invoke<ArchiveResult[]>('archive_all_symbols', {
          daysToKeep: days,
        });
      }

      setArchiveResults(results);
      await loadStatuses();
    } catch (e) {
      setError(String(e));
    } finally {
      setArchiving(false);
    }
  };

  // 格式化数字
  const formatNumber = (num: number) => {
    if (num >= 1000000) return `${(num / 1000000).toFixed(1)}M`;
    if (num >= 1000) return `${(num / 1000).toFixed(1)}K`;
    return num.toString();
  };

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <CardTitle className="text-base flex items-center gap-2">
            <Database className="w-4 h-4" />
            数据管理
            <Badge variant="secondary" className="text-xs">
              {statuses.filter(s => s.collecting).length} 采集中
            </Badge>
          </CardTitle>
          <Button
            variant="ghost"
            size="sm"
            onClick={loadStatuses}
            disabled={loading}
            className="h-7 w-7 p-0"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
          </Button>
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        {/* 添加交易对 */}
        <div className="flex gap-2">
          <Input
            placeholder="输入交易对，如 DOGEUSDT"
            value={newSymbol}
            onChange={(e) => { setNewSymbol(e.target.value); setError(null); }}
            onKeyDown={(e) => e.key === 'Enter' && handleAddSymbol()}
            className="h-8 text-sm"
          />
          <Input
            type="number"
            placeholder="回填天数"
            value={backfillDays}
            onChange={(e) => setBackfillDays(e.target.value)}
            className="h-8 text-sm w-24"
            min="0"
          />
          <Button
            size="sm"
            className="h-8 px-3"
            onClick={handleAddSymbol}
            disabled={!newSymbol.trim() || loading}
          >
            <Plus className="w-3.5 h-3.5 mr-1" />
            添加
          </Button>
        </div>

        {error && (
          <p className="text-xs text-destructive">{error}</p>
        )}

        {/* 交易对列表 */}
        {loading && statuses.length === 0 ? (
          <div className="flex items-center justify-center py-6">
            <Loader2 className="w-5 h-5 animate-spin mr-2" />
            <span className="text-sm text-muted-foreground">加载中...</span>
          </div>
        ) : (
          <div className="space-y-2">
            {statuses.map((s) => (
              <div key={s.symbol} className="flex items-center justify-between py-2 px-3 rounded border bg-muted/30">
                <div className="flex items-center gap-3">
                  <div>
                    <div className="flex items-center gap-2">
                      <span className={`font-medium text-sm ${s.collecting ? '' : 'text-muted-foreground line-through'}`}>
                        {s.symbol}
                      </span>
                      {s.collecting && (
                        <Badge variant="outline" className="text-[10px] px-1 py-0 border-green-500/30 text-green-500">
                          采集中
                        </Badge>
                      )}
                    </div>
                    <div className="flex items-center gap-4 mt-1 text-[10px] text-muted-foreground">
                      <span className="flex items-center gap-1">
                        <HardDrive className="w-3 h-3" />
                        {formatNumber(s.record_count)} 条
                      </span>
                      {s.earliest_time && (
                        <span className="flex items-center gap-1">
                          <Clock className="w-3 h-3" />
                          {s.earliest_time.slice(0, 10)}
                        </span>
                      )}
                    </div>
                  </div>
                </div>
                <div className="flex items-center gap-1">
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-6 px-2 text-xs"
                    onClick={() => handleArchive(s.symbol)}
                    disabled={archiving}
                    title="归档"
                  >
                    <Archive className="w-3.5 h-3.5" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-6 w-6 p-0 text-destructive hover:text-destructive"
                    onClick={() => handleRemoveSymbol(s.symbol)}
                    title="删除"
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                  </Button>
                </div>
              </div>
            ))}
          </div>
        )}

        {/* 批量归档按钮 */}
        {statuses.length > 0 && (
          <div className="pt-2 border-t">
            <Button
              variant="outline"
              size="sm"
              className="w-full"
              onClick={() => handleArchive()}
              disabled={archiving}
            >
              {archiving ? (
                <>
                  <Loader2 className="w-3.5 h-3.5 mr-2 animate-spin" />
                  归档中...
                </>
              ) : (
                <>
                  <Archive className="w-3.5 h-3.5 mr-2" />
                  归档所有交易对 (保留 {backfillDays} 天)
                </>
              )}
            </Button>
          </div>
        )}

        {/* 归档结果 */}
        {archiveResults.length > 0 && (
          <div className="pt-2 border-t space-y-1">
            <p className="text-xs font-medium text-muted-foreground">归档结果:</p>
            {archiveResults.map((r) => (
              <div key={r.symbol} className="flex items-center gap-2 text-xs">
                {r.success ? (
                  <CheckCircle className="w-3 h-3 text-green-500" />
                ) : (
                  <AlertCircle className="w-3 h-3 text-red-500" />
                )}
                <span>{r.symbol}</span>
                <span className="text-muted-foreground">
                  {r.archived_count > 0
                    ? `归档 ${r.archived_count} 条 (${r.file_size_mb.toFixed(2)} MB)`
                    : '无数据'}
                </span>
                {r.error && (
                  <span className="text-red-500">{r.error}</span>
                )}
              </div>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
