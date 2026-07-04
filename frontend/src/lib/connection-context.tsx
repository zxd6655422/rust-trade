'use client';

import React, { createContext, useContext, useEffect, useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

export type ConnectionStatus = 'connected' | 'disconnected' | 'connecting';

interface ConnectionState {
  /** trading-core 服务连接状态 */
  tradingCore: ConnectionStatus;
  /** 数据库连接状态 */
  database: ConnectionStatus;
  /** 最后检查时间 */
  lastCheck: Date | null;
  /** 错误信息 */
  error: string | null;
}

interface ConnectionContextType extends ConnectionState {
  /** 手动检查连接状态 */
  checkConnection: () => Promise<void>;
}

const ConnectionContext = createContext<ConnectionContextType>({
  tradingCore: 'disconnected',
  database: 'disconnected',
  lastCheck: null,
  error: null,
  checkConnection: async () => {},
});

export function useConnection() {
  return useContext(ConnectionContext);
}

interface ConnectionProviderProps {
  children: React.ReactNode;
  /** 自动检查间隔（毫秒），默认 30000 */
  checkInterval?: number;
}

export function ConnectionProvider({
  children,
  checkInterval = 30000,
}: ConnectionProviderProps) {
  const [state, setState] = useState<ConnectionState>({
    tradingCore: 'disconnected',
    database: 'disconnected',
    lastCheck: null,
    error: null,
  });

  const checkConnection = useCallback(async () => {
    setState((prev) => ({ ...prev, tradingCore: 'connecting' }));

    try {
      // 检查 trading-core 服务状态
      const result = await invoke<{ status: string; database: boolean }>(
        'check_trading_core_status'
      ).catch(() => null);

      if (result) {
        setState({
          tradingCore: 'connected',
          database: result.database ? 'connected' : 'disconnected',
          lastCheck: new Date(),
          error: null,
        });
      } else {
        // 尝试直接检查数据
        await invoke('get_data_info');
        setState({
          tradingCore: 'connected',
          database: 'connected',
          lastCheck: new Date(),
          error: null,
        });
      }
    } catch (err) {
      setState((prev) => ({
        ...prev,
        tradingCore: 'disconnected',
        lastCheck: new Date(),
        error: err instanceof Error ? err.message : 'Connection failed',
      }));
    }
  }, []);

  // 初始检查 + 定期检查
  useEffect(() => {
    checkConnection();
    const timer = setInterval(checkConnection, checkInterval);
    return () => clearInterval(timer);
  }, [checkConnection, checkInterval]);

  return (
    <ConnectionContext.Provider value={{ ...state, checkConnection }}>
      {children}
    </ConnectionContext.Provider>
  );
}
