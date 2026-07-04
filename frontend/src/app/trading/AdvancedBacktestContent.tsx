'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer } from 'recharts';
import {
  Loader2, AlertCircle, CheckCircle, Play, TrendingUp,
  BarChart3, GitBranch, TestTube, Globe, Activity, Shield
} from 'lucide-react';
import {
  DataInfoResponse,
  BacktestResponse,
  MultiTimeframeBacktestRequest,
  WalkForwardRequest,
  WalkForwardResult,
  OutOfSampleRequest,
  OutOfSampleResult,
  MultiSymbolBacktestRequest,
  MultiSymbolBacktestResult,
  MarketStateAnalysisRequest,
  MarketStateResult,
} from '@/types/backtest';
import { useLanguage } from '@/lib/i18n/context';
import type { Translations } from '@/lib/i18n/translations/en';

export default function AdvancedBacktestContent() {
  const { t } = useLanguage();
  const [dataInfo, setDataInfo] = useState<DataInfoResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // 多时间框架回测状态
  const [mtfRunning, setMtfRunning] = useState(false);
  const [mtfResult, setMtfResult] = useState<BacktestResponse | null>(null);
  const [mtfParams, setMtfParams] = useState({
    strategy: 'trend',
    symbol: '',
    capital: 10000,
    data_count: 50000,
    commission_rate: 0.1,
  });

  // 滚动前进测试状态
  const [wfRunning, setWfRunning] = useState(false);
  const [wfResult, setWfResult] = useState<WalkForwardResult | null>(null);
  const [wfParams, setWfParams] = useState({
    strategy: 'trend',
    symbol: '',
    capital: 10000,
    commission_rate: 0.1,
    train_candles: 43200,
    test_candles: 10080,
    step_candles: 10080,
    data_count: 100000,
  });

  // 样本外测试状态
  const [osRunning, setOsRunning] = useState(false);
  const [osResult, setOsResult] = useState<OutOfSampleResult | null>(null);
  const [osParams, setOsParams] = useState({
    strategy: 'trend',
    symbol: '',
    capital: 10000,
    commission_rate: 0.1,
    train_ratio: 0.7,
    data_count: 50000,
  });

  // 多交易对回测状态
  const [msRunning, setMsRunning] = useState(false);
  const [msResult, setMsResult] = useState<MultiSymbolBacktestResult | null>(null);
  const [msParams, setMsParams] = useState({
    strategy: 'trend',
    symbols: [] as string[],
    capital: 10000,
    commission_rate: 0.1,
    data_count: 50000,
    market_state_window: 50,
  });

  // 市场状态分析状态
  const [mktRunning, setMktRunning] = useState(false);
  const [mktResult, setMktResult] = useState<MarketStateResult | null>(null);
  const [mktParams, setMktParams] = useState({
    symbol: '',
    data_count: 50000,
    window: 50,
  });

  useEffect(() => {
    initializeData();
  }, []);

  const initializeData = async () => {
    try {
      setLoading(true);
      const info = await invoke<DataInfoResponse>('get_data_info');
      setDataInfo(info);
      if (info.symbol_info.length > 0) {
        const topSymbol = info.symbol_info.sort((a, b) => b.records_count - a.records_count)[0].symbol;
        setMtfParams(p => ({ ...p, symbol: topSymbol }));
        setWfParams(p => ({ ...p, symbol: topSymbol }));
        setOsParams(p => ({ ...p, symbol: topSymbol }));
        setMktParams(p => ({ ...p, symbol: topSymbol }));
        // 多交易对默认选前3个
        const top3 = info.symbol_info
          .sort((a, b) => b.records_count - a.records_count)
          .slice(0, 3)
          .map(s => s.symbol);
        setMsParams(p => ({ ...p, symbols: top3 }));
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load data');
    } finally {
      setLoading(false);
    }
  };

  const runMultiTimeframeBacktest = async () => {
    try {
      setMtfRunning(true);
      setError(null);
      setMtfResult(null);
      const request: MultiTimeframeBacktestRequest = {
        strategy: mtfParams.strategy,
        symbol: mtfParams.symbol,
        capital: mtfParams.capital,
        data_count: mtfParams.data_count,
        commission_rate: mtfParams.commission_rate,
      };
      const result = await invoke<BacktestResponse>('run_multi_timeframe_backtest', { request });
      setMtfResult(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Multi-timeframe backtest failed');
    } finally {
      setMtfRunning(false);
    }
  };

  const runWalkForwardTest = async () => {
    try {
      setWfRunning(true);
      setError(null);
      setWfResult(null);
      const request: WalkForwardRequest = {
        strategy: wfParams.strategy,
        symbol: wfParams.symbol,
        capital: wfParams.capital,
        commission_rate: wfParams.commission_rate,
        train_candles: wfParams.train_candles,
        test_candles: wfParams.test_candles,
        step_candles: wfParams.step_candles,
        data_count: wfParams.data_count,
      };
      const result = await invoke<WalkForwardResult>('run_walk_forward_test', { request });
      setWfResult(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Walk-forward test failed');
    } finally {
      setWfRunning(false);
    }
  };

  const runOutOfSampleTest = async () => {
    try {
      setOsRunning(true);
      setError(null);
      setOsResult(null);
      const request: OutOfSampleRequest = {
        strategy: osParams.strategy,
        symbol: osParams.symbol,
        capital: osParams.capital,
        commission_rate: osParams.commission_rate,
        train_ratio: osParams.train_ratio,
        data_count: osParams.data_count,
      };
      const result = await invoke<OutOfSampleResult>('run_out_of_sample_test', { request });
      setOsResult(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Out-of-sample test failed');
    } finally {
      setOsRunning(false);
    }
  };

  const runMultiSymbolBacktest = async () => {
    try {
      setMsRunning(true);
      setError(null);
      setMsResult(null);
      const request: MultiSymbolBacktestRequest = {
        strategy: msParams.strategy,
        symbols: msParams.symbols,
        capital: msParams.capital,
        commission_rate: msParams.commission_rate,
        data_count: msParams.data_count,
        market_state_window: msParams.market_state_window,
      };
      const result = await invoke<MultiSymbolBacktestResult>('run_multi_symbol_backtest', { request });
      setMsResult(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Multi-symbol backtest failed');
    } finally {
      setMsRunning(false);
    }
  };

  const runMarketStateAnalysis = async () => {
    try {
      setMktRunning(true);
      setError(null);
      setMktResult(null);
      const request: MarketStateAnalysisRequest = {
        symbol: mktParams.symbol,
        data_count: mktParams.data_count,
        window: mktParams.window,
      };
      const result = await invoke<MarketStateResult>('analyze_market_state', { request });
      setMktResult(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Market state analysis failed');
    } finally {
      setMktRunning(false);
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center py-20">
        <Loader2 className="w-6 h-6 animate-spin mr-2" />
        <span>{t.advancedBacktest.running}</span>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-bold tracking-tight flex items-center gap-2">
            <TrendingUp className="w-5 h-5" />
            {t.advancedBacktest.title}
          </h2>
          <p className="text-sm text-muted-foreground mt-1">
            {t.advancedBacktest.subtitle}
          </p>
        </div>
        <Badge variant="outline" className="flex items-center gap-1.5">
          <Shield className="w-3 h-3" />
          {t.advancedBacktest.overfitDetection}
        </Badge>
      </div>

      {/* Error */}
      {error && (
        <Card className="border-destructive/50 bg-destructive/5">
          <CardContent className="pt-6">
            <div className="flex items-center gap-2 text-destructive">
              <AlertCircle className="w-5 h-5" />
              <span>{error}</span>
            </div>
          </CardContent>
        </Card>
      )}

      <Tabs defaultValue="multi-tf" className="space-y-6">
        <TabsList className="h-11 w-full justify-start gap-1 bg-muted/50 p-1">
          <TabsTrigger value="multi-tf" className="gap-2 px-4">
            <BarChart3 className="w-4 h-4" />
            {t.advancedBacktest.multiTimeframe}
          </TabsTrigger>
          <TabsTrigger value="walk-forward" className="gap-2 px-4">
            <GitBranch className="w-4 h-4" />
            {t.advancedBacktest.walkForward}
          </TabsTrigger>
          <TabsTrigger value="out-of-sample" className="gap-2 px-4">
            <TestTube className="w-4 h-4" />
            {t.advancedBacktest.outOfSample}
          </TabsTrigger>
          <TabsTrigger value="multi-symbol" className="gap-2 px-4">
            <Globe className="w-4 h-4" />
            {t.advancedBacktest.multiSymbol}
          </TabsTrigger>
          <TabsTrigger value="market-state" className="gap-2 px-4">
            <Activity className="w-4 h-4" />
            {t.advancedBacktest.marketState}
          </TabsTrigger>
        </TabsList>

        {/* ============ Multi-Timeframe Backtest ============ */}
        <TabsContent value="multi-tf" className="space-y-6">
          <Card>
            <CardHeader className="pb-3">
              <CardTitle className="text-lg flex items-center gap-2">
                <BarChart3 className="w-5 h-5" />
                {t.advancedBacktest.mtfConfig}
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
                <div>
                  <label className="block text-sm font-medium mb-1.5">{t.advancedBacktest.strategyLabel}</label>
                  <select
                    value={mtfParams.strategy}
                    onChange={(e) => setMtfParams({ ...mtfParams, strategy: e.target.value })}
                    className="w-full p-2.5 border rounded-md bg-background text-sm"
                  >
                    <option value="trend">Multi-Timeframe Trend</option>
                  </select>
                </div>
                <div>
                  <label className="block text-sm font-medium mb-1.5">{t.advancedBacktest.symbolLabel}</label>
                  <select
                    value={mtfParams.symbol}
                    onChange={(e) => setMtfParams({ ...mtfParams, symbol: e.target.value })}
                    className="w-full p-2.5 border rounded-md bg-background text-sm"
                  >
                    {dataInfo?.symbol_info.map((s) => (
                      <option key={s.symbol} value={s.symbol}>
                        {s.symbol} ({s.records_count.toLocaleString()})
                      </option>
                    ))}
                  </select>
                </div>
                <div>
                  <label className="block text-sm font-medium mb-1.5">{t.advancedBacktest.capital}</label>
                  <input
                    type="number"
                    value={mtfParams.capital}
                    onChange={(e) => setMtfParams({ ...mtfParams, capital: Number(e.target.value) })}
                    className="w-full p-2.5 border rounded-md bg-background text-sm"
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium mb-1.5">{t.advancedBacktest.dataCount}</label>
                  <input
                    type="number"
                    value={mtfParams.data_count}
                    onChange={(e) => setMtfParams({ ...mtfParams, data_count: Number(e.target.value) })}
                    className="w-full p-2.5 border rounded-md bg-background text-sm"
                  />
                </div>
              </div>
              <Button onClick={runMultiTimeframeBacktest} disabled={mtfRunning || !mtfParams.symbol} className="mt-4">
                {mtfRunning ? (
                  <span className="flex items-center gap-2"><Loader2 className="w-4 h-4 animate-spin" />{t.advancedBacktest.running}</span>
                ) : (
                  <span className="flex items-center gap-2"><Play className="w-4 h-4" />{t.advancedBacktest.runMtfBacktest}</span>
                )}
              </Button>
            </CardContent>
          </Card>

          {mtfResult && renderBacktestResult(mtfResult, t)}
        </TabsContent>

        {/* ============ Walk-Forward Test ============ */}
        <TabsContent value="walk-forward" className="space-y-6">
          <Card>
            <CardHeader className="pb-3">
              <CardTitle className="text-lg flex items-center gap-2">
                <GitBranch className="w-5 h-5" />
                {t.advancedBacktest.wfConfig}
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
                <div>
                  <label className="block text-sm font-medium mb-1.5">{t.advancedBacktest.strategyLabel}</label>
                  <select
                    value={wfParams.strategy}
                    onChange={(e) => setWfParams({ ...wfParams, strategy: e.target.value })}
                    className="w-full p-2.5 border rounded-md bg-background text-sm"
                  >
                    <option value="trend">Multi-Timeframe Trend</option>
                  </select>
                </div>
                <div>
                  <label className="block text-sm font-medium mb-1.5">{t.advancedBacktest.symbolLabel}</label>
                  <select
                    value={wfParams.symbol}
                    onChange={(e) => setWfParams({ ...wfParams, symbol: e.target.value })}
                    className="w-full p-2.5 border rounded-md bg-background text-sm"
                  >
                    {dataInfo?.symbol_info.map((s) => (
                      <option key={s.symbol} value={s.symbol}>{s.symbol}</option>
                    ))}
                  </select>
                </div>
                <div>
                  <label className="block text-sm font-medium mb-1.5">{t.advancedBacktest.trainCandles}</label>
                  <input
                    type="number"
                    value={wfParams.train_candles}
                    onChange={(e) => setWfParams({ ...wfParams, train_candles: Number(e.target.value) })}
                    className="w-full p-2.5 border rounded-md bg-background text-sm"
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium mb-1.5">{t.advancedBacktest.testCandles}</label>
                  <input
                    type="number"
                    value={wfParams.test_candles}
                    onChange={(e) => setWfParams({ ...wfParams, test_candles: Number(e.target.value) })}
                    className="w-full p-2.5 border rounded-md bg-background text-sm"
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium mb-1.5">{t.advancedBacktest.stepCandles}</label>
                  <input
                    type="number"
                    value={wfParams.step_candles}
                    onChange={(e) => setWfParams({ ...wfParams, step_candles: Number(e.target.value) })}
                    className="w-full p-2.5 border rounded-md bg-background text-sm"
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium mb-1.5">{t.advancedBacktest.totalDataCount}</label>
                  <input
                    type="number"
                    value={wfParams.data_count}
                    onChange={(e) => setWfParams({ ...wfParams, data_count: Number(e.target.value) })}
                    className="w-full p-2.5 border rounded-md bg-background text-sm"
                  />
                </div>
              </div>
              <Button onClick={runWalkForwardTest} disabled={wfRunning || !wfParams.symbol} className="mt-4">
                {wfRunning ? (
                  <span className="flex items-center gap-2"><Loader2 className="w-4 h-4 animate-spin" />{t.advancedBacktest.running}</span>
                ) : (
                  <span className="flex items-center gap-2"><Play className="w-4 h-4" />{t.advancedBacktest.runWfTest}</span>
                )}
              </Button>
            </CardContent>
          </Card>

          {wfResult && renderWalkForwardResult(wfResult, t)}
        </TabsContent>

        {/* ============ Out-of-Sample Test ============ */}
        <TabsContent value="out-of-sample" className="space-y-6">
          <Card>
            <CardHeader className="pb-3">
              <CardTitle className="text-lg flex items-center gap-2">
                <TestTube className="w-5 h-5" />
                {t.advancedBacktest.osConfig}
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
                <div>
                  <label className="block text-sm font-medium mb-1.5">{t.advancedBacktest.strategyLabel}</label>
                  <select
                    value={osParams.strategy}
                    onChange={(e) => setOsParams({ ...osParams, strategy: e.target.value })}
                    className="w-full p-2.5 border rounded-md bg-background text-sm"
                  >
                    <option value="trend">Multi-Timeframe Trend</option>
                  </select>
                </div>
                <div>
                  <label className="block text-sm font-medium mb-1.5">{t.advancedBacktest.symbolLabel}</label>
                  <select
                    value={osParams.symbol}
                    onChange={(e) => setOsParams({ ...osParams, symbol: e.target.value })}
                    className="w-full p-2.5 border rounded-md bg-background text-sm"
                  >
                    {dataInfo?.symbol_info.map((s) => (
                      <option key={s.symbol} value={s.symbol}>{s.symbol}</option>
                    ))}
                  </select>
                </div>
                <div>
                  <label className="block text-sm font-medium mb-1.5">{t.advancedBacktest.trainRatio}</label>
                  <input
                    type="number"
                    value={osParams.train_ratio}
                    onChange={(e) => setOsParams({ ...osParams, train_ratio: Number(e.target.value) })}
                    className="w-full p-2.5 border rounded-md bg-background text-sm"
                    step="0.05"
                    min="0.5"
                    max="0.9"
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium mb-1.5">{t.advancedBacktest.dataCount}</label>
                  <input
                    type="number"
                    value={osParams.data_count}
                    onChange={(e) => setOsParams({ ...osParams, data_count: Number(e.target.value) })}
                    className="w-full p-2.5 border rounded-md bg-background text-sm"
                  />
                </div>
              </div>
              <Button onClick={runOutOfSampleTest} disabled={osRunning || !osParams.symbol} className="mt-4">
                {osRunning ? (
                  <span className="flex items-center gap-2"><Loader2 className="w-4 h-4 animate-spin" />{t.advancedBacktest.running}</span>
                ) : (
                  <span className="flex items-center gap-2"><Play className="w-4 h-4" />{t.advancedBacktest.runOsTest}</span>
                )}
              </Button>
            </CardContent>
          </Card>

          {osResult && renderOutOfSampleResult(osResult, t)}
        </TabsContent>

        {/* ============ Multi-Symbol Backtest ============ */}
        <TabsContent value="multi-symbol" className="space-y-6">
          <Card>
            <CardHeader className="pb-3">
              <CardTitle className="text-lg flex items-center gap-2">
                <Globe className="w-5 h-5" />
                {t.advancedBacktest.msConfig}
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                <div>
                  <label className="block text-sm font-medium mb-1.5">{t.advancedBacktest.strategyLabel}</label>
                  <select
                    value={msParams.strategy}
                    onChange={(e) => setMsParams({ ...msParams, strategy: e.target.value })}
                    className="w-full p-2.5 border rounded-md bg-background text-sm"
                  >
                    <option value="trend">Multi-Timeframe Trend</option>
                  </select>
                </div>
                <div>
                  <label className="block text-sm font-medium mb-1.5">{t.advancedBacktest.capital}</label>
                  <input
                    type="number"
                    value={msParams.capital}
                    onChange={(e) => setMsParams({ ...msParams, capital: Number(e.target.value) })}
                    className="w-full p-2.5 border rounded-md bg-background text-sm"
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium mb-1.5">{t.advancedBacktest.dataCountPerSymbol}</label>
                  <input
                    type="number"
                    value={msParams.data_count}
                    onChange={(e) => setMsParams({ ...msParams, data_count: Number(e.target.value) })}
                    className="w-full p-2.5 border rounded-md bg-background text-sm"
                  />
                </div>
              </div>
              <div className="mt-4">
                <label className="block text-sm font-medium mb-1.5">{t.advancedBacktest.symbols}</label>
                <div className="flex flex-wrap gap-2">
                  {dataInfo?.symbol_info.map((s) => (
                    <button
                      key={s.symbol}
                      onClick={() => {
                        setMsParams(prev => ({
                          ...prev,
                          symbols: prev.symbols.includes(s.symbol)
                            ? prev.symbols.filter(sym => sym !== s.symbol)
                            : [...prev.symbols, s.symbol]
                        }));
                      }}
                      className={`px-3 py-1.5 rounded-md text-sm font-medium transition-all ${
                        msParams.symbols.includes(s.symbol)
                          ? 'bg-primary text-primary-foreground'
                          : 'bg-muted text-muted-foreground hover:text-foreground'
                      }`}
                    >
                      {s.symbol}
                    </button>
                  ))}
                </div>
              </div>
              <Button onClick={runMultiSymbolBacktest} disabled={msRunning || msParams.symbols.length === 0} className="mt-4">
                {msRunning ? (
                  <span className="flex items-center gap-2"><Loader2 className="w-4 h-4 animate-spin" />{t.advancedBacktest.running}</span>
                ) : (
                  <span className="flex items-center gap-2"><Play className="w-4 h-4" />{t.advancedBacktest.runMsBacktest}</span>
                )}
              </Button>
            </CardContent>
          </Card>

          {msResult && renderMultiSymbolResult(msResult, t)}
        </TabsContent>

        {/* ============ Market State Analysis ============ */}
        <TabsContent value="market-state" className="space-y-6">
          <Card>
            <CardHeader className="pb-3">
              <CardTitle className="text-lg flex items-center gap-2">
                <Activity className="w-5 h-5" />
                {t.advancedBacktest.mktConfig}
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                <div>
                  <label className="block text-sm font-medium mb-1.5">{t.advancedBacktest.symbolLabel}</label>
                  <select
                    value={mktParams.symbol}
                    onChange={(e) => setMktParams({ ...mktParams, symbol: e.target.value })}
                    className="w-full p-2.5 border rounded-md bg-background text-sm"
                  >
                    {dataInfo?.symbol_info.map((s) => (
                      <option key={s.symbol} value={s.symbol}>{s.symbol}</option>
                    ))}
                  </select>
                </div>
                <div>
                  <label className="block text-sm font-medium mb-1.5">{t.advancedBacktest.dataCount}</label>
                  <input
                    type="number"
                    value={mktParams.data_count}
                    onChange={(e) => setMktParams({ ...mktParams, data_count: Number(e.target.value) })}
                    className="w-full p-2.5 border rounded-md bg-background text-sm"
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium mb-1.5">{t.advancedBacktest.windowSize}</label>
                  <input
                    type="number"
                    value={mktParams.window}
                    onChange={(e) => setMktParams({ ...mktParams, window: Number(e.target.value) })}
                    className="w-full p-2.5 border rounded-md bg-background text-sm"
                  />
                </div>
              </div>
              <Button onClick={runMarketStateAnalysis} disabled={mktRunning || !mktParams.symbol} className="mt-4">
                {mktRunning ? (
                  <span className="flex items-center gap-2"><Loader2 className="w-4 h-4 animate-spin" />{t.advancedBacktest.analyzing}</span>
                ) : (
                  <span className="flex items-center gap-2"><Play className="w-4 h-4" />{t.advancedBacktest.analyzeMarketState}</span>
                )}
              </Button>
            </CardContent>
          </Card>

          {mktResult && renderMarketStateResult(mktResult, t)}
        </TabsContent>
      </Tabs>
    </div>
  );
}

// ============ 渲染函数 ============

function renderBacktestResult(result: BacktestResponse, t: Translations) {
  return (
    <div className="space-y-6">
      <Card>
        <CardHeader className="pb-3">
          <div className="flex items-center justify-between">
            <CardTitle className="text-lg">{result.strategy_name} {t.backtestContent.results}</CardTitle>
            <Badge variant="default">{result.data_source}</Badge>
          </div>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-5 gap-4">
            <MetricCard label={t.backtestContent.returnValue} value={`${parseFloat(result.return_percentage).toFixed(2)}%`} color={parseFloat(result.return_percentage) >= 0 ? 'text-emerald-500' : 'text-red-500'} />
            <MetricCard label={t.backtestContent.finalValue} value={`$${parseFloat(result.final_value).toFixed(2)}`} />
            <MetricCard label={t.backtestContent.sharpe} value={parseFloat(result.sharpe_ratio).toFixed(2)} />
            <MetricCard label={t.backtestContent.maxDd} value={`${parseFloat(result.max_drawdown).toFixed(2)}%`} color="text-red-500" />
            <MetricCard label={t.backtestContent.winRate} value={`${parseFloat(result.win_rate).toFixed(1)}%`} />
            <MetricCard label={t.backtestContent.trades} value={result.total_trades.toString()} />
            <MetricCard label={t.backtestContent.wins} value={result.winning_trades.toString()} color="text-emerald-500" />
            <MetricCard label={t.backtestContent.losses} value={result.losing_trades.toString()} color="text-red-500" />
            <MetricCard label={t.backtestContent.profitFactor} value={parseFloat(result.profit_factor).toFixed(2)} />
            <MetricCard label={t.backtestContent.commission} value={`$${parseFloat(result.total_commission).toFixed(2)}`} />
          </div>
        </CardContent>
      </Card>

      {result.equity_curve?.length > 0 && (
        <Card>
          <CardHeader className="pb-3"><CardTitle className="text-lg">{t.backtestContent.equityCurve}</CardTitle></CardHeader>
          <CardContent>
            <div className="h-72">
              <ResponsiveContainer width="100%" height="100%">
                <LineChart data={result.equity_curve.map((v, i) => ({ index: i, value: parseFloat(v) }))}>
                  <CartesianGrid strokeDasharray="3 3" className="opacity-30" />
                  <XAxis dataKey="index" tick={{ fontSize: 10 }} />
                  <YAxis domain={['auto', 'auto']} tick={{ fontSize: 10 }} tickFormatter={(v) => `$${v.toFixed(0)}`} width={70} />
                  <Tooltip formatter={(v: number) => [`$${v.toFixed(2)}`, 'Value']} labelFormatter={(i) => `Trade #${i}`} />
                  <Line type="monotone" dataKey="value" stroke="#2563eb" dot={false} strokeWidth={2} />
                </LineChart>
              </ResponsiveContainer>
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

function renderWalkForwardResult(result: WalkForwardResult, t: Translations) {
  return (
    <div className="space-y-6">
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="text-lg">{t.advancedBacktest.wfSummary}</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-4">
            <MetricCard label={t.advancedBacktest.totalRounds} value={result.total_rounds.toString()} />
            <MetricCard label={t.advancedBacktest.profitableRounds} value={result.profitable_rounds.toString()} color="text-emerald-500" />
            <MetricCard label={t.advancedBacktest.overallTestReturn} value={result.overall_test_return_pct} />
            <MetricCard label={t.advancedBacktest.overallTestSharpe} value={result.overall_test_sharpe} />
            <MetricCard label={t.advancedBacktest.overallTestMaxDd} value={result.overall_test_max_drawdown} color="text-red-500" />
            <MetricCard label={t.advancedBacktest.overallTestWinRate} value={result.overall_test_win_rate} />
            <MetricCard label={t.advancedBacktest.avgOverfitRatio} value={result.avg_overfit_ratio} />
            <div className="flex items-center gap-2 p-3 rounded-lg border">
              <span className="text-sm text-muted-foreground">{t.advancedBacktest.overfitStatus}:</span>
              {result.is_overfit ? (
                <Badge variant="destructive">{t.advancedBacktest.overfit}</Badge>
              ) : (
                <Badge variant="default" className="bg-emerald-500">{t.advancedBacktest.ok}</Badge>
              )}
            </div>
          </div>
        </CardContent>
      </Card>

      {result.rounds.length > 0 && (
        <Card>
          <CardHeader className="pb-3">
            <CardTitle className="text-lg">{t.advancedBacktest.roundDetails} ({result.rounds.length})</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b text-left">
                    <th className="pb-2 font-medium text-muted-foreground">#</th>
                    <th className="pb-2 font-medium text-muted-foreground">{t.advancedBacktest.trainReturn}</th>
                    <th className="pb-2 font-medium text-muted-foreground">{t.advancedBacktest.testReturn}</th>
                    <th className="pb-2 font-medium text-muted-foreground">{t.advancedBacktest.trainSharpe}</th>
                    <th className="pb-2 font-medium text-muted-foreground">{t.advancedBacktest.testSharpe}</th>
                    <th className="pb-2 font-medium text-muted-foreground">{t.advancedBacktest.testWinRate}</th>
                    <th className="pb-2 font-medium text-muted-foreground">{t.advancedBacktest.testMaxDd}</th>
                    <th className="pb-2 font-medium text-muted-foreground">{t.advancedBacktest.overfitCol}</th>
                  </tr>
                </thead>
                <tbody>
                  {result.rounds.map((r, i) => (
                    <tr key={i} className="border-b last:border-0 hover:bg-muted/50">
                      <td className="py-2 text-muted-foreground">{r.round}</td>
                      <td className="py-2 font-mono">{r.train_return_pct}</td>
                      <td className="py-2 font-mono">{r.test_return_pct}</td>
                      <td className="py-2 font-mono">{r.train_sharpe}</td>
                      <td className="py-2 font-mono">{r.test_sharpe}</td>
                      <td className="py-2 font-mono">{r.test_win_rate}</td>
                      <td className="py-2 font-mono text-red-500">{r.test_max_drawdown}</td>
                      <td className="py-2">
                        <Badge variant={parseFloat(r.overfit_ratio) > 0.5 ? 'destructive' : 'secondary'} className="text-xs">
                          {r.overfit_ratio}
                        </Badge>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

function renderOutOfSampleResult(result: OutOfSampleResult, t: Translations) {
  return (
    <Card>
      <CardHeader className="pb-3">
        <CardTitle className="text-lg">{t.advancedBacktest.osResults}</CardTitle>
      </CardHeader>
      <CardContent>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
          <div>
            <h4 className="font-medium mb-3 text-blue-600">{t.advancedBacktest.trainSet}</h4>
            <div className="space-y-2">
              <MetricRow label={t.backtestContent.returnValue} value={result.train_return_pct} />
              <MetricRow label={t.backtestContent.sharpe} value={result.train_sharpe} />
              <MetricRow label={t.backtestContent.maxDd} value={result.train_max_drawdown} />
              <MetricRow label={t.backtestContent.winRate} value={result.train_win_rate} />
              <MetricRow label={t.backtestContent.trades} value={result.train_trades.toString()} />
              <MetricRow label={t.backtestContent.profitFactor} value={result.train_profit_factor} />
            </div>
          </div>
          <div>
            <h4 className="font-medium mb-3 text-purple-600">{t.advancedBacktest.testSet}</h4>
            <div className="space-y-2">
              <MetricRow label={t.backtestContent.returnValue} value={result.test_return_pct} />
              <MetricRow label={t.backtestContent.sharpe} value={result.test_sharpe} />
              <MetricRow label={t.backtestContent.maxDd} value={result.test_max_drawdown} />
              <MetricRow label={t.backtestContent.winRate} value={result.test_win_rate} />
              <MetricRow label={t.backtestContent.trades} value={result.test_trades.toString()} />
              <MetricRow label={t.backtestContent.profitFactor} value={result.test_profit_factor} />
            </div>
          </div>
        </div>
        <div className="mt-6 pt-4 border-t flex items-center justify-between">
          <div>
            <span className="text-sm text-muted-foreground">{t.advancedBacktest.avgOverfitRatio}: </span>
            <span className="font-mono font-bold">{result.overfit_ratio}</span>
          </div>
          {result.is_overfit ? (
            <Badge variant="destructive">{t.advancedBacktest.overfitDetected}</Badge>
          ) : (
            <Badge variant="default" className="bg-emerald-500">{t.advancedBacktest.noOverfit}</Badge>
          )}
        </div>
      </CardContent>
    </Card>
  );
}

function renderMultiSymbolResult(result: MultiSymbolBacktestResult, t: Translations) {
  return (
    <div className="space-y-6">
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="text-lg">{t.advancedBacktest.msSummary}</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-4">
            <MetricCard label={t.advancedBacktest.totalSymbols} value={result.total_symbols.toString()} />
            <MetricCard label={t.advancedBacktest.profitable} value={result.profitable_symbols.toString()} color="text-emerald-500" />
            <MetricCard label={t.advancedBacktest.losing} value={result.losing_symbols.toString()} color="text-red-500" />
            <MetricCard label={t.advancedBacktest.totalTrades} value={result.total_trades.toString()} />
            <MetricCard label={t.advancedBacktest.avgReturn} value={result.avg_return_pct} />
            <MetricCard label={t.advancedBacktest.avgSharpe} value={result.avg_sharpe} />
            <MetricCard label={t.advancedBacktest.avgWinRate} value={result.avg_win_rate} />
            <MetricCard label={t.advancedBacktest.avgMaxDd} value={result.avg_max_drawdown} color="text-red-500" />
          </div>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4 pt-4 border-t">
            <div className="p-3 bg-emerald-50 dark:bg-emerald-950/30 rounded-lg">
              <p className="text-xs text-muted-foreground">{t.advancedBacktest.bestSymbol}</p>
              <p className="text-lg font-bold text-emerald-600">{result.best_symbol} ({result.best_return_pct})</p>
            </div>
            <div className="p-3 bg-red-50 dark:bg-red-950/30 rounded-lg">
              <p className="text-xs text-muted-foreground">{t.advancedBacktest.worstSymbol}</p>
              <p className="text-lg font-bold text-red-600">{result.worst_symbol} ({result.worst_return_pct})</p>
            </div>
            <div className="p-3 bg-blue-50 dark:bg-blue-950/30 rounded-lg">
              <p className="text-xs text-muted-foreground">{t.advancedBacktest.crossSymbolCorrelation}</p>
              <p className="text-lg font-bold text-blue-600">{result.cross_symbol_correlation}</p>
            </div>
          </div>
        </CardContent>
      </Card>

      {result.symbols.length > 0 && (
        <Card>
          <CardHeader className="pb-3">
            <CardTitle className="text-lg">{t.advancedBacktest.perSymbolResults}</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b text-left">
                    <th className="pb-2 font-medium text-muted-foreground">{t.advancedBacktest.symbolLabel}</th>
                    <th className="pb-2 font-medium text-muted-foreground">{t.backtestContent.returnValue}</th>
                    <th className="pb-2 font-medium text-muted-foreground">{t.backtestContent.sharpe}</th>
                    <th className="pb-2 font-medium text-muted-foreground">{t.backtestContent.winRate}</th>
                    <th className="pb-2 font-medium text-muted-foreground">{t.backtestContent.maxDd}</th>
                    <th className="pb-2 font-medium text-muted-foreground">{t.backtestContent.trades}</th>
                    <th className="pb-2 font-medium text-muted-foreground">{t.backtestContent.profitFactor}</th>
                    <th className="pb-2 font-medium text-muted-foreground">{t.advancedBacktest.marketStateCol}</th>
                    <th className="pb-2 font-medium text-muted-foreground">{t.advancedBacktest.dataQuality}</th>
                  </tr>
                </thead>
                <tbody>
                  {result.symbols.map((s, i) => (
                    <tr key={i} className="border-b last:border-0 hover:bg-muted/50">
                      <td className="py-2 font-medium">{s.symbol}</td>
                      <td className="py-2 font-mono">{s.return_pct}</td>
                      <td className="py-2 font-mono">{s.sharpe}</td>
                      <td className="py-2 font-mono">{s.win_rate}</td>
                      <td className="py-2 font-mono text-red-500">{s.max_drawdown}</td>
                      <td className="py-2">{s.total_trades}</td>
                      <td className="py-2 font-mono">{s.profit_factor}</td>
                      <td className="py-2 text-xs">{s.market_state}</td>
                      <td className="py-2 text-xs">{s.data_quality}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

function renderMarketStateResult(result: MarketStateResult, t: Translations) {
  return (
    <Card>
      <CardHeader className="pb-3">
        <CardTitle className="text-lg">{t.advancedBacktest.mktResults} - {result.symbol}</CardTitle>
      </CardHeader>
      <CardContent>
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-6">
          <MetricCard label={t.advancedBacktest.totalCandles} value={result.total_candles.toString()} />
          <MetricCard label={t.advancedBacktest.avgVolatility} value={result.avg_volatility} />
          <MetricCard label={t.advancedBacktest.trendRatio} value={result.trend_ratio} />
          <MetricCard label={t.advancedBacktest.rangingRatio} value={result.ranging_ratio} />
          <MetricCard label={t.advancedBacktest.avgTrendStrength} value={result.avg_trend_strength} />
          <MetricCard label={t.advancedBacktest.dataQuality} value={result.data_quality_score} />
        </div>
        <div className="mb-4">
          <h4 className="font-medium mb-3">{t.advancedBacktest.stateDistribution}</h4>
          <div className="grid grid-cols-2 md:grid-cols-3 gap-2">
            {Object.entries(result.state_distribution).map(([state, pct]) => (
              <div key={state} className="flex items-center justify-between p-2 bg-muted/50 rounded-md">
                <span className="text-sm">{state}</span>
                <Badge variant="secondary">{pct}</Badge>
              </div>
            ))}
          </div>
        </div>
        <div className="pt-4 border-t">
          <p className="text-sm text-muted-foreground">
            <span className="font-medium">{t.advancedBacktest.summary}: </span>{result.summary}
          </p>
        </div>
      </CardContent>
    </Card>
  );
}

// ============ 辅助组件 ============

function MetricCard({ label, value, color }: { label: string; value: string; color?: string }) {
  return (
    <div>
      <p className="text-xs text-muted-foreground">{label}</p>
      <p className={`text-xl font-bold ${color || ''}`}>{value}</p>
    </div>
  );
}

function MetricRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between">
      <span className="text-sm text-muted-foreground">{label}</span>
      <span className="text-sm font-mono font-medium">{value}</span>
    </div>
  );
}
