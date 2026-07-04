'use client';

import { useState, useEffect, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { useLanguage } from '@/lib/i18n/context';
import { useToast } from '@/components/ui/toast';
import {
  Bell,
  BellRing,
  Plus,
  Trash2,
  ArrowUp,
  ArrowDown,
  AlertCircle,
} from 'lucide-react';

interface PriceAlert {
  id: string;
  symbol: string;
  condition: 'above' | 'below';
  price: number;
  triggered: boolean;
  createdAt: Date;
}

interface PriceAlertsProps {
  symbol: string;
  currentPrice?: number;
}

export default function PriceAlerts({ symbol, currentPrice }: PriceAlertsProps) {
  const { success, warning } = useToast();

  const [alerts, setAlerts] = useState<PriceAlert[]>([]);
  const [showAdd, setShowAdd] = useState(false);
  const [newCondition, setNewCondition] = useState<'above' | 'below'>('above');
  const [newPrice, setNewPrice] = useState('');

  // 从 localStorage 加载告警
  useEffect(() => {
    const saved = localStorage.getItem('price_alerts');
    if (saved) {
      try {
        const parsed = JSON.parse(saved);
        setAlerts(parsed.map((a: PriceAlert) => ({
          ...a,
          createdAt: new Date(a.createdAt),
        })));
      } catch {}
    }
  }, []);

  // 保存告警到 localStorage
  const saveAlerts = useCallback((newAlerts: PriceAlert[]) => {
    setAlerts(newAlerts);
    localStorage.setItem('price_alerts', JSON.stringify(newAlerts));
  }, []);

  // 检查告警触发
  useEffect(() => {
    if (!currentPrice) return;

    const updatedAlerts = alerts.map((alert) => {
      if (alert.triggered || alert.symbol !== symbol) return alert;

      const shouldTrigger =
        (alert.condition === 'above' && currentPrice >= alert.price) ||
        (alert.condition === 'below' && currentPrice <= alert.price);

      if (shouldTrigger) {
        warning(
          '价格告警触发',
          `${alert.symbol} 价格已${alert.condition === 'above' ? '突破' : '跌破'} $${alert.price.toLocaleString()}`
        );
        return { ...alert, triggered: true };
      }

      return alert;
    });

    const hasChanges = updatedAlerts.some((a, i) => a.triggered !== alerts[i].triggered);
    if (hasChanges) {
      saveAlerts(updatedAlerts);
    }
  }, [currentPrice, alerts, symbol, warning, saveAlerts]);

  const addAlert = () => {
    const price = parseFloat(newPrice);
    if (!price || price <= 0) return;

    const newAlert: PriceAlert = {
      id: Math.random().toString(36).substring(2, 9),
      symbol,
      condition: newCondition,
      price,
      triggered: false,
      createdAt: new Date(),
    };

    saveAlerts([...alerts, newAlert]);
    setNewPrice('');
    setShowAdd(false);
    success('告警已添加', `${symbol} ${newCondition === 'above' ? '突破' : '跌破'} $${price.toLocaleString()}`);
  };

  const removeAlert = (id: string) => {
    saveAlerts(alerts.filter((a) => a.id !== id));
  };

  const resetAlert = (id: string) => {
    saveAlerts(alerts.map((a) =>
      a.id === id ? { ...a, triggered: false } : a
    ));
  };

  const symbolAlerts = alerts.filter((a) => a.symbol === symbol);

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Bell className="w-5 h-5 text-muted-foreground" />
            <CardTitle className="text-lg">价格告警</CardTitle>
            {symbolAlerts.length > 0 && (
              <Badge variant="secondary" className="text-xs">
                {symbolAlerts.filter((a) => !a.triggered).length} 个活跃
              </Badge>
            )}
          </div>
          <Button
            variant={showAdd ? 'secondary' : 'ghost'}
            size="sm"
            onClick={() => setShowAdd(!showAdd)}
          >
            <Plus className="w-3 h-3" />
          </Button>
        </div>
      </CardHeader>
      <CardContent className="space-y-3">
        {/* 添加告警表单 */}
        {showAdd && (
          <div className="p-3 rounded-lg border space-y-3">
            <div className="grid grid-cols-2 gap-2">
              <Button
                variant={newCondition === 'above' ? 'default' : 'outline'}
                size="sm"
                onClick={() => setNewCondition('above')}
                className={newCondition === 'above' ? 'bg-emerald-600' : ''}
              >
                <ArrowUp className="w-3 h-3 mr-1" />
                突破
              </Button>
              <Button
                variant={newCondition === 'below' ? 'default' : 'outline'}
                size="sm"
                onClick={() => setNewCondition('below')}
                className={newCondition === 'below' ? 'bg-red-600' : ''}
              >
                <ArrowDown className="w-3 h-3 mr-1" />
                跌破
              </Button>
            </div>
            <div className="flex gap-2">
              <Input
                type="number"
                value={newPrice}
                onChange={(e) => setNewPrice(e.target.value)}
                placeholder={currentPrice?.toLocaleString() || '输入价格'}
                step="0.01"
                min="0"
              />
              <Button onClick={addAlert} disabled={!newPrice}>
                添加
              </Button>
            </div>
            {currentPrice && (
              <p className="text-xs text-muted-foreground">
                当前价格: ${currentPrice.toLocaleString('en-US', { minimumFractionDigits: 2 })}
              </p>
            )}
          </div>
        )}

        {/* 告警列表 */}
        {symbolAlerts.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-6 text-muted-foreground">
            <Bell className="w-8 h-8 mb-2 opacity-50" />
            <p className="text-sm">暂无价格告警</p>
            <p className="text-xs mt-1">点击 + 添加告警</p>
          </div>
        ) : (
          <div className="space-y-2">
            {symbolAlerts.map((alert) => (
              <div
                key={alert.id}
                className={`flex items-center justify-between p-2.5 rounded-lg border ${
                  alert.triggered
                    ? 'bg-yellow-50 dark:bg-yellow-950/30 border-yellow-200 dark:border-yellow-800'
                    : 'hover:bg-muted/50'
                }`}
              >
                <div className="flex items-center gap-2">
                  {alert.triggered ? (
                    <BellRing className="w-4 h-4 text-yellow-500" />
                  ) : alert.condition === 'above' ? (
                    <ArrowUp className="w-4 h-4 text-emerald-500" />
                  ) : (
                    <ArrowDown className="w-4 h-4 text-red-500" />
                  )}
                  <div>
                    <p className="text-sm font-medium">
                      {alert.condition === 'above' ? '突破' : '跌破'}{' '}
                      <span className="font-mono">${alert.price.toLocaleString()}</span>
                    </p>
                    {alert.triggered && (
                      <p className="text-xs text-yellow-600 dark:text-yellow-400">已触发</p>
                    )}
                  </div>
                </div>
                <div className="flex gap-1">
                  {alert.triggered && (
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => resetAlert(alert.id)}
                      title="重置告警"
                    >
                      <AlertCircle className="w-3 h-3" />
                    </Button>
                  )}
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => removeAlert(alert.id)}
                    title="删除告警"
                  >
                    <Trash2 className="w-3 h-3 text-red-500" />
                  </Button>
                </div>
              </div>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
