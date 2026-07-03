'use client';

import { useState } from 'react';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { Badge } from '@/components/ui/badge';
import {
  Activity, LineChart, FlaskConical, Zap,
  CircleDot, Layers
} from 'lucide-react';
import { useLanguage } from '@/lib/i18n/context';

// 子组件
import PriceTicker from '@/components/trading/PriceTicker';
import AccountProfitDashboard from '@/components/trading/AccountProfitDashboard';
import KlineChart from '@/components/trading/KlineChart';
import PositionTable from '@/components/trading/PositionTable';
import TradeHistory from '@/components/trading/TradeHistory';
import PnlSummaryCards from '@/components/trading/PnlSummaryCards';
import EquityCurve from '@/components/trading/EquityCurve';
import PerformancePanel from '@/components/trading/PerformancePanel';
import CommissionStats from '@/components/trading/CommissionStats';
import StrategyWinRate from '@/components/trading/StrategyWinRate';

// 回测页面内容 (内联导入)
import BacktestContent from './BacktestContent';

type MarketType = 'spot' | 'futures';

export default function TradingPage() {
  const [selectedSymbol, setSelectedSymbol] = useState('BTCUSDT');
  const [marketType, setMarketType] = useState<MarketType>('futures');
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
        </TabsList>

        {/* ============ Live Trading Tab ============ */}
        <TabsContent value="live" className="space-y-6">
          {/* Market Type Sub-Tabs */}
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

          {/* Price Ticker */}
          <PriceTicker
            onSymbolSelect={setSelectedSymbol}
            selectedSymbol={selectedSymbol}
          />

          {/* Account Profit Dashboard - 醒目位置 */}
          <AccountProfitDashboard symbol={selectedSymbol} />

          {/* K Line Chart */}
          <KlineChart symbol={selectedSymbol} />

          {/* Positions + PnL Summary Row */}
          <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
            <div className="lg:col-span-2">
              <PositionTable />
            </div>
            <div>
              <PnlSummaryCards symbol={selectedSymbol} days={30} />
            </div>
          </div>

          {/* Strategy Win Rate */}
          <StrategyWinRate symbol={selectedSymbol} />

          {/* Equity Curve + Performance Row */}
          <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
            <EquityCurve symbol={selectedSymbol} days={90} />
            <PerformancePanel symbol={selectedSymbol} days={30} />
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
          <div className="flex flex-col items-center justify-center py-20 text-muted-foreground">
            <FlaskConical className="w-16 h-16 mb-4 opacity-30" />
            <h2 className="text-xl font-semibold mb-2">{t.trading.paperTrading}</h2>
            <p className="text-sm max-w-md text-center">
              {t.trading.paperTradingDesc}
            </p>
            <Badge variant="outline" className="mt-4">{t.common.comingSoon}</Badge>
          </div>
        </TabsContent>
      </Tabs>
    </div>
  );
}
