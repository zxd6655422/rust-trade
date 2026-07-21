'use client';

import React, { useEffect, useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Loader2, DollarSign } from 'lucide-react';
import { useLanguage } from '@/lib/i18n/context';

// 资产余额项
interface AssetBalanceItem {
  asset: string;
  total: string;
  available: string;
  frozen: string;
}

// 估值后的资产项
interface AssetWithValue extends AssetBalanceItem {
  price: number;
  value: number;
}

// 从 localStorage 读取交易所配置
function getExchangeConfig(): { exchange: string } {
  try {
    const raw = localStorage.getItem('exchange_configs');
    if (raw) {
      const configs = JSON.parse(raw);
      if (configs.length > 0) return { exchange: configs[0].id };
    }
  } catch {}
  return { exchange: 'binance' };
}

// 从 localStorage 读取 trading-core 服务地址
function getServerUrl(): string {
  try {
    const saved = localStorage.getItem('server_config');
    if (saved) {
      const config = JSON.parse(saved);
      const protocol = config.protocol || 'http';
      const host = config.host || 'localhost';
      const port = config.port || 8080;
      return `${protocol}://${host}:${port}`;
    }
  } catch {}
  return 'http://localhost:8080';
}

const SpotBalances: React.FC = () => {
  const { t } = useLanguage();
  const [assets, setAssets] = useState<AssetWithValue[]>([]);
  const [loading, setLoading] = useState(true);
  const [totalValue, setTotalValue] = useState(0);

  const fetchData = useCallback(async () => {
    try {
      const { exchange } = getExchangeConfig();

      // 1. 获取现货余额
      const balances = await invoke<AssetBalanceItem[]>('get_asset_balances', {
        exchange,
        marketType: 'spot',
      });

      if (!balances || balances.length === 0) {
        setAssets([]);
        setTotalValue(0);
        setLoading(false);
        return;
      }

      // 2. 通过 trading-core 获取实时价格
      const assetNames = balances.map(b => b.asset);
      const serverUrl = getServerUrl();
      const prices = await invoke<Record<string, string>>('get_spot_prices', {
        assets: assetNames,
        serverUrl,
      });

      // 3. 计算估值
      let total = 0;
      const assetsWithValue: AssetWithValue[] = balances.map(b => {
        const price = parseFloat(prices[b.asset] || '0');
        const qty = parseFloat(b.total);
        const value = qty * price;
        total += value;
        return { ...b, price, value };
      });

      // 按估值降序排列
      assetsWithValue.sort((a, b) => b.value - a.value);

      setAssets(assetsWithValue);
      setTotalValue(total);
    } catch (err) {
      console.error('Failed to fetch spot balances:', err);
      setAssets([]);
      setTotalValue(0);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, 30000);
    return () => clearInterval(interval);
  }, [fetchData]);

  // 格式化数量（根据精度自动调整）
  const fmtQty = (v: string, asset: string) => {
    const n = parseFloat(v);
    if (isNaN(n)) return '--';
    // 稳定币保留2位，其他根据大小调整
    if (['USDT', 'USDC', 'BUSD', 'DAI', 'FDUSD'].includes(asset)) {
      return n.toFixed(2);
    }
    if (n >= 1) return n.toFixed(4);
    if (n >= 0.001) return n.toFixed(6);
    return n.toFixed(8);
  };

  // 格式化 USD 估值
  const fmtUsd = (v: number) => {
    if (v >= 1000) return `$${v.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
    if (v >= 1) return `$${v.toFixed(2)}`;
    if (v > 0) return `$${v.toFixed(4)}`;
    return '$0.00';
  };

  return (
    <div className="space-y-2">
      {/* 表头 */}
      <div className="grid grid-cols-[80px_1fr_1fr_1fr_1fr] gap-2 px-2 py-1 text-[10px] text-muted-foreground uppercase tracking-wider border-b">
        <span>{t.overview.asset}</span>
        <span className="text-right">{t.overview.totalAmount}</span>
        <span className="text-right">{t.overview.available}</span>
        <span className="text-right">{t.overview.frozen}</span>
        <span className="text-right">{t.overview.estimatedValue}</span>
      </div>

      {loading ? (
        <div className="flex items-center justify-center py-8 text-muted-foreground text-sm">
          <Loader2 className="w-4 h-4 animate-spin mr-2" />
          {t.common.loading}
        </div>
      ) : assets.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-8 text-muted-foreground">
          <DollarSign className="w-8 h-8 mb-2 opacity-50" />
          <p className="text-sm">{t.overview.noSpotAssets}</p>
          <p className="text-xs mt-1">{t.overview.noSpotAssetsDesc}</p>
        </div>
      ) : (
        <>
          {/* 资产行 */}
          {assets.map((asset) => (
            <div
              key={asset.asset}
              className="grid grid-cols-[80px_1fr_1fr_1fr_1fr] gap-2 items-center px-2 py-2 rounded-md hover:bg-muted/50 transition-colors border-b border-border/30 last:border-0"
            >
              {/* 币种 */}
              <span className="text-sm font-semibold font-mono truncate">
                {asset.asset}
              </span>

              {/* 总量 */}
              <span className="text-sm font-mono text-right">
                {fmtQty(asset.total, asset.asset)}
              </span>

              {/* 可用 */}
              <span className="text-sm font-mono text-right text-muted-foreground">
                {fmtQty(asset.available, asset.asset)}
              </span>

              {/* 冻结 */}
              <span className="text-sm font-mono text-right text-muted-foreground">
                {fmtQty(asset.frozen, asset.asset)}
              </span>

              {/* 估值 */}
              <span className="text-sm font-mono text-right font-medium">
                {fmtUsd(asset.value)}
              </span>
            </div>
          ))}

          {/* 总计 */}
          <div className="flex items-center justify-end px-2 pt-2 border-t">
            <span className="text-xs text-muted-foreground mr-2">{t.overview.spotTotal}:</span>
            <span className="text-sm font-bold">{fmtUsd(totalValue)}</span>
          </div>
        </>
      )}
    </div>
  );
};

export default SpotBalances;
