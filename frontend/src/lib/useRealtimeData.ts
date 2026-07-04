'use client';

import { useEffect, useRef, useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  TradingCoreConfig,
  loadTradingCoreConfig,
  getWebSocketUrl,
} from './config';

// ============ 类型定义 ============

export interface RealtimePrice {
  symbol: string;
  price: string;
  change_24h?: string;
  volume_24h?: string;
  high_24h?: string;
  low_24h?: string;
  updated_at: string;
}

export type DataSource = 'websocket' | 'polling' | 'disconnected';

interface WsTickMessage {
  Tick: {
    symbol: string;
    price: string;
    quantity: string;
    timestamp: string;
  };
}

interface WsSubscribedMessage {
  Subscribed: { symbols: string[] };
}

type WsMessage = WsTickMessage | WsSubscribedMessage | { Error: { message: string } };

interface UseRealtimeDataOptions {
  symbols?: string[];
  onPriceUpdate?: (price: RealtimePrice) => void;
}

interface UseRealtimeDataReturn {
  prices: Map<string, RealtimePrice>;
  dataSource: DataSource;
  isConnected: boolean;
  reconnect: () => void;
}

// ============ Hook 实现 ============

/**
 * 实时数据 Hook
 *
 * 双机制策略：
 * 1. 优先使用 WebSocket 连接 trading-core 服务获取实时推送
 * 2. WebSocket 不可用时，自动降级为 Tauri 命令轮询数据库
 * 3. WebSocket 恢复后自动切换回推送模式
 */
export function useRealtimeData(
  options: UseRealtimeDataOptions = {}
): UseRealtimeDataReturn {
  const { symbols = [], onPriceUpdate } = options;

  const [prices, setPrices] = useState<Map<string, RealtimePrice>>(new Map());
  const [dataSource, setDataSource] = useState<DataSource>('disconnected');
  const [isConnected, setIsConnected] = useState(false);

  const wsRef = useRef<WebSocket | null>(null);
  const configRef = useRef<TradingCoreConfig | null>(null);
  const reconnectCountRef = useRef(0);
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pollingTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const mountedRef = useRef(true);

  // 更新价格
  const updatePrice = useCallback(
    (price: RealtimePrice) => {
      if (!mountedRef.current) return;
      setPrices((prev) => {
        const next = new Map(prev);
        next.set(price.symbol, price);
        return next;
      });
      onPriceUpdate?.(price);
    },
    [onPriceUpdate]
  );

  // ============ 轮询模式 ============

  const startPolling = useCallback(
    async (config: TradingCoreConfig) => {
      if (!config.polling.enabled) return;
      if (pollingTimerRef.current) return; // 已在轮询中

      setDataSource('polling');

      const poll = async () => {
        if (!mountedRef.current) return;
        try {
          const targetSymbols =
            symbols.length > 0
              ? symbols
              : ['BTCUSDT', 'ETHUSDT', 'SOLUSDT'];
          const result = await invoke<RealtimePrice[]>(
            'get_realtime_prices',
            { symbols: targetSymbols }
          );
          for (const p of result) {
            updatePrice(p);
          }
        } catch {
          // 轮询失败，静默忽略
        }
      };

      // 立即执行一次
      await poll();
      // 定时轮询
      pollingTimerRef.current = setInterval(poll, config.polling.interval_ms);
    },
    [symbols, updatePrice]
  );

  const stopPolling = useCallback(() => {
    if (pollingTimerRef.current) {
      clearInterval(pollingTimerRef.current);
      pollingTimerRef.current = null;
    }
  }, []);

  // ============ WebSocket 模式 ============

  const connectWebSocket = useCallback(
    (config: TradingCoreConfig) => {
      if (!config.websocket.enabled) return;
      if (wsRef.current?.readyState === WebSocket.OPEN) return;

      const url = getWebSocketUrl(config);

      try {
        const ws = new WebSocket(url);
        wsRef.current = ws;

        ws.onopen = () => {
          if (!mountedRef.current) {
            ws.close();
            return;
          }
          setIsConnected(true);
          setDataSource('websocket');
          reconnectCountRef.current = 0;

          // 停止轮询（如果正在轮询）
          if (config.polling.fallback_only) {
            stopPolling();
          }

          // 订阅 symbols
          if (symbols.length > 0) {
            ws.send(
              JSON.stringify({ Subscribe: { symbols } })
            );
          }
        };

        ws.onmessage = (event) => {
          if (!mountedRef.current) return;
          try {
            const msg: WsMessage = JSON.parse(event.data);
            if ('Tick' in msg) {
              const tick = msg.Tick;
              updatePrice({
                symbol: tick.symbol,
                price: tick.price,
                updated_at: tick.timestamp,
              });
            }
          } catch {
            // 解析失败，忽略
          }
        };

        ws.onclose = () => {
          if (!mountedRef.current) return;
          setIsConnected(false);
          wsRef.current = null;

          // 降级到轮询
          if (config.polling.enabled && config.polling.fallback_only) {
            startPolling(config);
          }

          // 尝试重连
          scheduleReconnect(config);
        };

        ws.onerror = () => {
          // onerror 之后会触发 onclose，不需要额外处理
        };
      } catch {
        // WebSocket 构造失败，降级到轮询
        if (config.polling.enabled) {
          startPolling(config);
        }
        scheduleReconnect(config);
      }
    },
    [symbols, updatePrice, stopPolling, startPolling]
  );

  const scheduleReconnect = useCallback(
    (config: TradingCoreConfig) => {
      if (!mountedRef.current) return;
      if (reconnectTimerRef.current) return;
      if (
        reconnectCountRef.current >= config.websocket.max_reconnect_attempts
      ) {
        // 超过最大重连次数，保持轮询模式
        return;
      }

      reconnectTimerRef.current = setTimeout(() => {
        reconnectTimerRef.current = null;
        reconnectCountRef.current += 1;
        if (mountedRef.current) {
          connectWebSocket(config);
        }
      }, config.websocket.reconnect_interval_ms);
    },
    [connectWebSocket]
  );

  // 手动重连
  const reconnect = useCallback(() => {
    reconnectCountRef.current = 0;
    if (wsRef.current) {
      wsRef.current.close();
      wsRef.current = null;
    }
    stopPolling();
    if (configRef.current) {
      connectWebSocket(configRef.current);
    }
  }, [connectWebSocket, stopPolling]);

  // ============ 生命周期 ============

  useEffect(() => {
    mountedRef.current = true;

    const init = async () => {
      const config = await loadTradingCoreConfig();
      configRef.current = config;

      if (!mountedRef.current) return;

      // 尝试 WebSocket 连接
      if (config.websocket.enabled) {
        connectWebSocket(config);
      }

      // 如果配置了 fallback_only=false，同时启动轮询
      if (config.polling.enabled && !config.polling.fallback_only) {
        startPolling(config);
      }
    };

    init();

    return () => {
      mountedRef.current = false;
      if (wsRef.current) {
        wsRef.current.close();
        wsRef.current = null;
      }
      stopPolling();
      if (reconnectTimerRef.current) {
        clearTimeout(reconnectTimerRef.current);
        reconnectTimerRef.current = null;
      }
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // symbols 变化时重新订阅
  useEffect(() => {
    if (
      wsRef.current?.readyState === WebSocket.OPEN &&
      symbols.length > 0
    ) {
      wsRef.current.send(
        JSON.stringify({ Subscribe: { symbols } })
      );
    }
  }, [symbols]);

  return { prices, dataSource, isConnected, reconnect };
}
