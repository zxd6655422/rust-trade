'use client';

import { useState, useEffect } from 'react';
import { Badge } from '@/components/ui/badge';
import {
  Activity, TrendingUp, TrendingDown, Minus,
  RefreshCw, Settings2, Play, Pause, BarChart3,
  Target, Shield, Zap, Plus, Save, Trash2, Copy,
  ChevronDown, ChevronUp, Edit3, X
} from 'lucide-react';
import { useLanguage } from '@/lib/i18n/context';
import { invoke } from '@tauri-apps/api/core';

// 策略分析结果类型
interface StrategyAnalysis {
  strategy_id: string;
  strategy_name: string;
  timestamp: number;
  symbol: string;
  market_structure: {
    structure_type: string;
    confidence: number;
    description: string;
  };
  key_levels: {
    support: number[];
    resistance: number[];
    pivot: number | null;
  };
  bias: {
    direction: 'long' | 'short' | 'neutral';
    confidence: number;
    reasoning: string;
  };
  trade_setup: {
    entry_zone: [number, number];
    stop_loss: number;
    take_profit: number[];
    risk_reward: number;
    invalidation: string;
  } | null;
}

// 决策结果类型
interface DecisionResult {
  should_trade: boolean;
  direction: 'long' | 'short' | 'neutral';
  confidence: number;
  consensus_strategies: string[];
  trade_setup: {
    entry_zone: [number, number];
    stop_loss: number;
    take_profit: number[];
    risk_reward: number;
  } | null;
  market_structure: {
    structure_type: string;
    confidence: number;
    description: string;
  };
  reasoning: string;
}

// 策略实例类型
interface StrategyInstance {
  id: string;
  strategy_type: string;
  display_name: string;
  params: Record<string, unknown>;
  status: string;
  symbols: string[];
  auto_trade: boolean;
  position_size_pct: number;
  exchange: string;
  market_type: string;
  note: string;
}

// 策略参数模板
const STRATEGY_TEMPLATES: Record<string, {
  name: string;
  description: string;
  defaultParams: Record<string, unknown>;
  paramLabels: Record<string, string>;
}> = {
  trend: {
    name: '趋势策略',
    description: '多时间框架趋势跟踪策略',
    defaultParams: {
      fast_ma: 7,
      slow_ma: 25,
      trend_ma: 99,
      adx_threshold: 25,
      timeframes: ['1h', '4h'],
    },
    paramLabels: {
      fast_ma: '快速均线',
      slow_ma: '慢速均线',
      trend_ma: '趋势均线',
      adx_threshold: 'ADX阈值',
      timeframes: '时间框架',
    },
  },
  rsi: {
    name: 'RSI 策略',
    description: '基于相对强弱指数的反转策略',
    defaultParams: {
      period: 14,
      oversold: 30,
      overbought: 70,
      confirm_candles: 2,
    },
    paramLabels: {
      period: 'RSI周期',
      oversold: '超卖阈值',
      overbought: '超买阈值',
      confirm_candles: '确认K线',
    },
  },
  macd: {
    name: 'MACD 策略',
    description: '基于 MACD 指标的趋势策略',
    defaultParams: {
      fast_period: 12,
      slow_period: 26,
      signal_period: 9,
      histogram_threshold: 0,
    },
    paramLabels: {
      fast_period: '快线周期',
      slow_period: '慢线周期',
      signal_period: '信号线周期',
      histogram_threshold: '柱状图阈值',
    },
  },
  bollinger: {
    name: '布林带策略',
    description: '基于布林带的波动率策略',
    defaultParams: {
      period: 20,
      std_dev: 2.0,
      squeeze_threshold: 0.02,
    },
    paramLabels: {
      period: '均线周期',
      std_dev: '标准差倍数',
      squeeze_threshold: '挤压阈值',
    },
  },
};

export default function StrategyPage() {
  const { t } = useLanguage();
  const [selectedSymbol, setSelectedSymbol] = useState('BTCUSDT');
  const [strategies, setStrategies] = useState<StrategyInstance[]>([]);
  const [analyses, setAnalyses] = useState<Record<string, StrategyAnalysis>>({});
  const [decision, setDecision] = useState<DecisionResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [autoRefresh, setAutoRefresh] = useState(true);
  const [refreshInterval, setRefreshInterval] = useState(30);

  // 策略配置管理状态
  const [showConfig, setShowConfig] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editForm, setEditForm] = useState<Partial<StrategyInstance>>({});
  const [showNewForm, setShowNewForm] = useState(false);
  const [newForm, setNewForm] = useState({
    strategy_type: 'trend',
    display_name: '',
    symbols: ['BTCUSDT'],
    exchange: 'binance',
    market_type: 'futures',
  });
  const [availableSymbols, setAvailableSymbols] = useState<string[]>([]);

  // 加载策略列表
  useEffect(() => {
    loadStrategies();
  }, []);

  // 加载可用交易对（根据市场类型和交易所）
  useEffect(() => {
    loadAvailableSymbols(newForm.market_type, newForm.exchange);
  }, [newForm.market_type, newForm.exchange]);

  // 自动刷新分析结果
  useEffect(() => {
    if (autoRefresh && strategies.length > 0) {
      refreshAnalysis();
      const timer = setInterval(refreshAnalysis, refreshInterval * 1000);
      return () => clearInterval(timer);
    }
  }, [autoRefresh, strategies, selectedSymbol, refreshInterval]);

  const loadStrategies = async () => {
    try {
      const result = await invoke<StrategyInstance[]>('get_strategy_instances');
      setStrategies(result);
    } catch (error) {
      console.error('Failed to load strategies:', error);
    }
  };

  const loadAvailableSymbols = async (marketType?: string, exchange?: string) => {
    try {
      const result = await invoke<string[]>('get_available_symbols', {
        marketType: marketType || null,
        exchange: exchange || null,
      });
      setAvailableSymbols(result);
    } catch (error) {
      console.error('Failed to load symbols:', error);
      // 如果加载失败，使用默认列表
      setAvailableSymbols(['BTCUSDT', 'ETHUSDT', 'SOLUSDT', 'BNBUSDT', 'XRPUSDT', 'DOGEUSDT']);
    }
  };

  const refreshAnalysis = async () => {
    setLoading(true);
    try {
      const analysisResults: Record<string, StrategyAnalysis> = {};

      for (const strategy of strategies) {
        const symbol = strategy.symbols && strategy.symbols.length > 0
          ? strategy.symbols[0]
          : selectedSymbol;

        try {
          const analysis = await invoke<StrategyAnalysis>('get_strategy_analysis_simple', {
            strategyType: strategy.strategy_type,
            symbol: symbol,
          });
          analysisResults[strategy.id] = analysis;
        } catch (error) {
          // 不支持实时分析的策略，创建默认分析结果
          analysisResults[strategy.id] = {
            strategy_id: strategy.strategy_type,
            strategy_name: strategy.display_name || strategy.strategy_type,
            timestamp: Date.now(),
            symbol: symbol,
            market_structure: {
              structure_type: 'ranging',
              confidence: 0,
              description: '不支持实时分析',
            },
            key_levels: { support: [], resistance: [], pivot: null },
            bias: { direction: 'neutral', confidence: 0, reasoning: '该策略不支持实时分析' },
            trade_setup: null,
          };
        }
      }

      setAnalyses(analysisResults);

      // 获取综合决策
      const analysisList = Object.values(analysisResults).filter(a => a.bias.confidence > 0);
      if (analysisList.length > 0) {
        try {
          const decisionResult = await invoke<DecisionResult>('get_strategy_decision', {
            symbol: selectedSymbol,
            analyses: analysisList,
          });
          setDecision(decisionResult);
        } catch (error) {
          console.error('Failed to get decision:', error);
        }
      }
    } catch (error) {
      console.error('Failed to refresh analysis:', error);
    } finally {
      setLoading(false);
    }
  };

  // ========== 策略配置管理函数 ==========

  const startEdit = (strategy: StrategyInstance) => {
    setEditingId(strategy.id);
    setEditForm({ ...strategy });
    // 加载该市场类型和交易所的交易对列表
    loadAvailableSymbols(strategy.market_type, strategy.exchange);
  };

  const cancelEdit = () => {
    setEditingId(null);
    setEditForm({});
  };

  const saveEdit = async () => {
    if (!editingId || !editForm) return;

    try {
      await invoke('update_strategy_instance', {
        id: editingId,
        update: {
          display_name: editForm.display_name,
          params: editForm.params,
          symbols: editForm.symbols,
          auto_trade: editForm.auto_trade,
          position_size_pct: editForm.position_size_pct,
          exchange: editForm.exchange,
          market_type: editForm.market_type,
          note: editForm.note,
        },
      });
      await loadStrategies();
      setEditingId(null);
      setEditForm({});
    } catch (error) {
      console.error('Failed to save strategy:', error);
      alert('保存失败: ' + error);
    }
  };

  const updateStatus = async (id: string, status: string) => {
    try {
      await invoke('update_strategy_status', { id, status });
      await loadStrategies();
    } catch (error) {
      console.error('Failed to update status:', error);
    }
  };

  const deleteStrategy = async (id: string) => {
    if (!confirm('确定要删除这个策略吗？')) return;

    try {
      await invoke('delete_strategy_instance', { id });
      await loadStrategies();
    } catch (error) {
      console.error('Failed to delete strategy:', error);
    }
  };

  const duplicateStrategy = async (strategy: StrategyInstance) => {
    try {
      await invoke('create_strategy_instance', {
        request: {
          strategy_type: strategy.strategy_type,
          display_name: strategy.display_name + ' (副本)',
          params: strategy.params,
          symbols: strategy.symbols,
          auto_trade: false,
          position_size_pct: strategy.position_size_pct,
          exchange: strategy.exchange,
          market_type: strategy.market_type,
          note: strategy.note,
        },
      });
      await loadStrategies();
    } catch (error) {
      console.error('Failed to duplicate strategy:', error);
    }
  };

  const createStrategy = async () => {
    const template = STRATEGY_TEMPLATES[newForm.strategy_type];

    try {
      await invoke('create_strategy_instance', {
        request: {
          strategy_type: newForm.strategy_type,
          display_name: newForm.display_name || `${template.name}-${newForm.symbols[0]}`,
          params: template.defaultParams,
          symbols: newForm.symbols,
          auto_trade: false,
          position_size_pct: 10.0,
          exchange: newForm.exchange,
          market_type: newForm.market_type,
          note: '',
        },
      });
      await loadStrategies();
      setShowNewForm(false);
      setNewForm({
        strategy_type: 'trend',
        display_name: '',
        symbols: ['BTCUSDT'],
        exchange: 'binance',
        market_type: 'futures',
      });
    } catch (error) {
      console.error('Failed to create strategy:', error);
      alert('创建失败: ' + error);
    }
  };

  const updateEditParam = (key: string, value: unknown) => {
    setEditForm(prev => ({
      ...prev,
      params: { ...prev.params, [key]: value },
    }));
  };

  // ========== UI 辅助函数 ==========

  const getDirectionIcon = (direction: string) => {
    switch (direction) {
      case 'long': return <TrendingUp className="w-5 h-5 text-green-500" />;
      case 'short': return <TrendingDown className="w-5 h-5 text-red-500" />;
      default: return <Minus className="w-5 h-5 text-gray-400" />;
    }
  };

  const getDirectionColor = (direction: string) => {
    switch (direction) {
      case 'long': return 'text-green-500 bg-green-500/10 border-green-500/20';
      case 'short': return 'text-red-500 bg-red-500/10 border-red-500/20';
      default: return 'text-gray-400 bg-gray-400/10 border-gray-400/20';
    }
  };

  const getStructureIcon = (type: string) => {
    switch (type) {
      case 'trending_up': return <TrendingUp className="w-4 h-4 text-green-500" />;
      case 'trending_down': return <TrendingDown className="w-4 h-4 text-red-500" />;
      case 'ranging': return <Minus className="w-4 h-4 text-yellow-500" />;
      case 'breakout': return <Zap className="w-4 h-4 text-blue-500" />;
      default: return <Activity className="w-4 h-4 text-gray-400" />;
    }
  };

  // 渲染参数编辑器
  const renderParamEditor = (strategy_type: string, params: Record<string, unknown>, onChange: (key: string, value: unknown) => void) => {
    const template = STRATEGY_TEMPLATES[strategy_type];
    if (!template) return null;

    return (
      <div className="grid grid-cols-2 gap-3">
        {Object.entries(template.paramLabels).map(([key, label]) => (
          <div key={key} className="flex items-center gap-2">
            <label className="text-xs text-muted-foreground w-20 text-right">{label}</label>
            {Array.isArray(params[key]) ? (
              <input
                type="text"
                value={(params[key] as string[]).join(', ')}
                onChange={(e) => onChange(key, e.target.value.split(',').map(s => s.trim()))}
                className="flex-1 px-2 py-1 rounded border bg-background text-sm"
              />
            ) : typeof params[key] === 'number' ? (
              <input
                type="number"
                value={params[key] as number}
                onChange={(e) => onChange(key, parseFloat(e.target.value) || 0)}
                step={key.includes('std') || key.includes('threshold') ? 0.01 : 1}
                className="flex-1 px-2 py-1 rounded border bg-background text-sm"
              />
            ) : (
              <input
                type="text"
                value={String(params[key] || '')}
                onChange={(e) => onChange(key, e.target.value)}
                className="flex-1 px-2 py-1 rounded border bg-background text-sm"
              />
            )}
          </div>
        ))}
      </div>
    );
  };

  return (
    <div className="space-y-6">
      {/* 页面标题 */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">策略中心</h1>
          <p className="text-sm text-muted-foreground mt-1">
            策略分析、信号生成、配置管理
          </p>
        </div>
        <div className="flex items-center gap-3">
          <select
            value={selectedSymbol}
            onChange={(e) => setSelectedSymbol(e.target.value)}
            className="px-3 py-2 rounded-md border bg-background text-sm"
          >
            <option value="BTCUSDT">BTC/USDT</option>
            <option value="ETHUSDT">ETH/USDT</option>
            <option value="SOLUSDT">SOL/USDT</option>
          </select>

          <button
            onClick={() => setAutoRefresh(!autoRefresh)}
            className={`flex items-center gap-2 px-3 py-2 rounded-md text-sm font-medium transition-all border ${
              autoRefresh
                ? 'bg-green-500/10 text-green-500 border-green-500/20'
                : 'bg-muted text-muted-foreground'
            }`}
          >
            <RefreshCw className={`w-4 h-4 ${loading ? 'animate-spin' : ''}`} />
            {autoRefresh ? '自动刷新' : '手动刷新'}
          </button>

          <button
            onClick={refreshAnalysis}
            disabled={loading}
            className="flex items-center gap-2 px-4 py-2 rounded-md text-sm font-medium bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
          >
            <RefreshCw className={`w-4 h-4 ${loading ? 'animate-spin' : ''}`} />
            刷新分析
          </button>

          <button
            onClick={() => setShowConfig(!showConfig)}
            className={`flex items-center gap-2 px-4 py-2 rounded-md text-sm font-medium transition-all border ${
              showConfig
                ? 'bg-primary text-primary-foreground'
                : 'bg-muted text-muted-foreground hover:text-foreground'
            }`}
          >
            <Settings2 className="w-4 h-4" />
            策略配置
          </button>
        </div>
      </div>

      {/* 综合决策卡片 */}
      {decision && decision.should_trade && (
        <div className={`p-6 rounded-lg border-2 ${
          decision.direction === 'long'
            ? 'border-green-500/50 bg-green-500/5'
            : decision.direction === 'short'
            ? 'border-red-500/50 bg-red-500/5'
            : 'border-gray-200 bg-gray-50/5'
        }`}>
          <div className="flex items-start justify-between">
            <div>
              <h2 className="text-lg font-semibold mb-2">综合决策</h2>
              <p className="text-sm text-muted-foreground">{decision.reasoning}</p>
            </div>
            <div className="flex items-center gap-3">
              {getDirectionIcon(decision.direction)}
              <div className="text-right">
                <div className={`text-2xl font-bold ${
                  decision.direction === 'long' ? 'text-green-500' :
                  decision.direction === 'short' ? 'text-red-500' : 'text-gray-400'
                }`}>
                  {decision.direction === 'long' ? '做多' :
                   decision.direction === 'short' ? '做空' : '观望'}
                </div>
                <div className="text-sm text-muted-foreground">
                  置信度 {decision.confidence.toFixed(1)}%
                </div>
              </div>
            </div>
          </div>

          {decision.trade_setup && (
            <div className="mt-4 grid grid-cols-4 gap-4 p-4 rounded-md bg-background/50">
              <div>
                <div className="text-xs text-muted-foreground mb-1">入场区间</div>
                <div className="font-mono text-sm">
                  ${decision.trade_setup.entry_zone[0].toLocaleString()} - ${decision.trade_setup.entry_zone[1].toLocaleString()}
                </div>
              </div>
              <div>
                <div className="text-xs text-muted-foreground mb-1">止损</div>
                <div className="font-mono text-sm text-red-500">
                  ${decision.trade_setup.stop_loss.toLocaleString()}
                </div>
              </div>
              <div>
                <div className="text-xs text-muted-foreground mb-1">止盈目标</div>
                <div className="font-mono text-sm text-green-500">
                  {decision.trade_setup.take_profit.map(tp => `$${tp.toLocaleString()}`).join(' / ')}
                </div>
              </div>
              <div>
                <div className="text-xs text-muted-foreground mb-1">风险收益比</div>
                <div className="font-mono text-sm">
                  1:{decision.trade_setup.risk_reward.toFixed(2)}
                </div>
              </div>
            </div>
          )}

          <div className="mt-3 flex items-center gap-2">
            <span className="text-xs text-muted-foreground">共识策略:</span>
            {decision.consensus_strategies.map(name => (
              <Badge key={name} variant="secondary" className="text-xs">{name}</Badge>
            ))}
          </div>
        </div>
      )}

      {/* 策略配置面板（可折叠） */}
      {showConfig && (
        <div className="p-6 rounded-lg border bg-background">
          <div className="flex items-center justify-between mb-4">
            <h2 className="text-lg font-semibold">策略配置管理</h2>
            <button
              onClick={() => setShowNewForm(true)}
              className="flex items-center gap-2 px-3 py-1.5 rounded-md text-sm font-medium bg-primary text-primary-foreground hover:bg-primary/90"
            >
              <Plus className="w-4 h-4" />
              新建策略
            </button>
          </div>

          {/* 新建策略表单 */}
          {showNewForm && (
            <div className="p-4 rounded-md border mb-4 bg-muted/50">
              <h3 className="text-sm font-medium mb-3">新建策略实例</h3>
              <div className="grid grid-cols-3 gap-3">
                <div>
                  <label className="text-xs text-muted-foreground">策略类型</label>
                  <select
                    value={newForm.strategy_type}
                    onChange={(e) => setNewForm(prev => ({ ...prev, strategy_type: e.target.value }))}
                    className="w-full mt-1 px-2 py-1.5 rounded border bg-background text-sm"
                  >
                    {Object.entries(STRATEGY_TEMPLATES).map(([key, tpl]) => (
                      <option key={key} value={key}>{tpl.name}</option>
                    ))}
                  </select>
                </div>
                <div>
                  <label className="text-xs text-muted-foreground">显示名称</label>
                  <input
                    type="text"
                    value={newForm.display_name}
                    onChange={(e) => setNewForm(prev => ({ ...prev, display_name: e.target.value }))}
                    placeholder="留空自动生成"
                    className="w-full mt-1 px-2 py-1.5 rounded border bg-background text-sm"
                  />
                </div>
                <div>
                  <label className="text-xs text-muted-foreground">交易对</label>
                  <select
                    value={newForm.symbols[0] || ''}
                    onChange={(e) => setNewForm(prev => ({ ...prev, symbols: [e.target.value] }))}
                    className="w-full mt-1 px-2 py-1.5 rounded border bg-background text-sm"
                  >
                    {availableSymbols.map(symbol => (
                      <option key={symbol} value={symbol}>{symbol}</option>
                    ))}
                  </select>
                </div>
                <div>
                  <label className="text-xs text-muted-foreground">交易所</label>
                  <select
                    value={newForm.exchange}
                    onChange={(e) => {
                      const newExchange = e.target.value;
                      setNewForm(prev => ({ ...prev, exchange: newExchange }));
                      // 重新加载交易对列表
                      loadAvailableSymbols(newForm.market_type, newExchange).then(() => {
                        // 自动选择第一个可用的交易对
                        setNewForm(prev => ({
                          ...prev,
                          exchange: newExchange,
                          symbols: availableSymbols.length > 0 ? [availableSymbols[0]] : ['BTCUSDT'],
                        }));
                      });
                    }}
                    className="w-full mt-1 px-2 py-1.5 rounded border bg-background text-sm"
                  >
                    <option value="binance">Binance</option>
                    <option value="okx">OKX</option>
                  </select>
                </div>
                <div>
                  <label className="text-xs text-muted-foreground">市场类型</label>
                  <select
                    value={newForm.market_type}
                    onChange={(e) => {
                      const newMarketType = e.target.value;
                      setNewForm(prev => ({ ...prev, market_type: newMarketType }));
                      // 重新加载交易对列表
                      loadAvailableSymbols(newMarketType).then(() => {
                        // 自动选择第一个可用的交易对
                        setNewForm(prev => ({
                          ...prev,
                          market_type: newMarketType,
                          symbols: availableSymbols.length > 0 ? [availableSymbols[0]] : ['BTCUSDT'],
                        }));
                      });
                    }}
                    className="w-full mt-1 px-2 py-1.5 rounded border bg-background text-sm"
                  >
                    <option value="spot">现货</option>
                    <option value="futures">合约</option>
                  </select>
                </div>
              </div>
              <div className="flex justify-end gap-2 mt-3">
                <button
                  onClick={() => setShowNewForm(false)}
                  className="px-3 py-1.5 rounded text-sm border hover:bg-muted"
                >
                  取消
                </button>
                <button
                  onClick={createStrategy}
                  className="px-3 py-1.5 rounded text-sm bg-primary text-primary-foreground hover:bg-primary/90"
                >
                  创建
                </button>
              </div>
            </div>
          )}

          {/* 策略列表 */}
          <div className="space-y-3">
            {strategies.map(strategy => (
              <div key={strategy.id} className="p-4 rounded-md border">
                {editingId === strategy.id ? (
                  /* 编辑模式 */
                  <div className="space-y-3">
                    <div className="flex items-center justify-between">
                      <span className="text-sm font-medium">编辑策略</span>
                      <div className="flex gap-2">
                        <button onClick={cancelEdit} className="px-2 py-1 rounded text-xs border hover:bg-muted">
                          取消
                        </button>
                        <button onClick={saveEdit} className="px-2 py-1 rounded text-xs bg-primary text-primary-foreground hover:bg-primary/90">
                          <Save className="w-3 h-3 inline mr-1" />
                          保存
                        </button>
                      </div>
                    </div>

                    <div className="grid grid-cols-3 gap-3">
                      <div>
                        <label className="text-xs text-muted-foreground">显示名称</label>
                        <input
                          type="text"
                          value={editForm.display_name || ''}
                          onChange={(e) => setEditForm(prev => ({ ...prev, display_name: e.target.value }))}
                          className="w-full mt-1 px-2 py-1 rounded border bg-background text-sm"
                        />
                      </div>
                      <div>
                        <label className="text-xs text-muted-foreground">市场类型</label>
                        <select
                          value={editForm.market_type || ''}
                          onChange={(e) => {
                            const newMarketType = e.target.value;
                            setEditForm(prev => ({ ...prev, market_type: newMarketType }));
                            loadAvailableSymbols(newMarketType);
                          }}
                          className="w-full mt-1 px-2 py-1 rounded border bg-background text-sm"
                        >
                          <option value="spot">现货</option>
                          <option value="futures">合约</option>
                        </select>
                      </div>
                      <div>
                        <label className="text-xs text-muted-foreground">交易对</label>
                        <select
                          value={editForm.symbols?.[0] || ''}
                          onChange={(e) => setEditForm(prev => ({ ...prev, symbols: [e.target.value] }))}
                          className="w-full mt-1 px-2 py-1 rounded border bg-background text-sm"
                        >
                          {availableSymbols.map(symbol => (
                            <option key={symbol} value={symbol}>{symbol}</option>
                          ))}
                        </select>
                      </div>
                    </div>
                    <div className="grid grid-cols-3 gap-3 mt-3">
                      <div>
                        <label className="text-xs text-muted-foreground">交易所</label>
                        <select
                          value={editForm.exchange || ''}
                          onChange={(e) => {
                            const newExchange = e.target.value;
                            setEditForm(prev => ({ ...prev, exchange: newExchange }));
                            loadAvailableSymbols(editForm.market_type, newExchange);
                          }}
                          className="w-full mt-1 px-2 py-1 rounded border bg-background text-sm"
                        >
                          <option value="binance">Binance</option>
                          <option value="okx">OKX</option>
                        </select>
                      </div>
                      <div>
                        <label className="text-xs text-muted-foreground">仓位百分比</label>
                        <input
                          type="number"
                          value={editForm.position_size_pct || 10}
                          onChange={(e) => setEditForm(prev => ({ ...prev, position_size_pct: parseFloat(e.target.value) || 10 }))}
                          min={1}
                          max={100}
                          className="w-full mt-1 px-2 py-1 rounded border bg-background text-sm"
                        />
                      </div>
                      <div>
                        <label className="text-xs text-muted-foreground">备注</label>
                        <input
                          type="text"
                          value={editForm.note || ''}
                          onChange={(e) => setEditForm(prev => ({ ...prev, note: e.target.value }))}
                          className="w-full mt-1 px-2 py-1 rounded border bg-background text-sm"
                        />
                      </div>
                    </div>

                    <div>
                      <label className="text-xs text-muted-foreground">策略参数</label>
                      {renderParamEditor(strategy.strategy_type, editForm.params || {}, updateEditParam)}
                    </div>
                  </div>
                ) : (
                  /* 显示模式 */
                  <div>
                    <div className="flex items-center justify-between">
                      <div className="flex items-center gap-3">
                        <span className="font-medium">{strategy.display_name}</span>
                        <Badge variant={strategy.status === 'active' ? 'default' : 'secondary'} className="text-xs">
                          {strategy.status === 'active' ? '运行中' : '已暂停'}
                        </Badge>
                        <span className="text-xs text-muted-foreground">
                          {strategy.strategy_type.toUpperCase()} • {strategy.symbols.join(', ')} • {strategy.exchange.toUpperCase()}
                        </span>
                      </div>
                      <div className="flex gap-1">
                        <button onClick={() => startEdit(strategy)} className="p-1.5 rounded hover:bg-muted" title="编辑">
                          <Edit3 className="w-3.5 h-3.5" />
                        </button>
                        <button onClick={() => duplicateStrategy(strategy)} className="p-1.5 rounded hover:bg-muted" title="复制">
                          <Copy className="w-3.5 h-3.5" />
                        </button>
                        {strategy.status === 'active' ? (
                          <button onClick={() => updateStatus(strategy.id, 'paused')} className="p-1.5 rounded hover:bg-muted text-yellow-500" title="暂停">
                            <Pause className="w-3.5 h-3.5" />
                          </button>
                        ) : (
                          <button onClick={() => updateStatus(strategy.id, 'active')} className="p-1.5 rounded hover:bg-muted text-green-500" title="启动">
                            <Play className="w-3.5 h-3.5" />
                          </button>
                        )}
                        <button onClick={() => deleteStrategy(strategy.id)} className="p-1.5 rounded hover:bg-muted text-red-500" title="删除">
                          <Trash2 className="w-3.5 h-3.5" />
                        </button>
                      </div>
                    </div>

                    <div className="mt-2 flex flex-wrap gap-2">
                      {Object.entries(strategy.params).map(([key, value]) => (
                        <Badge key={key} variant="outline" className="text-xs">
                          {key}: {Array.isArray(value) ? value.join(', ') : String(value)}
                        </Badge>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            ))}
          </div>
        </div>
      )}

      {/* 策略分析卡片 */}
      <div>
        <h2 className="text-lg font-semibold mb-4">策略分析</h2>
        {strategies.length === 0 ? (
          <div className="flex flex-col items-center justify-center p-12 rounded-lg border border-dashed">
            <Activity className="w-12 h-12 text-muted-foreground mb-4" />
            <h3 className="text-lg font-medium mb-2">暂无策略实例</h3>
            <p className="text-sm text-muted-foreground text-center max-w-md">
              点击上方"策略配置"按钮创建策略实例
            </p>
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {strategies.map(strategy => {
              const analysis = analyses[strategy.id];
              const isActive = strategy.status === 'active';

              return (
                <div
                  key={strategy.id}
                  className={`p-5 rounded-lg border transition-all hover:shadow-md ${
                    isActive ? 'bg-background' : 'bg-muted/50 opacity-70'
                  }`}
                >
                  <div className="flex items-start justify-between mb-4">
                    <div>
                      <h3 className="font-semibold text-base">
                        {strategy.display_name || strategy.strategy_type.toUpperCase()}
                      </h3>
                      <p className="text-xs text-muted-foreground mt-1">
                        {strategy.symbols.join(', ')} • {strategy.exchange.toUpperCase()}
                      </p>
                    </div>
                    <Badge variant={isActive ? 'default' : 'secondary'} className="text-xs">
                      {isActive ? '运行中' : '已暂停'}
                    </Badge>
                  </div>

                  {analysis ? (
                    <>
                      <div className="flex items-center gap-2 mb-3 p-2 rounded-md bg-muted/50">
                        {getStructureIcon(analysis.market_structure.structure_type)}
                        <div>
                          <div className="text-sm font-medium">{analysis.market_structure.description}</div>
                          <div className="text-xs text-muted-foreground">
                            置信度 {analysis.market_structure.confidence.toFixed(0)}%
                          </div>
                        </div>
                      </div>

                      <div className={`flex items-center justify-between p-3 rounded-md border ${getDirectionColor(analysis.bias.direction)}`}>
                        <div className="flex items-center gap-2">
                          {getDirectionIcon(analysis.bias.direction)}
                          <span className="font-medium">
                            {analysis.bias.direction === 'long' ? '做多' :
                             analysis.bias.direction === 'short' ? '做空' : '中性'}
                          </span>
                        </div>
                        <span className="text-sm font-mono">{analysis.bias.confidence.toFixed(0)}%</span>
                      </div>

                      {analysis.key_levels.support.length > 0 && (
                        <div className="mt-3 space-y-2">
                          <div className="flex items-center gap-2 text-xs text-muted-foreground">
                            <Shield className="w-3 h-3" />
                            <span>支撑位</span>
                          </div>
                          <div className="flex flex-wrap gap-2">
                            {analysis.key_levels.support.slice(0, 3).map((price, i) => (
                              <Badge key={i} variant="outline" className="font-mono text-xs text-green-500">
                                ${price.toLocaleString()}
                              </Badge>
                            ))}
                          </div>
                        </div>
                      )}

                      {analysis.key_levels.resistance.length > 0 && (
                        <div className="mt-2 space-y-2">
                          <div className="flex items-center gap-2 text-xs text-muted-foreground">
                            <Target className="w-3 h-3" />
                            <span>阻力位</span>
                          </div>
                          <div className="flex flex-wrap gap-2">
                            {analysis.key_levels.resistance.slice(0, 3).map((price, i) => (
                              <Badge key={i} variant="outline" className="font-mono text-xs text-red-500">
                                ${price.toLocaleString()}
                              </Badge>
                            ))}
                          </div>
                        </div>
                      )}

                      {analysis.trade_setup && (
                        <div className="mt-3 p-2 rounded-md bg-background border text-xs space-y-1">
                          <div className="flex justify-between">
                            <span className="text-muted-foreground">入场区间</span>
                            <span className="font-mono">
                              ${analysis.trade_setup.entry_zone[0].toLocaleString()} - ${analysis.trade_setup.entry_zone[1].toLocaleString()}
                            </span>
                          </div>
                          <div className="flex justify-between">
                            <span className="text-muted-foreground">止损</span>
                            <span className="font-mono text-red-500">
                              ${analysis.trade_setup.stop_loss.toLocaleString()}
                            </span>
                          </div>
                          <div className="flex justify-between">
                            <span className="text-muted-foreground">风险收益比</span>
                            <span className="font-mono">1:{analysis.trade_setup.risk_reward.toFixed(2)}</span>
                          </div>
                        </div>
                      )}
                    </>
                  ) : (
                    <div className="flex items-center justify-center h-32 text-muted-foreground">
                      <RefreshCw className={`w-5 h-5 ${loading ? 'animate-spin' : ''}`} />
                      <span className="ml-2">加载中...</span>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
