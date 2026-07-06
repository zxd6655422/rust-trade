'use client';

import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import {
  Database, Archive, RefreshCw, Loader2, CheckCircle,
  AlertCircle, Plus, Trash2, HardDrive, Clock,
  Play, Pause, Settings2, Eye, EyeOff
} from 'lucide-react';

interface TradingPairConfig {
  id: number;
  symbol: string;
  market_type: string;
  exchange: string;
  status: string;
  note: string | null;
  created_at: string;
  updated_at: string;
}

interface CollectionStatus {
  symbol: string;
  status: string;
  market_type: string;
  record_count: number;
  earliest_time: string | null;
  latest_time: string | null;
}

interface MonitorConfig {
  symbol: string;
  enabled: boolean;
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
  // 交易对配置
  const [tradingPairs, setTradingPairs] = useState<TradingPairConfig[]>([]);
  // 监控列表
  const [monitorList, setMonitorList] = useState<MonitorConfig[]>([]);
  // 数据库中有数据的交易对
  const [availableSymbols, setAvailableSymbols] = useState<string[]>([]);
  // 采集状态
  const [statuses, setStatuses] = useState<CollectionStatus[]>([]);
  // 新增交易对
  const [newSymbol, setNewSymbol] = useState('');
  const [newMarketType, setNewMarketType] = useState<'spot' | 'futures'>('futures');
  const [newExchange, setNewExchange] = useState('binance');
  // 归档
  const [archiving, setArchiving] = useState(false);
  const [archiveResults, setArchiveResults] = useState<ArchiveResult[]>([]);
  // 通用
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<'pairs' | 'archive'>('pairs');

  // 加载所有数据
  const loadAllData = async () => {
    setLoading(true);
    try {
      const [pairs, symbols, stats, monitors] = await Promise.all([
        invoke<TradingPairConfig[]>('get_trading_pairs'),
        invoke<string[]>('get_available_symbols_from_data'),
        invoke<CollectionStatus[]>('get_all_collection_status'),
        invoke<MonitorConfig[]>('get_symbols'),
      ]);
      setTradingPairs(pairs);
      setAvailableSymbols(symbols);
      setStatuses(stats);
      setMonitorList(monitors);
      // 通知父组件：只返回启用的交易对
      onSymbolsChange?.(monitors.filter(m => m.enabled).map(m => m.symbol));
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadAllData();
  }, []);

  // 添加新交易对
  const handleAddPair = async () => {
    const symbol = newSymbol.trim().toUpperCase();
    if (!symbol) return;

    try {
      setLoading(true);
      await invoke('add_trading_pair', {
        symbol,
        marketType: newMarketType,
        exchange: newExchange,
        note: null,
      });
      setNewSymbol('');
      setError(null);
      await loadAllData();
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  // 暂停交易对
  const handlePause = async (symbol: string) => {
    try {
      await invoke('update_trading_pair_status', { symbol, status: 'paused' });
      await invoke('remove_from_monitor', { symbol });
      await loadAllData();
    } catch (e) {
      setError(String(e));
    }
  };

  // 恢复交易对
  const handleResume = async (symbol: string) => {
    try {
      await invoke('update_trading_pair_status', { symbol, status: 'active' });
      await invoke('add_to_monitor', { symbol });
      await loadAllData();
    } catch (e) {
      setError(String(e));
    }
  };

  // 删除交易对
  const handleDelete = async (symbol: string) => {
    if (!confirm(`确定要删除 ${symbol} 吗？`)) return;
    try {
      await invoke('delete_trading_pair', { symbol });
      await invoke('remove_from_monitor', { symbol });
      await loadAllData();
    } catch (e) {
      setError(String(e));
    }
  };

  // 加入监控
  const handleAddToMonitor = async (symbol: string) => {
    try {
      await invoke('add_to_monitor', { symbol });
      await loadAllData();
    } catch (e) {
      setError(String(e));
    }
  };

  // 从监控移除
  const handleRemoveFromMonitor = async (symbol: string) => {
    try {
      await invoke('remove_from_monitor', { symbol });
      await loadAllData();
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
      let results: ArchiveResult[];
      if (symbol) {
        const result = await invoke<ArchiveResult>('archive_symbol_data', {
          symbol,
          daysToKeep: 7,
        });
        results = [result];
      } else {
        results = await invoke<ArchiveResult[]>('archive_all_symbols', {
          daysToKeep: 7,
        });
      }
      setArchiveResults(results);
      await loadAllData();
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

  // 获取状态颜色
  const getStatusColor = (status: string) => {
    switch (status) {
      case 'active': return 'border-green-500/30 text-green-500';
      case 'paused': return 'border-yellow-500/30 text-yellow-500';
      case 'archived': return 'border-gray-500/30 text-gray-500';
      default: return '';
    }
  };

  // 获取状态标签
  const getStatusLabel = (status: string) => {
    switch (status) {
      case 'active': return '启用';
      case 'paused': return '暂停';
      case 'archived': return '归档';
      default: return status;
    }
  };

  // 检查是否在监控列表中
  const isInMonitor = (symbol: string) => {
    return monitorList.some(m => m.symbol === symbol && m.enabled);
  };

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <CardTitle className="text-base flex items-center gap-2">
            <Database className="w-4 h-4" />
            交易对管理
            <Badge variant="secondary" className="text-xs">
              {monitorList.filter(m => m.enabled).length} 监控中
            </Badge>
          </CardTitle>
          <Button
            variant="ghost"
            size="sm"
            onClick={loadAllData}
            disabled={loading}
            className="h-7 w-7 p-0"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
          </Button>
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        {/* Tab 切换 */}
        <div className="flex gap-1 p-1 bg-muted rounded-lg">
          <button
            onClick={() => setActiveTab('pairs')}
            className={`flex-1 px-3 py-1.5 rounded-md text-xs font-medium transition-all ${
              activeTab === 'pairs'
                ? 'bg-background text-foreground shadow-sm'
                : 'text-muted-foreground hover:text-foreground'
            }`}
          >
            交易对
          </button>
          <button
            onClick={() => setActiveTab('archive')}
            className={`flex-1 px-3 py-1.5 rounded-md text-xs font-medium transition-all ${
              activeTab === 'archive'
                ? 'bg-background text-foreground shadow-sm'
                : 'text-muted-foreground hover:text-foreground'
            }`}
          >
            数据归档
          </button>
        </div>

        {error && (
          <p className="text-xs text-destructive">{error}</p>
        )}

        {/* 交易对 Tab */}
        {activeTab === 'pairs' && (
          <div className="space-y-4">
            {/* 新增交易对 */}
            <div className="pb-3 border-b">
              <label className="text-xs font-medium text-muted-foreground mb-2 block">
                新增交易对
              </label>
              <div className="flex gap-2">
                <Input
                  placeholder="输入交易对，如 DOGEUSDT"
                  value={newSymbol}
                  onChange={(e) => { setNewSymbol(e.target.value); setError(null); }}
                  onKeyDown={(e) => e.key === 'Enter' && handleAddPair()}
                  className="h-8 text-sm"
                />
                <select
                  value={newMarketType}
                  onChange={(e) => setNewMarketType(e.target.value as 'spot' | 'futures')}
                  className="h-8 px-2 text-sm border rounded-md bg-background"
                >
                  <option value="futures">合约</option>
                  <option value="spot">现货</option>
                </select>
                <select
                  value={newExchange}
                  onChange={(e) => setNewExchange(e.target.value)}
                  className="h-8 px-2 text-sm border rounded-md bg-background"
                >
                  <option value="binance">Binance</option>
                  <option value="okx">OKX</option>
                </select>
                <Button
                  size="sm"
                  className="h-8 px-3"
                  onClick={handleAddPair}
                  disabled={!newSymbol.trim() || loading}
                >
                  <Plus className="w-3.5 h-3.5 mr-1" />
                  添加
                </Button>
              </div>
              {/* 从数据库选择已有交易对 */}
              {availableSymbols.length > 0 && (
                <div className="mt-2">
                  <div className="flex flex-wrap gap-1.5">
                    {availableSymbols.slice(0, 10).map((symbol) => {
                      const exists = tradingPairs.some(p => p.symbol === symbol);
                      return (
                        <button
                          key={symbol}
                          onClick={async () => {
                            if (!exists) {
                              await invoke('add_trading_pair', {
                                symbol,
                                marketType: 'futures',
                                exchange: 'binance',
                                note: null,
                              });
                              await loadAllData();
                            }
                          }}
                          disabled={exists}
                          className={`px-2 py-1 rounded text-[10px] font-medium transition-all ${
                            exists
                              ? 'bg-muted text-muted-foreground cursor-not-allowed'
                              : 'bg-primary/10 text-primary hover:bg-primary/20 cursor-pointer'
                          }`}
                        >
                          {symbol}
                        </button>
                      );
                    })}
                  </div>
                </div>
              )}
            </div>

            {/* 交易对列表 */}
            <div className="space-y-2">
              {loading ? (
                <div className="flex items-center justify-center py-6">
                  <Loader2 className="w-5 h-5 animate-spin mr-2" />
                  <span className="text-sm text-muted-foreground">加载中...</span>
                </div>
              ) : tradingPairs.length === 0 ? (
                <div className="text-center py-6 text-muted-foreground text-sm">
                  暂无交易对，请先添加
                </div>
              ) : (
                tradingPairs.map((pair) => {
                  const status = statuses.find(s => s.symbol === pair.symbol);
                  const monitoring = isInMonitor(pair.symbol);
                  return (
                    <div key={pair.symbol} className="flex items-center justify-between py-2 px-3 rounded border bg-muted/30">
                      <div className="flex items-center gap-3">
                        <div>
                          <div className="flex items-center gap-2">
                            <span className="font-medium text-sm">{pair.symbol}</span>
                            <Badge variant="outline" className={`text-[10px] px-1 py-0 ${getStatusColor(pair.status)}`}>
                              {getStatusLabel(pair.status)}
                            </Badge>
                            <Badge variant="secondary" className="text-[10px] px-1 py-0">
                              {pair.market_type === 'futures' ? '合约' : '现货'}
                            </Badge>
                            {monitoring && (
                              <Badge variant="default" className="text-[10px] px-1 py-0 bg-green-500">
                                监控中
                              </Badge>
                            )}
                          </div>
                          <div className="flex items-center gap-4 mt-1 text-[10px] text-muted-foreground">
                            {status && (
                              <>
                                <span className="flex items-center gap-1">
                                  <HardDrive className="w-3 h-3" />
                                  {formatNumber(status.record_count)} 条
                                </span>
                                {status.earliest_time && (
                                  <span className="flex items-center gap-1">
                                    <Clock className="w-3 h-3" />
                                    {status.earliest_time.slice(0, 10)}
                                  </span>
                                )}
                              </>
                            )}
                          </div>
                        </div>
                      </div>
                      <div className="flex items-center gap-1">
                        {/* 加入/移除监控 */}
                        {monitoring ? (
                          <Button
                            variant="ghost"
                            size="sm"
                            className="h-6 px-2 text-xs"
                            onClick={() => handleRemoveFromMonitor(pair.symbol)}
                            title="移除监控"
                          >
                            <EyeOff className="w-3.5 h-3.5" />
                          </Button>
                        ) : (
                          <Button
                            variant="ghost"
                            size="sm"
                            className="h-6 px-2 text-xs"
                            onClick={() => handleAddToMonitor(pair.symbol)}
                            title="加入监控"
                          >
                            <Eye className="w-3.5 h-3.5" />
                          </Button>
                        )}
                        {/* 暂停/恢复 */}
                        {pair.status === 'active' ? (
                          <Button
                            variant="ghost"
                            size="sm"
                            className="h-6 px-2 text-xs"
                            onClick={() => handlePause(pair.symbol)}
                            title="暂停采集"
                          >
                            <Pause className="w-3.5 h-3.5" />
                          </Button>
                        ) : (
                          <Button
                            variant="ghost"
                            size="sm"
                            className="h-6 px-2 text-xs"
                            onClick={() => handleResume(pair.symbol)}
                            title="恢复采集"
                          >
                            <Play className="w-3.5 h-3.5" />
                          </Button>
                        )}
                        {/* 归档 */}
                        <Button
                          variant="ghost"
                          size="sm"
                          className="h-6 px-2 text-xs"
                          onClick={() => handleArchive(pair.symbol)}
                          disabled={archiving}
                          title="归档"
                        >
                          <Archive className="w-3.5 h-3.5" />
                        </Button>
                        {/* 删除 */}
                        <Button
                          variant="ghost"
                          size="sm"
                          className="h-6 w-6 p-0 text-destructive hover:text-destructive"
                          onClick={() => handleDelete(pair.symbol)}
                          title="删除"
                        >
                          <Trash2 className="w-3.5 h-3.5" />
                        </Button>
                      </div>
                    </div>
                  );
                })
              )}
            </div>
          </div>
        )}

        {/* 数据归档 Tab */}
        {activeTab === 'archive' && (
          <div className="space-y-4">
            <p className="text-xs text-muted-foreground">
              将 PostgreSQL 中的历史数据导出到 Parquet 文件，释放数据库空间。
            </p>
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
                  归档所有交易对（保留 7 天）
                </>
              )}
            </Button>

            {/* 归档结果 */}
            {archiveResults.length > 0 && (
              <div className="space-y-1">
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
          </div>
        )}
      </CardContent>
    </Card>
  );
}
