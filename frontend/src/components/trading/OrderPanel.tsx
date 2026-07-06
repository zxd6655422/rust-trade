'use client';

import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { useLanguage } from '@/lib/i18n/context';
import { useToast } from '@/components/ui/toast';
import {
  ArrowUpCircle,
  ArrowDownCircle,
  Loader2,
  AlertTriangle,
  Settings2,
} from 'lucide-react';

// Binance 合约订单类型
type BinanceOrderType = 'LIMIT' | 'MARKET' | 'STOP_MARKET' | 'TAKE_PROFIT_MARKET' | 'TRAILING_STOP_MARKET';

// OKX 订单类型
type OkxOrderType = 'market' | 'limit' | 'post_only' | 'fok' | 'ioc';

// 通用订单类型
type OrderType = 'market' | 'limit' | 'stop_loss' | 'take_profit';

// 仓位方向（Binance 合约）
type PositionSide = 'LONG' | 'SHORT' | 'BOTH';

// 保证金模式
type MarginMode = 'isolated' | 'cross';

// 有效期
type TimeInForce = 'GTC' | 'IOC' | 'FOK';

interface OrderPanelProps {
  symbol: string;
  /** 市场类型 */
  marketType?: 'spot' | 'futures';
  /** 交易所 */
  exchange?: 'binance' | 'okx';
  currentPrice?: string;
  onOrderPlaced?: () => void;
}

export default function OrderPanel({
  symbol,
  marketType = 'futures',
  exchange = 'binance',
  currentPrice,
  onOrderPlaced
}: OrderPanelProps) {
  const { t } = useLanguage();
  const { success, error: showError } = useToast();

  const [side, setSide] = useState<'buy' | 'sell'>('buy');
  const [orderType, setOrderType] = useState<OrderType>('market');
  const [quantity, setQuantity] = useState('');
  const [price, setPrice] = useState('');
  const [stopPrice, setStopPrice] = useState('');
  const [loading, setLoading] = useState(false);
  const [showAdvanced, setShowAdvanced] = useState(false);

  // 合约特有参数
  const [positionSide, setPositionSide] = useState<PositionSide>('LONG');
  const [marginMode, setMarginMode] = useState<MarginMode>('cross');
  const [leverage, setLeverage] = useState('10');
  const [timeInForce, setTimeInForce] = useState<TimeInForce>('GTC');
  const [reduceOnly, setReduceOnly] = useState(false);

  // 计算预估成本
  const estimatedCost = quantity && (price || currentPrice)
    ? (parseFloat(quantity) * parseFloat(price || currentPrice || '0')).toFixed(2)
    : null;

  // 构建订单参数
  const buildOrderParams = () => {
    const baseParams: Record<string, any> = {
      symbol,
      side: side.toUpperCase(),
    };

    if (exchange === 'binance') {
      // Binance 参数
      if (marketType === 'futures') {
        // 合约订单
        const binanceOrderType = mapToBinanceOrderType(orderType);
        return {
          ...baseParams,
          order_type: binanceOrderType,
          position_side: positionSide,
          margin_mode: marginMode,
          leverage: parseInt(leverage),
          quantity: parseFloat(quantity),
          price: orderType === 'limit' ? parseFloat(price) : null,
          stop_price: ['stop_loss', 'take_profit'].includes(orderType) ? parseFloat(stopPrice) : null,
          time_in_force: orderType === 'limit' ? timeInForce : null,
          reduce_only: reduceOnly,
          exchange: 'binance',
          market_type: 'futures',
        };
      } else {
        // 现货订单
        return {
          ...baseParams,
          order_type: orderType === 'market' ? 'MARKET' : 'LIMIT',
          quantity: parseFloat(quantity),
          price: orderType === 'limit' ? parseFloat(price) : null,
          exchange: 'binance',
          market_type: 'spot',
        };
      }
    } else {
      // OKX 参数
      const okxOrderType = mapToOkxOrderType(orderType);
      return {
        ...baseParams,
        inst_id: symbol.replace('USDT', '-USDT'), // OKX 格式: BTC-USDT
        td_mode: marginMode,
        ord_type: okxOrderType,
        sz: quantity,
        px: orderType === 'limit' ? price : null,
        exchange: 'okx',
        market_type: marketType,
      };
    }
  };

  // 映射到 Binance 订单类型
  const mapToBinanceOrderType = (type: OrderType): BinanceOrderType => {
    switch (type) {
      case 'market': return 'MARKET';
      case 'limit': return 'LIMIT';
      case 'stop_loss': return 'STOP_MARKET';
      case 'take_profit': return 'TAKE_PROFIT_MARKET';
    }
  };

  // 映射到 OKX 订单类型
  const mapToOkxOrderType = (type: OrderType): OkxOrderType => {
    switch (type) {
      case 'market': return 'market';
      case 'limit': return 'limit';
      case 'stop_loss': return 'market'; // OKX 用 algo order 处理止损
      case 'take_profit': return 'market'; // OKX 用 algo order 处理止盈
    }
  };

  const handleSubmit = async () => {
    if (!quantity || parseFloat(quantity) <= 0) {
      showError('请输入有效的数量');
      return;
    }

    if (orderType === 'limit' && (!price || parseFloat(price) <= 0)) {
      showError('请输入有效的价格');
      return;
    }

    if (['stop_loss', 'take_profit'].includes(orderType) && (!stopPrice || parseFloat(stopPrice) <= 0)) {
      showError('请输入有效的触发价格');
      return;
    }

    setLoading(true);
    try {
      const params = buildOrderParams();
      await invoke('place_order', { request: params });

      const orderTypeLabel = {
        market: '市价',
        limit: '限价',
        stop_loss: '止损',
        take_profit: '止盈',
      }[orderType];

      success(
        '下单成功',
        `${side === 'buy' ? '买入' : '卖出'} ${quantity} ${symbol} (${orderTypeLabel})`
      );

      // 重置表单
      setQuantity('');
      setPrice('');
      setStopPrice('');

      // 通知父组件刷新数据
      onOrderPlaced?.();
    } catch (err) {
      showError('下单失败', String(err));
    } finally {
      setLoading(false);
    }
  };

  const isFutures = marketType === 'futures';

  return (
    <Card>
      <CardHeader className="pb-3">
        <CardTitle className="text-lg flex items-center justify-between">
          <span>下单面板</span>
          <div className="flex items-center gap-2">
            <Badge variant="outline">{symbol}</Badge>
            <Badge variant="secondary" className="text-xs">
              {exchange.toUpperCase()} · {isFutures ? '合约' : '现货'}
            </Badge>
          </div>
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        {/* 买卖方向选择 */}
        <div className="grid grid-cols-2 gap-2">
          <Button
            variant={side === 'buy' ? 'default' : 'outline'}
            className={side === 'buy' ? 'bg-emerald-600 hover:bg-emerald-700' : ''}
            onClick={() => setSide('buy')}
          >
            <ArrowUpCircle className="w-4 h-4 mr-1" />
            买入{isFutures ? '/开多' : ''}
          </Button>
          <Button
            variant={side === 'sell' ? 'default' : 'outline'}
            className={side === 'sell' ? 'bg-red-600 hover:bg-red-700' : ''}
            onClick={() => setSide('sell')}
          >
            <ArrowDownCircle className="w-4 h-4 mr-1" />
            卖出{isFutures ? '/开空' : ''}
          </Button>
        </div>

        {/* 订单类型选择 */}
        <div className="grid grid-cols-4 gap-1">
          {(['market', 'limit', 'stop_loss', 'take_profit'] as OrderType[]).map((type) => (
            <Button
              key={type}
              variant={orderType === type ? 'secondary' : 'ghost'}
              size="sm"
              className="text-xs"
              onClick={() => setOrderType(type)}
            >
              {type === 'market' && '市价'}
              {type === 'limit' && '限价'}
              {type === 'stop_loss' && '止损'}
              {type === 'take_profit' && '止盈'}
            </Button>
          ))}
        </div>

        {/* 合约特有参数 */}
        {isFutures && exchange === 'binance' && (
          <>
            {/* 仓位方向 */}
            <div className="space-y-1">
              <label className="text-sm font-medium">仓位方向</label>
              <div className="grid grid-cols-2 gap-2">
                <Button
                  variant={positionSide === 'LONG' ? 'default' : 'outline'}
                  size="sm"
                  className={positionSide === 'LONG' ? 'bg-emerald-600' : ''}
                  onClick={() => setPositionSide('LONG')}
                >
                  多仓 (LONG)
                </Button>
                <Button
                  variant={positionSide === 'SHORT' ? 'default' : 'outline'}
                  size="sm"
                  className={positionSide === 'SHORT' ? 'bg-red-600' : ''}
                  onClick={() => setPositionSide('SHORT')}
                >
                  空仓 (SHORT)
                </Button>
              </div>
            </div>

            {/* 保证金模式和杠杆 */}
            <div className="grid grid-cols-2 gap-2">
              <div className="space-y-1">
                <label className="text-sm font-medium">保证金模式</label>
                <select
                  value={marginMode}
                  onChange={(e) => setMarginMode(e.target.value as MarginMode)}
                  className="w-full h-9 px-3 text-sm border rounded-md bg-background"
                >
                  <option value="cross">全仓</option>
                  <option value="isolated">逐仓</option>
                </select>
              </div>
              <div className="space-y-1">
                <label className="text-sm font-medium">杠杆倍数</label>
                <select
                  value={leverage}
                  onChange={(e) => setLeverage(e.target.value)}
                  className="w-full h-9 px-3 text-sm border rounded-md bg-background"
                >
                  {[1, 2, 3, 5, 10, 20, 25, 50, 75, 100, 125].map((l) => (
                    <option key={l} value={l}>{l}x</option>
                  ))}
                </select>
              </div>
            </div>
          </>
        )}

        {/* 数量输入 */}
        <div className="space-y-1">
          <div className="flex items-center justify-between">
            <label className="text-sm font-medium">数量</label>
            {isFutures && (
              <span className="text-xs text-muted-foreground">
                (张)
              </span>
            )}
          </div>
          <Input
            type="number"
            value={quantity}
            onChange={(e) => setQuantity(e.target.value)}
            placeholder={isFutures ? '1' : '0.001'}
            step={isFutures ? '1' : '0.001'}
            min="0"
          />
        </div>

        {/* 价格输入（限价单） */}
        {orderType === 'limit' && (
          <div className="space-y-1">
            <label className="text-sm font-medium">价格</label>
            <Input
              type="number"
              value={price}
              onChange={(e) => setPrice(e.target.value)}
              placeholder={currentPrice || '0.00'}
              step="0.01"
              min="0"
            />
          </div>
        )}

        {/* 触发价格（止损/止盈单） */}
        {['stop_loss', 'take_profit'].includes(orderType) && (
          <div className="space-y-1">
            <label className="text-sm font-medium">触发价格</label>
            <Input
              type="number"
              value={stopPrice}
              onChange={(e) => setStopPrice(e.target.value)}
              placeholder={currentPrice || '0.00'}
              step="0.01"
              min="0"
            />
          </div>
        )}

        {/* 高级选项 */}
        <div>
          <Button
            variant="ghost"
            size="sm"
            className="text-xs text-muted-foreground"
            onClick={() => setShowAdvanced(!showAdvanced)}
          >
            <Settings2 className="w-3 h-3 mr-1" />
            {showAdvanced ? '隐藏高级选项' : '高级选项'}
          </Button>

          {showAdvanced && (
            <div className="mt-2 space-y-2 p-2 rounded-md bg-muted/30">
              {/* 有效期（限价单） */}
              {orderType === 'limit' && (
                <div className="space-y-1">
                  <label className="text-xs font-medium text-muted-foreground">有效期</label>
                  <div className="grid grid-cols-3 gap-1">
                    {(['GTC', 'IOC', 'FOK'] as TimeInForce[]).map((tif) => (
                      <Button
                        key={tif}
                        variant={timeInForce === tif ? 'secondary' : 'ghost'}
                        size="sm"
                        className="text-xs"
                        onClick={() => setTimeInForce(tif)}
                      >
                        {tif}
                      </Button>
                    ))}
                  </div>
                  <p className="text-[10px] text-muted-foreground">
                    {timeInForce === 'GTC' && '撤销前有效'}
                    {timeInForce === 'IOC' && '立即成交并取消剩余'}
                    {timeInForce === 'FOK' && '全部成交或立即取消'}
                  </p>
                </div>
              )}

              {/* 仅减仓（合约） */}
              {isFutures && (
                <div className="flex items-center gap-2">
                  <input
                    type="checkbox"
                    id="reduceOnly"
                    checked={reduceOnly}
                    onChange={(e) => setReduceOnly(e.target.checked)}
                    className="rounded"
                  />
                  <label htmlFor="reduceOnly" className="text-xs">
                    仅减仓 (Reduce Only)
                  </label>
                </div>
              )}
            </div>
          )}
        </div>

        {/* 当前价格显示 */}
        {currentPrice && (
          <div className="flex justify-between text-sm text-muted-foreground">
            <span>当前价格</span>
            <span className="font-mono">${parseFloat(currentPrice).toLocaleString('en-US', { minimumFractionDigits: 2 })}</span>
          </div>
        )}

        {/* 预估成本 */}
        {estimatedCost && (
          <div className="flex justify-between text-sm">
            <span className="text-muted-foreground">预估成本</span>
            <span className="font-medium">${parseFloat(estimatedCost).toLocaleString('en-US', { minimumFractionDigits: 2 })}</span>
          </div>
        )}

        {/* 风险提示 */}
        {orderType === 'market' && (
          <div className="flex items-start gap-2 p-2 rounded-md bg-yellow-50 dark:bg-yellow-950/30 text-yellow-800 dark:text-yellow-200 text-xs">
            <AlertTriangle className="w-3.5 h-3.5 mt-0.5 flex-shrink-0" />
            <span>市价单将立即以当前市场价格成交，可能存在滑点</span>
          </div>
        )}

        {/* 下单按钮 */}
        <Button
          className={`w-full ${
            side === 'buy'
              ? 'bg-emerald-600 hover:bg-emerald-700'
              : 'bg-red-600 hover:bg-red-700'
          }`}
          onClick={handleSubmit}
          disabled={loading || !quantity}
        >
          {loading ? (
            <Loader2 className="w-4 h-4 mr-2 animate-spin" />
          ) : side === 'buy' ? (
            <ArrowUpCircle className="w-4 h-4 mr-2" />
          ) : (
            <ArrowDownCircle className="w-4 h-4 mr-2" />
          )}
          {loading ? '下单中...' : `${side === 'buy' ? '买入' : '卖出'} ${symbol}`}
        </Button>
      </CardContent>
    </Card>
  );
}
