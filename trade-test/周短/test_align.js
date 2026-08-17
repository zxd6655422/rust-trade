/**
 * 对齐测试: JS 版完全复刻 Python backtest_features.py 逻辑
 * 纯30m bar, 入场/出场逻辑与Python完全一致
 */

const fs = require('fs');

function loadCSV(path, timeCol) {
  const content = fs.readFileSync(path, 'utf-8');
  const lines = content.trim().split('\n');
  const header = lines[0].split(',').map(h => h.replace(/"/g, '').trim());
  const rows = [];
  for (let i = 1; i < lines.length; i++) {
    const vals = lines[i].split(',').map(v => v.replace(/"/g, '').trim());
    if (vals.length < 6) continue;
    const row = {};
    for (let j = 0; j < header.length; j++) {
      if (header[j] === timeCol) row[header[j]] = new Date(vals[j]);
      else if (['open','high','low','close','volume'].includes(header[j])) row[header[j]] = parseFloat(vals[j]);
      else row[header[j]] = vals[j];
    }
    rows.push(row);
  }
  rows.sort((a, b) => a[timeCol] - b[timeCol]);
  return rows;
}

// 计算SMA序列
function calcSMA(closes, period) {
  const result = new Array(closes.length).fill(null);
  for (let i = period - 1; i < closes.length; i++) {
    let sum = 0;
    for (let j = i - period + 1; j <= i; j++) sum += closes[j];
    result[i] = sum / period;
  }
  return result;
}

// 完全对齐Python的策略函数
function runBacktest(df, params) {
  const closes = df.map(r => r.close);
  const fast_ma = calcSMA(closes, params.fast_ma_period);
  const slow_ma = calcSMA(closes, params.slow_ma_period);
  const SLOW = params.slow_ma_period; // 488

  let pos = null;
  const trades = [];

  for (let i = 0; i < df.length; i++) {
    if (i + 1 < SLOW) continue; // Python: if i + 1 < SLOW: continue

    const close = closes[i];
    const prev_close = closes[i - 1];
    const fma = fast_ma[i];
    const sma = slow_ma[i];
    const prev_fma = fast_ma[i - 1];
    const bar = df[i];

    // === 持仓中：平仓 ===
    if (pos !== null) {
      const pnl = pos.side === 'long' ? (close - pos.entry) / pos.entry * 100 : (pos.entry - close) / pos.entry * 100;
      pos.maxProfit = Math.max(pos.maxProfit, pnl);

      let exitPrice = null;
      let reason = '';

      // 硬止损 (Python line63-67)
      if (params.hard_stop_pct > 0) {
        if (pos.side === 'long' && bar.low <= pos.hardStopPrice) {
          exitPrice = pos.hardStopPrice; reason = '硬止损';
        } else if (pos.side === 'short' && bar.high >= pos.hardStopPrice) {
          exitPrice = pos.hardStopPrice; reason = '硬止损';
        }
      }

      // MA288穿越止损 (Python line69-73)
      // 关键: Python用 prev_close vs prev_fma, close vs fma
      if (exitPrice === null && params.stop_mode === 'ma288' && prev_fma !== null) {
        if (pos.side === 'long' && prev_close > prev_fma && close < fma) {
          exitPrice = close; reason = 'MA288止损';
        } else if (pos.side === 'short' && prev_close < prev_fma && close > fma) {
          exitPrice = close; reason = 'MA288止损';
        }
      }

      // 移动止盈 (Python line75-78)
      if (exitPrice === null && params.take_profit_mode === 'trailing') {
        if (pos.maxProfit >= params.trailing_activate_pct) {
          if (pos.maxProfit - pnl >= params.trailing_callback_pct) {
            exitPrice = close; reason = '移动止盈';
          }
        }
      }

      // 趋势反转出场 — 已禁用（生产系统无此逻辑）
      // if (exitPrice === null) {
      //   if (pos.side === 'long' && fma < sma) exitPrice = close;
      //   else if (pos.side === 'short' && fma > sma) exitPrice = close;
      // }

      if (exitPrice !== null) {
        const ret = pos.side === 'long' ? (exitPrice - pos.entry) / pos.entry : (pos.entry - exitPrice) / pos.entry;
        trades.push({ side: pos.side, entry: pos.entry, exit: exitPrice, ret_pct: ret * 100, reason, entryTime: pos.entryTime, exitTime: bar.open_time });
        pos = null;
        continue;
      }
    }

    // === 无持仓：入场 ===
    // Python line97-117
    if (pos === null && fma !== null && sma !== null && prev_fma !== null) {
      // 多头入场: prev_close < prev_fma AND close > fma
      if (fma > sma) {
        if (prev_close < prev_fma && close > fma) {
          const hardStop = params.hard_stop_pct > 0 ? close * (1 - params.hard_stop_pct / 100) : fma * 0.98;
          pos = { side: 'long', entry: close, hardStopPrice: hardStop, maxProfit: 0, entryTime: bar.open_time };
        }
      }
      // 空头入场: prev_close > prev_fma AND close < fma
      else if (fma < sma) {
        if (prev_close > prev_fma && close < fma) {
          const hardStop = params.hard_stop_pct > 0 ? close * (1 + params.hard_stop_pct / 100) : fma * 1.02;
          pos = { side: 'short', entry: close, hardStopPrice: hardStop, maxProfit: 0, entryTime: bar.open_time };
        }
      }
    }
  }

  // 末尾持仓处理 (Python line119-131)
  if (pos !== null) {
    const exitPrice = closes[closes.length - 1];
    const ret = pos.side === 'long' ? (exitPrice - pos.entry) / pos.entry : (pos.entry - exitPrice) / pos.entry;
    trades.push({ side: pos.side, entry: pos.entry, exit: exitPrice, ret_pct: ret * 100, reason: '持仓到结束', entryTime: pos.entryTime, exitTime: df[df.length-1].open_time });
  }

  const totalPnL = trades.reduce((s, t) => s + t.ret_pct, 0);
  const wins = trades.filter(t => t.ret_pct > 0).length;
  return { trades, totalPnL, tradeCount: trades.length, winCount: wins, winRate: trades.length > 0 ? wins / trades.length * 100 : 0 };
}

// ============================================================
// 测试: 用Python相同的参数
// ============================================================
const coins = [
  { name: 'BTC', file: '../../../data_2026-08-13/kline_30m_202608141617_BTC.csv', hs: 1.5, act: 4.0, cb: 1.0 },
  { name: 'ETH', file: '../../../data_2026-08-13/kline_30m_202608141605_ETH.csv', hs: 1.5, act: 5.0, cb: 1.0 },
  { name: 'SOL', file: '../../../data_2026-08-13/kline_30m_202608131247_SOL.csv', hs: 2.0, act: 4.0, cb: 1.0 },
  { name: 'BNB', file: '../../../data_2026-08-13/kline_30m_202608141530_BNB.csv', hs: 1.0, act: 6.0, cb: 2.0 },
  { name: 'SUI', file: '../../../data_2026-08-13/kline_30m_202608141533_SUI.csv', hs: 1.0, act: 6.0, cb: 2.0 },
  { name: 'HYPE', file: '../../../data_2026-08-13/kline_30m_202608141537_HYPE.csv', hs: 1.0, act: 6.0, cb: 0.5 },
];

// Python baseline results for comparison
const pythonBaseline = {
  BTC: { simple: 33.54, trades: 2024 },
  ETH: { simple: -10.66, trades: 2010 },
  SOL: { simple: 21.72, trades: 1364 },
  BNB: { simple: 61.41, trades: 1579 },
  SUI: { simple: 45.29, trades: 811 },
  HYPE: { simple: 8.96, trades: 304 },
};

console.log("=" .repeat(85));
console.log("对齐测试: JS (纯30m, Python同逻辑) vs Python 基线");
console.log("=" .repeat(85));
console.log(`${"币种".padEnd(5)} | ${"JS收益".padStart(10)} | ${"JS交易".padStart(6)} | ${"Py收益".padStart(10)} | ${"Py交易".padStart(6)} | ${"收益差".padStart(10)} | ${"交易差".padStart(6)}`);
console.log("-".repeat(85));

for (const c of coins) {
  const df = loadCSV(c.file, 'open_time');
  const params = {
    fast_ma_period: 288,
    slow_ma_period: 488,
    stop_mode: 'ma288',
    hard_stop_pct: c.hs,
    take_profit_mode: 'trailing',
    trailing_activate_pct: c.act,
    trailing_callback_pct: c.cb,
  };
  const result = runBacktest(df, params);
  const py = pythonBaseline[c.name];
  const pnlDiff = (result.totalPnL - py.simple).toFixed(2);
  const tradeDiff = result.tradeCount - py.trades;
  console.log(`${c.name.padEnd(5)} | ${(result.totalPnL>=0?'+':'')+result.totalPnL.toFixed(2).padStart(8)}% | ${String(result.tradeCount).padStart(6)} | ${(py.simple>=0?'+':'')+String(py.simple).padStart(8)}% | ${String(py.trades).padStart(6)} | ${(pnlDiff>=0?'+':'')+pnlDiff.padStart(8)}% | ${(tradeDiff>=0?'+':'')+String(tradeDiff).padStart(6)}`);
}
console.log("=" .repeat(85));
console.log("\n说明: 收益差 = JS - Python, 正值表示JS收益更高");
