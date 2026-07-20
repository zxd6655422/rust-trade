'use client';

import React, { useEffect, useState, useCallback } from 'react';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Server, Database, Zap, Calendar, ArrowLeftRight, CheckCircle2, XCircle, Pause } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { useConnection } from '@/lib/connection-context';
import { useLanguage } from '@/lib/i18n/context';

interface SchedulerStatus {
  is_running: boolean;
  is_paused: boolean;
}

const SystemStatus: React.FC = () => {
  const { t } = useLanguage();
  const { tradingCore, database } = useConnection();
  const [scheduler, setScheduler] = useState<SchedulerStatus | null>(null);
  const [dataCoverage, setDataCoverage] = useState<number | null>(null);

  const fetchStatus = useCallback(async () => {
    try {
      const status = await invoke<SchedulerStatus>('get_scheduler_status');
      setScheduler(status);
    } catch (err) {
      console.error('Failed to fetch scheduler status:', err);
    }

    try {
      const info = await invoke<{ total_records: number; symbols_count: number }>('get_data_info');
      // Estimate coverage days from data (rough estimate)
      if (info.total_records > 0) {
        setDataCoverage(Math.floor(info.total_records / 86400)); // assuming ~1 record/sec
      }
    } catch (err) {
      console.error('Failed to fetch data info:', err);
    }
  }, []);

  useEffect(() => {
    fetchStatus();
    const interval = setInterval(fetchStatus, 30000);
    return () => clearInterval(interval);
  }, [fetchStatus]);

  const getTradingStatus = () => {
    if (!scheduler) return { label: t.overview.stopped, color: 'text-slate-400', icon: XCircle };
    if (scheduler.is_paused) return { label: t.overview.paused, color: 'text-amber-500', icon: Pause };
    if (scheduler.is_running) return { label: t.overview.running, color: 'text-emerald-500', icon: CheckCircle2 };
    return { label: t.overview.stopped, color: 'text-slate-400', icon: XCircle };
  };

  const tradingStatus = getTradingStatus();
  const TradingIcon = tradingStatus.icon;

  const coreConnected = tradingCore === 'connected';
  const dbConnected = database === 'connected';

  let exchange = 'Binance';
  try {
    const raw = typeof window !== 'undefined' ? localStorage.getItem('exchange_configs') : null;
    if (raw) {
      const configs = JSON.parse(raw);
      if (configs.length > 0) exchange = configs[0].name || 'Binance';
    }
  } catch {}

  const items = [
    {
      icon: Server,
      label: t.overview.tradingCore,
      status: coreConnected ? t.common.connected : t.common.disconnected,
      connected: coreConnected,
    },
    {
      icon: Database,
      label: t.overview.database,
      status: dbConnected ? t.common.connected : t.common.disconnected,
      connected: dbConnected,
    },
    {
      icon: Zap,
      label: t.overview.autoTrading,
      status: tradingStatus.label,
      connected: scheduler?.is_running ?? false,
      customIcon: <TradingIcon className={`w-3.5 h-3.5 ${tradingStatus.color}`} />,
    },
    {
      icon: Calendar,
      label: t.overview.dataCoverage,
      status: dataCoverage !== null ? `${dataCoverage} ${t.overview.days}` : '--',
      connected: dataCoverage !== null && dataCoverage > 0,
      hideIndicator: true,
    },
    {
      icon: ArrowLeftRight,
      label: t.overview.exchange,
      status: exchange,
      connected: true,
      hideIndicator: true,
    },
  ];

  return (
    <Card className="h-full">
      <CardHeader className="pb-3">
        <CardTitle className="text-sm font-medium flex items-center gap-2">
          <Server className="w-4 h-4" />
          {t.overview.systemStatus}
        </CardTitle>
      </CardHeader>
      <CardContent>
        <div className="space-y-3">
          {items.map((item) => {
            const Icon = item.icon;
            return (
              <div
                key={item.label}
                className="flex items-center justify-between py-1.5 border-b border-border/50 last:border-0"
              >
                <div className="flex items-center gap-2">
                  <Icon className="w-3.5 h-3.5 text-muted-foreground" />
                  <span className="text-sm">{item.label}</span>
                </div>
                <div className="flex items-center gap-2">
                  {item.customIcon ? (
                    item.customIcon
                  ) : !item.hideIndicator ? (
                    <div
                      className={`w-2 h-2 rounded-full ${
                        item.connected ? 'bg-emerald-500' : 'bg-red-500'
                      }`}
                    />
                  ) : null}
                  <span
                    className={`text-xs font-medium ${
                      item.connected ? 'text-foreground' : 'text-muted-foreground'
                    }`}
                  >
                    {item.status}
                  </span>
                </div>
              </div>
            );
          })}
        </div>
      </CardContent>
    </Card>
  );
};

export default SystemStatus;
