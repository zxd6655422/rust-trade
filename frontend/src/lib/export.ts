// 数据导出工具

export type ExportFormat = 'csv' | 'json';

interface ExportOptions {
  filename: string;
  format: ExportFormat;
}

/**
 * 将数据导出为 CSV 文件
 */
export function exportToCsv<T extends Record<string, unknown>>(
  data: T[],
  options: ExportOptions
) {
  if (data.length === 0) {
    throw new Error('No data to export');
  }

  // 获取所有列名
  const headers = Object.keys(data[0]);

  // 构建 CSV 内容
  const csvContent = [
    // 表头
    headers.join(','),
    // 数据行
    ...data.map((row) =>
      headers
        .map((header) => {
          const value = row[header];
          // 处理包含逗号、引号或换行的值
          if (value === null || value === undefined) {
            return '';
          }
          const stringValue = String(value);
          if (
            stringValue.includes(',') ||
            stringValue.includes('"') ||
            stringValue.includes('\n')
          ) {
            return `"${stringValue.replace(/"/g, '""')}"`;
          }
          return stringValue;
        })
        .join(',')
    ),
  ].join('\n');

  // 添加 BOM 以支持中文
  const bom = '﻿';
  const blob = new Blob([bom + csvContent], { type: 'text/csv;charset=utf-8;' });

  downloadBlob(blob, `${options.filename}.csv`);
}

/**
 * 将数据导出为 JSON 文件
 */
export function exportToJson<T>(data: T[], options: ExportOptions) {
  const jsonContent = JSON.stringify(data, null, 2);
  const blob = new Blob([jsonContent], { type: 'application/json' });

  downloadBlob(blob, `${options.filename}.json`);
}

/**
 * 下载 Blob 为文件
 */
function downloadBlob(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = filename;
  document.body.appendChild(link);
  link.click();
  document.body.removeChild(link);
  URL.revokeObjectURL(url);
}

/**
 * 格式化日期为文件名可用格式
 */
export function formatDateForFilename(date: Date = new Date()): string {
  return date.toISOString().slice(0, 10).replace(/-/g, '');
}

/**
 * 导出交易历史
 */
export function exportTradeHistory(
  trades: Array<{
    timestamp: string;
    side: string;
    symbol: string;
    quantity: string;
    price: string;
    pnl?: string;
    commission?: string;
  }>,
  format: ExportFormat = 'csv'
) {
  const filename = `trade_history_${formatDateForFilename()}`;

  if (format === 'csv') {
    exportToCsv(trades, { filename, format });
  } else {
    exportToJson(trades, { filename, format });
  }
}

/**
 * 导出回测结果
 */
export function exportBacktestResult(
  result: {
    strategy: string;
    symbol: string;
    initial_capital: string;
    final_capital: string;
    total_return_pct: string;
    total_trades: number;
    win_rate: string;
    max_drawdown: string;
    sharpe_ratio: string;
    profit_factor: string;
    trades?: Array<Record<string, unknown>>;
  },
  format: ExportFormat = 'csv'
) {
  const filename = `backtest_${result.strategy}_${result.symbol}_${formatDateForFilename()}`;

  if (format === 'csv') {
    // 导出摘要信息
    exportToCsv(
      [
        {
          Strategy: result.strategy,
          Symbol: result.symbol,
          'Initial Capital': result.initial_capital,
          'Final Capital': result.final_capital,
          'Total Return': result.total_return_pct,
          'Total Trades': result.total_trades,
          'Win Rate': result.win_rate,
          'Max Drawdown': result.max_drawdown,
          'Sharpe Ratio': result.sharpe_ratio,
          'Profit Factor': result.profit_factor,
        },
      ],
      { filename, format }
    );
  } else {
    exportToJson(result, { filename, format });
  }
}

/**
 * 导出持仓数据
 */
export function exportPositions(
  positions: Array<{
    symbol: string;
    side: string;
    quantity: string;
    avg_price: string;
    current_price?: string;
    unrealized_pnl?: string;
  }>,
  format: ExportFormat = 'csv'
) {
  const filename = `positions_${formatDateForFilename()}`;

  if (format === 'csv') {
    exportToCsv(positions, { filename, format });
  } else {
    exportToJson(positions, { filename, format });
  }
}
