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
} from 'lucide-react';

type OrderSide = 'buy' | 'sell';
type OrderType = 'market' | 'limit' | 'stop_loss' | 'take_profit';

interface OrderPanelProps {
  symbol: string;
  /** 市场类型 */
  marketType?: 'spot' | 'futures';
  currentPrice?: string;
  onOrderPlaced?: () => void;
}

export default function OrderPanel({ symbol, marketType = 'futures', currentPrice, onOrderPlaced }: OrderPanelProps) {
  const { t } = useLanguage();
  const { success, error: showError } = useToast();

  const [side, setSide] = useState<OrderSide>('buy');
  const [orderType, setOrderType] = useState<OrderType>('market');
  const [quantity, setQuantity] = useState('');
  const [price, setPrice] = useState('');
  const [stopPrice, setStopPrice] = useState('');
  const [loading, setLoading] = useState(false);

  // 计算预估成本
  const estimatedCost = quantity && (price || currentPrice)
    ? (parseFloat(quantity) * parseFloat(price || currentPrice || '0')).toFixed(2)
    : null;

  const handleSubmit = async () => {
    if (!quantity || parseFloat(quantity) <= 0) {
      showError('请输入有效的数量');
      return;
    }

    if (orderType !== 'market' && (!price || parseFloat(price) <= 0)) {
      showError('请输入有效的价格');
      return;
    }

    setLoading(true);
    try {
      // 调用 Tauri 命令下单（需要后端支持）
      await invoke('place_order', {
        request: {
          symbol,
          side: side.toUpperCase(),
          order_type: orderType.toUpperCase(),
          quantity: parseFloat(quantity),
          price: orderType !== 'market' ? parseFloat(price) : null,
          stop_price: orderType === 'stop_loss' || orderType === 'take_profit'
            ? parseFloat(stopPrice)
            : null,
        }
      });

      success('下单成功', `${side === 'buy' ? '买入' : '卖出'} ${quantity} ${symbol}`);

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

  return (
    <Card>
      <CardHeader className="pb-3">
        <CardTitle className="text-lg flex items-center justify-between">
          <span>下单面板</span>
          <Badge variant="outline">{symbol}</Badge>
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
            买入
          </Button>
          <Button
            variant={side === 'sell' ? 'default' : 'outline'}
            className={side === 'sell' ? 'bg-red-600 hover:bg-red-700' : ''}
            onClick={() => setSide('sell')}
          >
            <ArrowDownCircle className="w-4 h-4 mr-1" />
            卖出
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

        {/* 数量输入 */}
        <div className="space-y-1">
          <label className="text-sm font-medium">数量</label>
          <Input
            type="number"
            value={quantity}
            onChange={(e) => setQuantity(e.target.value)}
            placeholder="0.001"
            step="0.001"
            min="0"
          />
        </div>

        {/* 价格输入（限价单） */}
        {orderType !== 'market' && (
          <div className="space-y-1">
            <label className="text-sm font-medium">
              {orderType === 'limit' ? '限价' : '触发价格'}
            </label>
            <Input
              type="number"
              value={orderType === 'limit' ? price : stopPrice}
              onChange={(e) => {
                if (orderType === 'limit') {
                  setPrice(e.target.value);
                } else {
                  setStopPrice(e.target.value);
                }
              }}
              placeholder={currentPrice || '0.00'}
              step="0.01"
              min="0"
            />
          </div>
        )}

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
