'use client';

import { useState } from 'react';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { Badge } from '@/components/ui/badge';
import {
  Activity, LineChart, FlaskConical, Zap,
  CircleDot, Layers, TrendingUp, Settings2, Database,
  Wrench
} from 'lucide-react';
import { useLanguage } from '@/lib/i18n/context';

// 子组件
import PriceTicker from '@/components/trading/PriceTicker';
import AccountProfitDashboard from '@/components/trading/AccountProfitDashboard';
import AutoTradingStatus from '@/components/trading/AutoTradingStatus';
import KlineChart from '@/components/trading/KlineChart';
import PositionTable from '@/components/trading/PositionTable';
import TradeHistory from '@/components/trading/TradeHistory';
import EquityCurve from '@/components/trading/EquityCurve';
import CommissionStats from '@/components/trading/CommissionStats';
import StrategyWinRate from '@/components/trading/StrategyWinRate';
import OrderPanel from '@/components/trading/OrderPanel';
import PriceAlerts from '@/components/trading/PriceAlerts';
import StrategyAnalysisPanel from '@/components/trading/StrategyAnalysisPanel';
import SignalHistory from '@/components/trading/SignalHistory';
import DataManager from '@/components/trading/DataManager';

// 子页面内容 (内联导入)
import BacktestContent from './BacktestContent';
import AdvancedBacktestContent from './AdvancedBacktestContent';
import PaperTradingContent from './PaperTradingContent';

type MarketType = 'spot' | 'futures';
type Exchange = 'binance' | 'okx';

export default function TradingPage() {
  const [symbols, setSymbols] = useState<string[]>([]);
  const [selectedSymbol, setSelectedSymbol] = useState('BTCUSDT');
  const [marketType, setMarketType] = useState<MarketType>('futures');
  const [exchange, setExchange] = useState<Exchange>('binance');
  const [showDataManager, setShowDataManager] = useState(false);
  const [showDebugOrder, setShowDebugOrder] = useState(false);
  const { t } = useLanguage();

  return (
    <div className="space-y-6">
      {/* Page Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">{t.trading.title}</h1>
          <p className="text-sm text-muted-foreground mt-1">
            {t.trading.subtitle}
          </p>
        </div>
        <Badge variant="outline" className="flex items-center gap-1.5">
          <Activity className="w-3 h-3" />
          {t.trading.liveData}
        </Badge>
      </div>

      {/* Main Tabs */}
      <Tabs defaultValue="live" className="space-y-6">
        <TabsList className="h-11 w-full justify-start gap-1 bg-muted/50 p-1">
          <TabsTrigger value="live" className="gap-2 px-4">
            <Zap className="w-4 h-4" />
            {t.trading.liveTrading}
          </TabsTrigger>
          <TabsTrigger value="backtest" className="gap-2 px-4">
            <LineChart className="w-4 h-4" />
            {t.trading.backtest}
          </TabsTrigger>
          <TabsTrigger value="paper" className="gap-2 px-4">
            <FlaskConical className="w-4 h-4" />
            {t.trading.paperTrading}
          </TabsTrigger>
          <TabsTrigger value="advanced" className="gap-2 px-4">
            <TrendingUp className="w-4 h-4" />
            {t.trading.advancedBacktest}
          </TabsTrigger>
        </TabsList>

        {/* ============ Live Trading Tab ============ */}
        <TabsContent value="live" className="space-y-6">
          {/* 交易所切换 */}
          <div className="flex items-center gap-4">
            <div className="flex bg-muted rounded-lg p-1">
              <button
                onClick={() => setExchange('binance')}
                className={`flex items-center gap-2 px-4 py-2 rounded-md text-sm font-medium transition-all ${
                  exchange === 'binance'
                    ? 'bg-background text-foreground shadow-sm'
                    : 'text-muted-foreground hover:text-foreground'
                }`}
              >
                <img src="/binance.svg" alt="Binance" className="w-4 h-4" onError={(e) => {
                  e.currentTarget.style.display = 'none';
                }} />
                Binance
              </button>
              <button
                onClick={() => setExchange('okx')}
                className={`flex items-center gap-2 px-4 py-2 rounded-md text-sm font-medium transition-all ${
                  exchange === 'okx'
                    ? 'bg-background text-foreground shadow-sm'
                    : 'text-muted-foreground hover:text-foreground'
                }`}
              >
                <img src="/okx.svg" alt="OKX" className="w-4 h-4" onError={(e) => {
                  e.currentTarget.style.display = 'none';
                }} />
                OKX
              </button>
            </div>
            <Badge variant="secondary" className="text-xs">
              {exchange.toUpperCase()}
            </Badge>
          </div>

          {/* Account Profit Dashboard - 醒目位置 */}
          <AccountProfitDashboard symbol={selectedSymbol} exchange={exchange} />

          {/* Data Manager Toggle */}
          <div className="flex items-center gap-2">
            <button
              onClick={() => setShowDataManager(!showDataManager)}
              className={`flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-medium transition-all border ${
                showDataManager ? 'bg-primary text-primary-foreground' : 'bg-muted text-muted-foreground hover:text-foreground'
              }`}
            >
              <Database className="w-3.5 h-3.5" />
              交易对管理
            </button>
          </div>

          {showDataManager && (
            <DataManager onSymbolsChange={(s) => {
              setSymbols(s);
              if (s.length > 0 && !s.includes(selectedSymbol)) {
                setSelectedSymbol(s[0]);
              }
            }} />
          )}

          <PriceTicker
            symbols={symbols}
            onSymbolSelect={setSelectedSymbol}
            selectedSymbol={selectedSymbol}
          />

          {/* 自动交易状态 + K线图 */}
          <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
            <div className="lg:col-span-1">
              <AutoTradingStatus symbol={selectedSymbol} />
            </div>
            <div className="lg:col-span-2">
              <KlineChart symbol={selectedSymbol} marketType={marketType} exchange={exchange} autoRefreshInterval={30000} />
            </div>
          </div>

          {/* 现货/合约切换 */}
          <div className="flex items-center gap-4">
            <div className="flex bg-muted rounded-lg p-1">
              <button
                onClick={() => setMarketType('spot')}
                className={`flex items-center gap-2 px-4 py-2 rounded-md text-sm font-medium transition-all ${
                  marketType === 'spot'
                    ? 'bg-background text-foreground shadow-sm'
                    : 'text-muted-foreground hover:text-foreground'
                }`}
              >
                <CircleDot className="w-4 h-4" />
                {t.trading.spot}
              </button>
              <button
                onClick={() => setMarketType('futures')}
                className={`flex items-center gap-2 px-4 py-2 rounded-md text-sm font-medium transition-all ${
                  marketType === 'futures'
                    ? 'bg-background text-foreground shadow-sm'
                    : 'text-muted-foreground hover:text-foreground'
                }`}
              >
                <Layers className="w-4 h-4" />
                {t.trading.futures}
              </button>
            </div>
            <Badge variant="secondary" className="text-xs">
              {marketType === 'spot' ? t.trading.spotMarket : t.trading.futuresMarket}
            </Badge>
          </div>

          {/* Positions */}
          <PositionTable exchange={exchange} />

          {/* Strategy Analysis Panel */}
          <StrategyAnalysisPanel symbol={selectedSymbol} autoRefreshInterval={60000} />

          {/* 调试下单（折叠） */}
          <div>
            <button
              onClick={() => setShowDebugOrder(!showDebugOrder)}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-medium transition-all border bg-muted text-muted-foreground hover:text-foreground"
            >
              <Wrench className="w-3.5 h-3.5" />
              {showDebugOrder ? '隐藏调试下单' : '调试下单'}
            </button>
            {showDebugOrder && (
              <div className="mt-4">
                <OrderPanel symbol={selectedSymbol} marketType={marketType} exchange={exchange} />
              </div>
            )}
          </div>

          {/* Strategy Win Rate + Signal History */}
          <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
            <StrategyWinRate symbol={selectedSymbol} />
            <SignalHistory symbol={selectedSymbol} limit={30} />
          </div>

          {/* Equity Curve + Price Alerts */}
          <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
            <div className="lg:col-span-2">
              <EquityCurve symbol={selectedSymbol} days={90} />
            </div>
            <div>
              <PriceAlerts symbol={selectedSymbol} />
            </div>
          </div>

          {/* Commission Stats */}
          <CommissionStats symbol={selectedSymbol} days={30} />

          {/* Trade History */}
          <TradeHistory symbol={selectedSymbol} />
        </TabsContent>

        {/* ============ Backtest Tab ============ */}
        <TabsContent value="backtest">
          <BacktestContent />
        </TabsContent>

        {/* ============ Paper Trading Tab ============ */}
        <TabsContent value="paper">
          <PaperTradingContent />
        </TabsContent>

        {/* ============ Advanced Backtest Tab ============ */}
        <TabsContent value="advanced">
          <AdvancedBacktestContent />
        </TabsContent>
      </Tabs>
    </div>
  );
}
