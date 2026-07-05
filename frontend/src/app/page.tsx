'use client';

import { useEffect, useState, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, LineChart, Line } from 'recharts';
import { Loader2, Database, TrendingUp, Activity, Zap, Clock, BarChart3, Play, Eye, Coins, Layers, Timer, Sparkles, Pause } from 'lucide-react';
import Link from 'next/link';
import { useLanguage } from '@/lib/i18n/context';

interface DataInfoResponse {
  total_records: number;
  symbols_count: number;
  earliest_time?: string;
  latest_time?: string;
  symbol_info: Array<{
    symbol: string;
    records_count: number;
    earliest_time?: string;
    latest_time?: string;
    min_price?: string;
    max_price?: string;
    total_volume_usd: string;
  }>;
}

interface StrategyCapability {
  id: string;
  name: string;
  description: string;
  supports_ohlc: boolean;
  preferred_timeframe?: string;
}

interface OHLCPreview {
  timestamp: string;
  symbol: string;
  open: string;
  high: string;
  low: string;
  close: string;
  volume: string;
  trade_count: number;
}

interface QuickBacktestResult {
  strategy: string;
  symbol: string;
  return_pct: number;
  final_value: number;
  trades: number;
  processing_time: number;
  data_source: string; // "tick" or "OHLC-1m" etc.
}

export default function Home() {
  const { t } = useLanguage();
  const [loading, setLoading] = useState(true);
  const [dataInfo, setDataInfo] = useState<DataInfoResponse | null>(null);
  const [strategyCapabilities, setStrategyCapabilities] = useState<StrategyCapability[]>([]);
  const [quickResults, setQuickResults] = useState<QuickBacktestResult[]>([]);
  const [ohlcPreview, setOhlcPreview] = useState<OHLCPreview[]>([]);
  const [isRunningQuick, setIsRunningQuick] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loadingOhlcPreview, setLoadingOhlcPreview] = useState(false);
  const [selectedTimeframe, setSelectedTimeframe] = useState('1h');
  const [selectedSymbol, setSelectedSymbol] = useState('');
  const [autoRefreshOhlc, setAutoRefreshOhlc] = useState(true);
  const [lastFetchTime, setLastFetchTime] = useState<Date | null>(null);
  const refreshTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    initializeDashboard();
  }, []);

  useEffect(() => {
    if (selectedSymbol && selectedTimeframe) {
      loadOhlcPreview();
    }
  }, [selectedSymbol, selectedTimeframe]);

  const initializeDashboard = async () => {
    try {
      setLoading(true);
      setError(null);
      
      const [dataInfoResult, capabilitiesResult] = await Promise.all([
        invoke<DataInfoResponse>('get_data_info'),
        invoke<StrategyCapability[]>('get_strategy_capabilities')
      ]);

      setDataInfo(dataInfoResult);
      setStrategyCapabilities(capabilitiesResult);

      // Set default symbol for OHLC preview (by volume)
      if (dataInfoResult.symbol_info.length > 0) {
        const topSymbol = dataInfoResult.symbol_info
          .sort((a, b) => parseFloat(b.total_volume_usd) - parseFloat(a.total_volume_usd))[0].symbol;
        setSelectedSymbol(topSymbol);
      }

    } catch (error) {
      console.error('Failed to initialize dashboard:', error);
      setError(error instanceof Error ? error.message : 'Failed to load dashboard data');
    } finally {
      setLoading(false);
    }
  };

  const loadOhlcPreview = async () => {
    if (!selectedSymbol || !selectedTimeframe) return;
    
    try {
      setLoadingOhlcPreview(true);
      
      // Calculate count based on timeframe
      const getCountByTimeframe = (tf: string) => {
        switch (tf) {
          case '1m': return 120; // Last 2 hours in 1-minute candles
          case '5m': return 100; // Last ~8 hours in 5-minute candles
          case '15m': return 96; // Last 24 hours in 15-minute candles
          case '30m': return 72; // Last 36 hours in 30-minute candles
          case '1h': return 72;  // Last 3 days in 1-hour candles
          case '4h': return 60;  // Last 10 days in 4-hour candles
          case '1d': return 60;  // Last 2 months in daily candles
          case '1w': return 26;  // Last 6 months in weekly candles
          default: return 72;
        }
      };
      
      const ohlcData = await invoke<OHLCPreview[]>('get_ohlc_preview', {
        request: {
          symbol: selectedSymbol,
          timeframe: selectedTimeframe,
          count: getCountByTimeframe(selectedTimeframe)
        }
      });
      
      setOhlcPreview(ohlcData);
      setLastFetchTime(new Date());
    } catch (error) {
      console.error('Failed to load OHLC preview:', error);
      setOhlcPreview([]);
    } finally {
      setLoadingOhlcPreview(false);
    }
  };

  // Auto-refresh OHLC data every 30 seconds
  useEffect(() => {
    if (autoRefreshOhlc && selectedSymbol && selectedTimeframe) {
      refreshTimerRef.current = setInterval(() => {
        loadOhlcPreview();
      }, 30000);
      return () => {
        if (refreshTimerRef.current) {
          clearInterval(refreshTimerRef.current);
          refreshTimerRef.current = null;
        }
      };
    } else {
      if (refreshTimerRef.current) {
        clearInterval(refreshTimerRef.current);
        refreshTimerRef.current = null;
      }
    }
  }, [autoRefreshOhlc, selectedSymbol, selectedTimeframe]);

  const runQuickBacktests = async () => {
    if (!dataInfo || !strategyCapabilities.length) return;
    
    setIsRunningQuick(true);
    setQuickResults([]);
    
    const topSymbols = dataInfo.symbol_info
      .sort((a, b) => parseFloat(b.total_volume_usd) - parseFloat(a.total_volume_usd))
      .slice(0, 3);
    
    const results: QuickBacktestResult[] = [];
    
    for (const symbolInfo of topSymbols) {
      for (const strategy of strategyCapabilities.slice(0, 2)) { 
        try {
          const startTime = Date.now();
          
          const response = await invoke<{
            return_percentage: string;
            final_value: string;
            total_trades: number;
            data_source?: string;
          }>('run_backtest', {
            request: {
              strategy_id: strategy.id,
              symbol: symbolInfo.symbol,
              data_count: Math.min(5000, symbolInfo.records_count),
              initial_capital: "10000",
              commission_rate: "0.001",
              strategy_params: {}
            }
          });

          const processingTime = Date.now() - startTime;

          results.push({
            strategy: strategy.name,
            symbol: symbolInfo.symbol,
            return_pct: parseFloat(response.return_percentage),
            final_value: parseFloat(response.final_value),
            trades: response.total_trades,
            processing_time: processingTime,
            data_source: response.data_source || 'tick'
          });
          
          setQuickResults([...results]);
          
        } catch (error) {
          console.error(`Quick backtest failed for ${strategy.id} on ${symbolInfo.symbol}:`, error);
        }
      }
    }
    
    setIsRunningQuick(false);
  };

  const getDataCoverageDays = () => {
    if (!dataInfo?.earliest_time || !dataInfo?.latest_time) return 0;
    const start = new Date(dataInfo.earliest_time);
    const end = new Date(dataInfo.latest_time);
    return Math.floor((end.getTime() - start.getTime()) / (1000 * 60 * 60 * 24));
  };

  const getOhlcSupportCount = () => {
    return strategyCapabilities.filter(s => s.supports_ohlc).length;
  };

  const getAvgProcessingTime = () => {
    if (quickResults.length === 0) return 0;
    return quickResults.reduce((sum, r) => sum + r.processing_time, 0) / quickResults.length;
  };

  const getOhlcChartData = () => {
    if (ohlcPreview.length === 0) return [];

    // 后端已返回时间正序（最旧在前，最新在后）
    // 取最后 maxCandles 根（最新的）
    const maxCandles = 40;
    const startIndex = Math.max(0, ohlcPreview.length - maxCandles);

    return ohlcPreview.slice(startIndex).map(candle => {
      const timestamp = new Date(candle.timestamp);

      // Format time display for x-axis
      const formatTime = (timeframe: string, date: Date) => {
        switch (timeframe) {
          case '1m':
          case '5m':
          case '15m':
          case '30m':
            return date.toLocaleTimeString([], {hour: '2-digit', minute:'2-digit'});
          case '1h':
          case '4h':
            return date.toLocaleDateString([], {month: 'short', day: 'numeric'}) + ' ' +
                   date.toLocaleTimeString([], {hour: '2-digit'});
          case '1d':
            return date.toLocaleDateString([], {month: 'short', day: 'numeric'});
          case '1w':
            return date.toLocaleDateString([], {month: 'short', day: 'numeric'});
          default:
            return date.toLocaleTimeString([], {hour: '2-digit', minute:'2-digit'});
        }
      };

      // Full datetime for tooltip display
      const fullDateTime = timestamp.toLocaleString([], {
        year: 'numeric', month: '2-digit', day: '2-digit',
        hour: '2-digit', minute: '2-digit'
      });

      return {
        time: formatTime(selectedTimeframe, timestamp),
        fullTime: fullDateTime,
        price: parseFloat(candle.close),
        volume: parseFloat(candle.volume),
        trades: candle.trade_count,
        high: parseFloat(candle.high),
        low: parseFloat(candle.low),
        open: parseFloat(candle.open),
      };
    });
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center h-screen">
        <Loader2 className="w-8 h-8 animate-spin mr-2" />
        <span>{t.dashboard.loadDashboard}</span>
      </div>
    );
  }

  const ohlcChartData = getOhlcChartData();
  const ohlcSupportCount = getOhlcSupportCount();

  return (
    <div className="space-y-6">
      {/* Welcome Header with OHLC Badge */}
      <div className="flex justify-between items-center">
        <div>
          <div className="flex items-center gap-3 mb-2">
            <h1 className="text-3xl font-bold">{t.dashboard.title}</h1>
            <Badge variant="secondary" className="flex items-center gap-1">
              <Layers className="w-3 h-3" />
              {t.dashboard.ohlcEnhanced}
            </Badge>
          </div>
          <p className="text-gray-600 dark:text-gray-400">
            {t.dashboard.subtitle}
          </p>
        </div>
        <div className="flex gap-2">
          <Button
            onClick={runQuickBacktests}
            disabled={isRunningQuick || !dataInfo}
            variant="outline"
            className="flex items-center gap-2"
          >
            {isRunningQuick ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : (
              <Zap className="w-4 h-4" />
            )}
            {t.dashboard.quickTest}
          </Button>
          <Link href="/trading">
            <Button className="flex items-center gap-2">
              <Play className="w-4 h-4" />
              {t.dashboard.fullBacktest}
            </Button>
          </Link>
        </div>
      </div>

      {/* Enhanced System Overview Cards */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-5 gap-4">
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">{t.dashboard.totalRecords}</CardTitle>
            <Database className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {dataInfo?.total_records.toLocaleString() || '0'}
            </div>
            <p className="text-xs text-muted-foreground">
              {t.dashboard.totalRecordsDesc}
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">{t.dashboard.tradingPairs}</CardTitle>
            <Coins className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-blue-600">
              {dataInfo?.symbols_count || 0}
            </div>
            <p className="text-xs text-muted-foreground">
              {t.dashboard.tradingPairsDesc}
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">{t.dashboard.dataCoverage}</CardTitle>
            <Clock className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-green-600">
              {getDataCoverageDays()}
            </div>
            <p className="text-xs text-muted-foreground">
              {t.dashboard.dataCoverageDesc}
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">{t.dashboard.totalStrategies}</CardTitle>
            <TrendingUp className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-purple-600">
              {strategyCapabilities.length}
            </div>
            <p className="text-xs text-muted-foreground">
              {t.dashboard.totalStrategiesDesc}
            </p>
          </CardContent>
        </Card>

        <Card className="border-2 border-blue-200 bg-blue-50 dark:border-blue-800 dark:bg-blue-900/20">
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">{t.dashboard.ohlcSupport}</CardTitle>
            <Sparkles className="h-4 w-4 text-blue-600" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-blue-600">
              {ohlcSupportCount}/{strategyCapabilities.length}
            </div>
            <p className="text-xs text-blue-600">
              {t.dashboard.ohlcSupportDesc}
            </p>
          </CardContent>
        </Card>
      </div>

      {/* OHLC Preview Chart */}
      <Card className="border-blue-200 bg-gradient-to-br from-blue-50 to-indigo-50 dark:from-blue-900/20 dark:to-indigo-900/20">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <BarChart3 className="w-5 h-5 text-blue-600" />
            {t.dashboard.liveOhlcPreview}
            <Badge variant="outline" className="ml-2">
              {selectedSymbol} • {selectedTimeframe.toUpperCase()}
            </Badge>
          </CardTitle>
          
          {/* OHLC Controls */}
          <div className="flex flex-wrap gap-3 mt-3">
            <div className="flex items-center gap-2">
              <label className="text-sm font-medium">{t.dashboard.selectSymbol}:</label>
              <select
                value={selectedSymbol}
                onChange={(e) => setSelectedSymbol(e.target.value)}
                className="text-sm px-2 py-1 border rounded dark:bg-gray-800 dark:border-gray-600"
              >
                {dataInfo?.symbol_info
                  .sort((a, b) => parseFloat(b.total_volume_usd) - parseFloat(a.total_volume_usd))
                  .map((symbol) => (
                  <option key={symbol.symbol} value={symbol.symbol}>
                    {symbol.symbol}
                  </option>
                ))}
              </select>
            </div>

            <div className="flex items-center gap-2">
              <label className="text-sm font-medium">{t.dashboard.selectTimeframe}:</label>
              <select
                value={selectedTimeframe}
                onChange={(e) => setSelectedTimeframe(e.target.value)}
                className="text-sm px-2 py-1 border rounded dark:bg-gray-800 dark:border-gray-600"
              >
                <option value="1m">1 Minute</option>
                <option value="5m">5 Minutes</option>
                <option value="15m">15 Minutes</option>
                <option value="30m">30 Minutes</option>
                <option value="1h">1 Hour</option>
                <option value="4h">4 Hours</option>
                <option value="1d">1 Day</option>
                <option value="1w">1 Week</option>
              </select>
            </div>
            
            <Button
              size="sm"
              variant="outline"
              onClick={loadOhlcPreview}
              disabled={loadingOhlcPreview}
              className="flex items-center gap-1"
            >
              {loadingOhlcPreview ? (
                <Loader2 className="w-3 h-3 animate-spin" />
              ) : (
                <Activity className="w-3 h-3" />
              )}
              {t.common.refresh}
            </Button>

            <Button
              size="sm"
              variant={autoRefreshOhlc ? "default" : "outline"}
              onClick={() => setAutoRefreshOhlc(!autoRefreshOhlc)}
              className="flex items-center gap-1"
            >
              {autoRefreshOhlc ? (
                <>
                  <Pause className="w-3 h-3" />
                  Auto
                </>
              ) : (
                <>
                  <Play className="w-3 h-3" />
                  Manual
                </>
              )}
            </Button>
          </div>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
            <div className="lg:col-span-2">
              {ohlcChartData.length > 0 ? (
                <div>
                  {/* Data time range indicator */}
                  <div className="flex items-center justify-between text-xs text-muted-foreground px-1 mb-1">
                    <span>{ohlcChartData[0].fullTime}</span>
                    <span>← {ohlcChartData.length} candles →</span>
                    <span>{ohlcChartData[ohlcChartData.length - 1].fullTime}</span>
                  </div>
                  <div className="h-64">
                  <ResponsiveContainer width="100%" height="100%">
                    <LineChart data={ohlcChartData}>
                      <CartesianGrid strokeDasharray="3 3" />
                      <XAxis dataKey="time" />
                      <YAxis 
                        domain={['dataMin - 10', 'dataMax + 10']}
                        tickFormatter={(value) => `$${value.toFixed(0)}`}
                      />
                      <Tooltip
                        content={({ active, payload }) => {
                          if (!active || !payload?.length) return null;
                          const d = payload[0].payload;
                          return (
                            <div className="bg-background border rounded-lg shadow-lg p-3 text-xs">
                              <p className="font-medium mb-1">{d.fullTime || d.time}</p>
                              <div className="grid grid-cols-2 gap-x-4 gap-y-0.5">
                                <span className="text-muted-foreground">Open:</span>
                                <span className="font-mono">${d.open.toFixed(2)}</span>
                                <span className="text-muted-foreground">High:</span>
                                <span className="font-mono text-emerald-500">${d.high.toFixed(2)}</span>
                                <span className="text-muted-foreground">Low:</span>
                                <span className="font-mono text-red-500">${d.low.toFixed(2)}</span>
                                <span className="text-muted-foreground">Close:</span>
                                <span className="font-mono font-bold">${d.price.toFixed(2)}</span>
                                <span className="text-muted-foreground">Volume:</span>
                                <span className="font-mono">{d.volume.toFixed(4)}</span>
                              </div>
                            </div>
                          );
                        }}
                      />
                      <Line
                        type="monotone"
                        dataKey="price"
                        stroke="#3b82f6"
                        strokeWidth={2}
                        dot={{ r: 3 }}
                      />
                    </LineChart>
                  </ResponsiveContainer>
                </div>
                </div>
              ) : (
                <div className="h-64 flex items-center justify-center text-gray-500">
                  {loadingOhlcPreview ? (
                    <div className="flex items-center gap-2">
                      <Loader2 className="w-5 h-5 animate-spin" />
                      <span>{t.dashboard.loadingOhlc}</span>
                    </div>
                  ) : (
                    <span>{t.dashboard.noOhlcData}</span>
                  )}
                </div>
              )}
            </div>
            
            <div className="space-y-4">
              <div>
                <h4 className="font-medium text-blue-800 dark:text-blue-200 mb-2">
                  {t.dashboard.latestOhlcCandle}
                </h4>
                {ohlcPreview.length > 0 ? (
                  <div className="space-y-2 text-sm">
                    <div className="flex justify-between">
                      <span className="text-muted-foreground">Time:</span>
                      <span className="font-mono text-xs font-medium">
                        {new Date(ohlcPreview[ohlcPreview.length - 1].timestamp).toLocaleString([], {
                          year: 'numeric', month: '2-digit', day: '2-digit',
                          hour: '2-digit', minute: '2-digit', second: '2-digit'
                        })}
                      </span>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-muted-foreground">Open:</span>
                      <span className="font-mono">${parseFloat(ohlcPreview[ohlcPreview.length - 1].open).toFixed(2)}</span>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-muted-foreground">High:</span>
                      <span className="font-mono text-green-600">${parseFloat(ohlcPreview[ohlcPreview.length - 1].high).toFixed(2)}</span>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-muted-foreground">Low:</span>
                      <span className="font-mono text-red-600">${parseFloat(ohlcPreview[ohlcPreview.length - 1].low).toFixed(2)}</span>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-muted-foreground">Close:</span>
                      <span className="font-mono font-bold">${parseFloat(ohlcPreview[ohlcPreview.length - 1].close).toFixed(2)}</span>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-muted-foreground">Volume:</span>
                      <span className="font-mono">{parseFloat(ohlcPreview[ohlcPreview.length - 1].volume).toFixed(4)}</span>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-muted-foreground">Trades:</span>
                      <span className="font-mono">{ohlcPreview[ohlcPreview.length - 1].trade_count}</span>
                    </div>
                  </div>
                ) : (
                  <div className="text-sm text-gray-500 italic">
                    {loadingOhlcPreview ? t.dashboard.loadingOhlc : t.dashboard.noOhlcData}
                  </div>
                )}
              </div>
              
              <div className="pt-2 border-t">
                <p className="text-xs text-blue-600 dark:text-blue-400">
                  {t.dashboard.ohlcNote}
                </p>
                <p className="text-xs text-gray-500 mt-1">
                  {t.dashboard.selectTimeframe}: {selectedTimeframe.toUpperCase()} •
                  {t.dashboard.candles}: {ohlcPreview.length} •
                  {selectedTimeframe === '1m' && t.dashboard.lastHour}
                  {selectedTimeframe === '5m' && t.dashboard.last4Hours}
                  {selectedTimeframe === '15m' && t.dashboard.last8Hours}
                  {selectedTimeframe === '30m' && t.dashboard.last12Hours}
                  {selectedTimeframe === '1h' && t.dashboard.lastDay}
                  {selectedTimeframe === '4h' && t.dashboard.last4Days}
                  {selectedTimeframe === '1d' && t.dashboard.lastMonth}
                  {selectedTimeframe === '1w' && t.dashboard.last3Months}
                </p>
                {ohlcPreview.length > 0 && (
                  <p className="text-xs text-green-600 dark:text-green-400 mt-1">
                    Latest data: {new Date(ohlcPreview[ohlcPreview.length - 1].timestamp).toLocaleString([], {
                      year: 'numeric', month: '2-digit', day: '2-digit',
                      hour: '2-digit', minute: '2-digit', second: '2-digit'
                    })}
                  </p>
                )}
                {lastFetchTime && (
                  <p className="text-xs text-gray-400 mt-1">
                    Last fetch: {lastFetchTime.toLocaleTimeString()} •
                    {autoRefreshOhlc ? ' Auto-refresh: 30s' : ' Manual mode'}
                  </p>
                )}
              </div>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Enhanced Strategy Capabilities */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Activity className="w-5 h-5" />
            {t.dashboard.strategyCapabilities}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {strategyCapabilities.map((strategy) => {
              // 用翻译替代后端英文
              const strategyNames: Record<string, string> = {
                sma: t.dashboard.strategySmaName,
                rsi: t.dashboard.strategyRsiName,
                trend: t.dashboard.strategyTrendName,
              };
              const strategyDescs: Record<string, string> = {
                sma: t.dashboard.strategySmaDesc,
                rsi: t.dashboard.strategyRsiDesc,
                trend: t.dashboard.strategyTrendDesc,
              };
              const displayName = strategyNames[strategy.id] || strategy.name;
              const displayDesc = strategyDescs[strategy.id] || strategy.description;

              return (
              <div
                key={strategy.id}
                className={`p-4 border rounded-lg ${
                  strategy.supports_ohlc
                    ? 'border-blue-200 bg-blue-50 dark:border-blue-800 dark:bg-blue-900/20'
                    : 'border-gray-200 bg-gray-50 dark:border-gray-700 dark:bg-gray-800/20'
                }`}
              >
                <div className="flex items-start justify-between mb-2">
                  <div>
                    <h4 className="font-medium flex items-center gap-2">
                      {displayName}
                      {strategy.supports_ohlc && (
                        <Badge variant="secondary" className="text-xs">
                          <Layers className="w-3 h-3 mr-1" />
                          OHLC
                        </Badge>
                      )}
                    </h4>
                    <p className="text-sm text-gray-600 dark:text-gray-400 mt-1">
                      {displayDesc}
                    </p>
                  </div>
                  <Badge variant="outline" className="text-xs">
                    {strategy.id.toUpperCase()}
                  </Badge>
                </div>
                
                <div className="mt-3 flex items-center justify-between text-xs">
                  <div className="flex items-center gap-3">
                    <span className={`flex items-center gap-1 ${
                      strategy.supports_ohlc ? 'text-blue-600' : 'text-gray-500'
                    }`}>
                      <Layers className="w-3 h-3" />
                      {strategy.supports_ohlc ? t.dashboard.ohlcReady : t.dashboard.tickOnly}
                    </span>
                    {strategy.preferred_timeframe && (
                      <span className="flex items-center gap-1 text-purple-600">
                        <Timer className="w-3 h-3" />
                        {strategy.preferred_timeframe}
                      </span>
                    )}
                  </div>
                </div>
              </div>
              );
            })}
          </div>
          
          <div className="mt-6 pt-4 border-t">
            <Link href="/trading">
              <Button className="w-full">
                {t.dashboard.configureAdvancedBacktest}
              </Button>
            </Link>
          </div>
        </CardContent>
      </Card>

      {/* Enhanced Quick Backtest Results */}
      {(quickResults.length > 0 || isRunningQuick) && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Zap className="w-5 h-5" />
              {t.dashboard.quickStrategyTest}
              {isRunningQuick && <Loader2 className="w-4 h-4 animate-spin ml-2" />}
            </CardTitle>
          </CardHeader>
          <CardContent>
            {isRunningQuick && quickResults.length === 0 && (
              <div className="flex items-center justify-center py-8">
                <Loader2 className="w-6 h-6 animate-spin mr-2" />
                <span>{t.dashboard.runningQuickTests}</span>
              </div>
            )}
            
            {quickResults.length > 0 && (
              <div className="space-y-4">
                <div className="grid grid-cols-1 md:grid-cols-4 gap-4 mb-4">
                  <div className="text-center">
                    <p className="text-2xl font-bold text-green-600">
                      {quickResults.filter(r => r.return_pct > 0).length}
                    </p>
                    <p className="text-sm text-gray-500">{t.dashboard.profitableTests}</p>
                  </div>
                  <div className="text-center">
                    <p className="text-2xl font-bold text-blue-600">
                      {getAvgProcessingTime().toFixed(0)}ms
                    </p>
                    <p className="text-sm text-gray-500">{t.dashboard.avgProcessingTime}</p>
                  </div>
                  <div className="text-center">
                    <p className="text-2xl font-bold text-purple-600">
                      {quickResults.reduce((sum, r) => sum + r.trades, 0)}
                    </p>
                    <p className="text-sm text-gray-500">{t.dashboard.totalTrades}</p>
                  </div>
                  <div className="text-center">
                    <p className="text-2xl font-bold text-orange-600">
                      {quickResults.filter(r => r.data_source.startsWith('OHLC')).length}
                    </p>
                    <p className="text-sm text-gray-500">{t.dashboard.ohlcTests}</p>
                  </div>
                </div>

                <div className="overflow-x-auto">
                  <table className="w-full">
                    <thead>
                      <tr className="text-left border-b">
                        <th className="pb-2">{t.dashboard.strategy}</th>
                        <th className="pb-2">{t.dashboard.symbol}</th>
                        <th className="pb-2">{t.dashboard.dataSource}</th>
                        <th className="pb-2">{t.dashboard.returnValue}</th>
                        <th className="pb-2">{t.dashboard.finalValue}</th>
                        <th className="pb-2">{t.dashboard.totalTrades}</th>
                        <th className="pb-2">{t.dashboard.time}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {quickResults
                        .sort((a, b) => b.return_pct - a.return_pct)
                        .map((result, index) => (
                          <tr key={index} className="border-b">
                            <td className="py-2 font-medium">{result.strategy}</td>
                            <td className="py-2">{result.symbol}</td>
                            <td className="py-2">
                              <Badge 
                                variant={result.data_source.startsWith('OHLC') ? 'default' : 'secondary'}
                                className="text-xs"
                              >
                                {result.data_source.startsWith('OHLC') 
                                  ? `OHLC-${result.data_source.split('-')[1]}`
                                  : 'Tick'
                                }
                              </Badge>
                            </td>
                            <td className={`py-2 font-medium ${
                              result.return_pct >= 0 ? 'text-green-500' : 'text-red-500'
                            }`}>
                              {result.return_pct >= 0 ? '+' : ''}{result.return_pct.toFixed(2)}%
                            </td>
                            <td className="py-2">${result.final_value.toFixed(2)}</td>
                            <td className="py-2">{result.trades}</td>
                            <td className="py-2 text-gray-500">{result.processing_time}ms</td>
                          </tr>
                        ))}
                    </tbody>
                  </table>
                </div>
              </div>
            )}
          </CardContent>
        </Card>
      )}

      {/* Data Quality Overview - Enhanced */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Eye className="w-5 h-5" />
            {t.dashboard.marketDataOverview}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
            <div>
              <h4 className="font-medium mb-3">{t.dashboard.topSymbolsByVolume}</h4>
              <div className="space-y-2">
                {dataInfo?.symbol_info
                  .sort((a, b) => parseFloat(b.total_volume_usd) - parseFloat(a.total_volume_usd))
                  .slice(0, 5)
                  .map((symbol, index) => (
                  <div key={symbol.symbol} className="flex items-center justify-between">
                    <span className="flex items-center gap-2">
                      <span className="w-6 h-6 bg-blue-500 text-white text-xs rounded-full flex items-center justify-center">
                        {index + 1}
                      </span>
                      {symbol.symbol}
                    </span>
                    <div className="flex items-center gap-2">
                      <span className="text-sm text-gray-500 font-mono">
                        ${parseFloat(symbol.total_volume_usd).toLocaleString('en-US', { maximumFractionDigits: 0 })}
                      </span>
                      <Badge variant="outline" className="text-xs">
                        {t.dashboard.ohlcReady}
                      </Badge>
                    </div>
                  </div>
                ))}
              </div>
            </div>
            
            <div>
              <h4 className="font-medium mb-3">{t.dashboard.dataCapabilities}</h4>
              <div className="space-y-3">
                <div className="flex items-center justify-between p-3 bg-blue-50 dark:bg-blue-900/20 rounded-lg">
                  <div className="flex items-center gap-2">
                    <Layers className="w-4 h-4 text-blue-600" />
                    <span className="font-medium">{t.dashboard.ohlcGeneration}</span>
                  </div>
                  <Badge variant="secondary">{t.dashboard.active}</Badge>
                </div>
                <div className="flex items-center justify-between p-3 bg-green-50 dark:bg-green-900/20 rounded-lg">
                  <div className="flex items-center gap-2">
                    <Database className="w-4 h-4 text-green-600" />
                    <span className="font-medium">{t.dashboard.tickData}</span>
                  </div>
                  <Badge variant="secondary">
                    {dataInfo?.total_records.toLocaleString() || '0'}
                  </Badge>
                </div>
                <div className="flex items-center justify-between p-3 bg-purple-50 dark:bg-purple-900/20 rounded-lg">
                  <div className="flex items-center gap-2">
                    <Timer className="w-4 h-4 text-purple-600" />
                    <span className="font-medium">{t.dashboard.timeframes}</span>
                  </div>
                  <Badge variant="secondary">1m-1w</Badge>
                </div>
                <div className="flex items-center justify-between p-3 bg-orange-50 dark:bg-orange-900/20 rounded-lg">
                  <div className="flex items-center gap-2">
                    <Clock className="w-4 h-4 text-orange-600" />
                    <span className="font-medium">{t.dashboard.coverage}</span>
                  </div>
                  <Badge variant="secondary">
                    {getDataCoverageDays()} {t.dashboard.days}
                  </Badge>
                </div>
              </div>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Error Display */}
      {error && (
        <Card className="border-red-200 bg-red-50 dark:border-red-800 dark:bg-red-900/20">
          <CardContent className="pt-6">
            <p className="text-red-800 dark:text-red-200">{error}</p>
          </CardContent>
        </Card>
      )}
    </div>
  );
}