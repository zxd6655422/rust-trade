// Trading Core 服务器配置
// 配置文件: public/config/trading-core.json

export interface TradingCoreConfig {
  server: {
    host: string;
    port: number;
    protocol: 'http' | 'https';
  };
  websocket: {
    enabled: boolean;
    reconnect_interval_ms: number;
    max_reconnect_attempts: number;
  };
  polling: {
    enabled: boolean;
    interval_ms: number;
    fallback_only: boolean; // 仅在 WebSocket 不可用时使用轮询
  };
}

const DEFAULT_CONFIG: TradingCoreConfig = {
  server: {
    host: 'localhost',
    port: 8080,
    protocol: 'http',
  },
  websocket: {
    enabled: true,
    reconnect_interval_ms: 5000,
    max_reconnect_attempts: 10,
  },
  polling: {
    enabled: true,
    interval_ms: 10000,
    fallback_only: true,
  },
};

let cachedConfig: TradingCoreConfig | null = null;

/**
 * 加载 Trading Core 配置
 * 从 public/config/trading-core.json 读取，失败则使用默认值
 */
export async function loadTradingCoreConfig(): Promise<TradingCoreConfig> {
  if (cachedConfig) return cachedConfig;

  try {
    const res = await fetch('/config/trading-core.json');
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const userConfig = await res.json();

    // 深度合并，用户配置覆盖默认值
    cachedConfig = deepMerge(DEFAULT_CONFIG, userConfig) as TradingCoreConfig;
  } catch {
    // 配置文件不存在或解析失败，使用默认值
    cachedConfig = { ...DEFAULT_CONFIG };
  }

  return cachedConfig;
}

/**
 * 获取 WebSocket URL
 */
export function getWebSocketUrl(config: TradingCoreConfig): string {
  const wsProtocol = config.server.protocol === 'https' ? 'wss' : 'ws';
  return `${wsProtocol}://${config.server.host}:${config.server.port}/ws`;
}

/**
 * 获取 HTTP API base URL
 */
export function getApiBaseUrl(config: TradingCoreConfig): string {
  return `${config.server.protocol}://${config.server.host}:${config.server.port}`;
}

/**
 * 深度合并对象
 */
function deepMerge(target: any, source: any): any {
  const result = { ...target };
  for (const key of Object.keys(source)) {
    if (
      source[key] &&
      typeof source[key] === 'object' &&
      !Array.isArray(source[key]) &&
      target[key] &&
      typeof target[key] === 'object'
    ) {
      result[key] = deepMerge(target[key], source[key]);
    } else {
      result[key] = source[key];
    }
  }
  return result;
}
