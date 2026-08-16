/**
 * ETH 第十四次分析: 综合测试 (硬止损 + 止盈参数)
 *
 * 配置: 5m+30m双扩散, 无slope/bbw/vol过滤
 * 测试: 硬止损 × 止盈参数 组合
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
      if (header[j] === timeCol) {
        row[header[j]] = new Date(vals[j]);
      } else if (['open','high','low','close','volume'].includes(header[j])) {
        row[header[j]] = parseFloat(vals[j]);
      } else {
        row[header[j]] = vals[j];
      }
    }
    rows.push(row);
  }
  rows.sort((a, b) => a[timeCol] - b[timeCol]);
  return rows;
}

console.log("=".repeat(70));
console.log("ETH 第十四次分析: 综合测试 (硬止损 + 止盈参数)");
console.log("=".repeat(70));

const df_5m = loadCSV('kline_5m_202607232006.csv', 'open_time');
const df_30m = loadCSV('kline_30m_202607232006.csv', 'open_time');

function addIndicators(df, prefix = '') {
  const closes = df.map(r => r.close);
  const volumes = df.map(r => r.volume);
  const calcMA = (period) => {
    const result = new Array(df.length).fill(null);
    for (let i = period - 1; i < df.length; i++) {
      let sum = 0;
      for (let j = i - period + 1; j <= i; j++) sum += closes[j];
      result[i] = sum / period;
    }
    return result;
  };
  const ma288 = calcMA(288);
  const ma488 = calcMA(488);
  const spread = new Array(df.length).fill(null);
  const isExpanding = new Array(df.length).fill(null);
  for (let i = 0; i < df.length; i++) {
    if (ma288[i] !== null && ma488[i] !== null) spread[i] = ma288[i] - ma488[i];
  }
  const anglePeriod = 5;
  for (let i = anglePeriod; i < df.length; i++) {
    if (spread[i] !== null && spread[i - anglePeriod] !== null) {
      isExpanding[i] = Math.abs(spread[i]) > Math.abs(spread[i - anglePeriod]);
    }
  }
  for (let i = 0; i < df.length; i++) {
    df[i][`${prefix}ma288`] = ma288[i];
    df[i][`${prefix}ma488`] = ma488[i];
    df[i][`${prefix}spread`] = spread[i];
    df[i][`${prefix}isExpanding`] = isExpanding[i];
  }
  return df;
}

addIndicators(df_5m, 'm5_');
addIndicators(df_30m, 'm30_');

const df_5m_valid = df_5m.filter(r => r.m5_ma288 !== null && r.m5_ma488 !== null);
const df_30m_valid = df_30m.filter(r => r.m30_ma288 !== null && r.m30_ma488 !== null);

function build5mMap(df5) {
  const map = new Map();
  for (const r of df5) {
    map.set(r.open_time.getTime(), { isExpanding: r.m5_isExpanding });
  }
  return map;
}
const map5m = build5mMap(df_5m_valid);

function get5mAt(time) {
  const t = time.getTime();
  let best = null;
  let bestDiff = Infinity;
  for (const [ts, data] of map5m) {
    const diff = t - ts;
    if (diff >= 0 && diff < bestDiff) { bestDiff = diff; best = data; }
    if (diff < 0) break;
  }
  return best;
}

function runStrategy(df, config) {
  const {
    useHardStop = true, hardStopPct = 2.0,
    tpMode = 'trailing', trailingActivate = 5.0, trailingCallback = 5.0,
    use5mExpanding = true, use30mExpanding = true,
  } = config;

  let position = null;
  let entryPrice = 0, hardStopPrice = 0, maxProfitPct = 0;
  let totalPnL = 0, winCount = 0, lossCount = 0;
  let maxWin = 0, maxLoss = 0;
  const trades = [];

  for (let i = 1; i < df.length; i++) {
    const row = df[i];
    const ma288 = row.m30_ma288, ma488 = row.m30_ma488;
    const o = row.open, h = row.high, l = row.low, c = row.close;

    if (ma288 < ma488) continue;

    if (use30mExpanding && row.m30_isExpanding === false) continue;
    if (use5mExpanding) {
      const data5m = get5mAt(row.open_time);
      if (data5m && !data5m.isExpanding) continue;
    }

    if (position !== null) {
      const currentPnl = position === 'long' ? (c - entryPrice) / entryPrice * 100 : (entryPrice - c) / entryPrice * 100;
      maxProfitPct = Math.max(maxProfitPct, currentPnl);

      let shouldStop = false, exitPrice = c;
      if (useHardStop && position === 'long' && l <= hardStopPrice) { shouldStop = true; exitPrice = hardStopPrice; }
      else if (useHardStop && position === 'short' && h >= hardStopPrice) { shouldStop = true; exitPrice = hardStopPrice; }
      else if (position === 'long' && o > ma288 && c < ma288) { shouldStop = true; }
      else if (position === 'short' && o < ma288 && c > ma288) { shouldStop = true; }

      if (shouldStop) {
        const pnl = position === 'long' ? (exitPrice - entryPrice) / entryPrice * 100 : (entryPrice - exitPrice) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); } else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ pnl });
        position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0;
        continue;
      }

      if (tpMode === 'trailing' && maxProfitPct >= trailingActivate) {
        if (maxProfitPct - currentPnl >= trailingCallback) {
          totalPnL += currentPnl;
          if (currentPnl > 0) { winCount++; maxWin = Math.max(maxWin, currentPnl); } else { lossCount++; maxLoss = Math.min(maxLoss, currentPnl); }
          trades.push({ pnl: currentPnl });
          position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0;
          continue;
        }
      }
    }

    let isEntry = false, entryDir = '';
    if (o < ma288 && c > ma288) { isEntry = true; entryDir = 'long'; }
    else if (o > ma288 && c < ma288) { isEntry = true; entryDir = 'short'; }

    if (isEntry) {
      if (position !== null && position !== entryDir) {
        const pnl = position === 'long' ? (c - entryPrice) / entryPrice * 100 : (entryPrice - c) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); } else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ pnl });
        position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0;
      }
      if (position === null) {
        position = entryDir; entryPrice = c; maxProfitPct = 0;
        hardStopPrice = entryDir === 'long' ? entryPrice * (1 - hardStopPct / 100) : entryPrice * (1 + hardStopPct / 100);
      }
    }
  }

  return {
    tradeCount: trades.length, winCount, lossCount,
    winRate: trades.length > 0 ? (winCount / trades.length * 100) : 0,
    totalPnL, avgPnL: trades.length > 0 ? (totalPnL / trades.length) : 0,
    maxWin, maxLoss,
    profitFactor: maxLoss !== 0 ? (maxWin / Math.abs(maxLoss)) : 0,
  };
}

// ============================================================
// 综合测试: 硬止损 × 止盈参数
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【综合测试: 硬止损 × 止盈参数】");
console.log("=".repeat(70));

const hardStopPcts = [0, 1.0, 1.5, 2.0, 2.5, 3.0];
const trailingConfigs = [
  { label: '移动(3+3)', act: 3, cb: 3 },
  { label: '移动(3+5)', act: 3, cb: 5 },
  { label: '移动(4+4)', act: 4, cb: 4 },
  { label: '移动(4+6)', act: 4, cb: 6 },
  { label: '移动(5+3)', act: 5, cb: 3 },
  { label: '移动(5+5)', act: 5, cb: 5 },
  { label: '移动(5+6)', act: 5, cb: 6 },
  { label: '移动(6+4)', act: 6, cb: 4 },
  { label: '移动(6+6)', act: 6, cb: 6 },
  { label: '移动(6+8)', act: 6, cb: 8 },
  { label: '移动(8+3)', act: 8, cb: 3 },
  { label: '移动(8+5)', act: 8, cb: 5 },
  { label: '移动(8+6)', act: 8, cb: 6 },
  { label: '移动(8+8)', act: 8, cb: 8 },
  { label: '移动(10+5)', act: 10, cb: 5 },
  { label: '移动(10+6)', act: 10, cb: 6 },
  { label: '移动(10+8)', act: 10, cb: 8 },
];

// 打印表头
let header = "硬止损\\止盈  |";
for (const tc of trailingConfigs) header += ` ${tc.label.padStart(10)} |`;
console.log("\n" + header);
console.log("-".repeat(header.length));

const results = [];

for (const hs of hardStopPcts) {
  let row = `  ${hs > 0 ? hs + '%' : '无'.padStart(3)}       |`;
  for (const tc of trailingConfigs) {
    const r = runStrategy(df_30m_valid, {
      useHardStop: hs > 0,
      hardStopPct: hs,
      tpMode: 'trailing',
      trailingActivate: tc.act,
      trailingCallback: tc.cb,
      use5mExpanding: true,
      use30mExpanding: true,
    });
    results.push({ hardStop: hs, activate: tc.act, callback: tc.cb, ...r });
    row += `${r.totalPnL >= 0 ? '+' : ''}${r.totalPnL.toFixed(1).padStart(8)}% |`;
  }
  console.log(row);
}

// ============================================================
// 最优组合详细分析
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【最优组合详细分析】");
console.log("=".repeat(70));

const bestByReturn = results.reduce((a, b) => a.totalPnL > b.totalPnL ? a : b);
const bestByProfitFactor = results.reduce((a, b) => a.profitFactor > b.profitFactor ? a : b);
const bestByMaxLoss = results.reduce((a, b) => a.maxLoss > b.maxLoss ? a : b);

console.log(`
指标          | 硬止损 | activate | callback | 交易数 | 胜率   | 总收益   | 最大亏  | 盈亏比
-------------|--------|----------|----------|--------|--------|----------|---------|-------
最高总收益     | ${bestByReturn.hardStop > 0 ? bestByReturn.hardStop + '%' : '无'.padStart(6)} | ${String(bestByReturn.activate).padStart(8)}% | ${String(bestByReturn.callback).padStart(8)}% | ${String(bestByReturn.tradeCount).padStart(6)} | ${bestByReturn.winRate.toFixed(1).padStart(5)}% | ${(bestByReturn.totalPnL >= 0 ? '+' : '') + bestByReturn.totalPnL.toFixed(2).padStart(7)}% | ${bestByReturn.maxLoss.toFixed(2).padStart(7)}% | ${bestByReturn.profitFactor.toFixed(2).padStart(6)}
最高盈亏比     | ${bestByProfitFactor.hardStop > 0 ? bestByProfitFactor.hardStop + '%' : '无'.padStart(6)} | ${String(bestByProfitFactor.activate).padStart(8)}% | ${String(bestByProfitFactor.callback).padStart(8)}% | ${String(bestByProfitFactor.tradeCount).padStart(6)} | ${bestByProfitFactor.winRate.toFixed(1).padStart(5)}% | ${(bestByProfitFactor.totalPnL >= 0 ? '+' : '') + bestByProfitFactor.totalPnL.toFixed(2).padStart(7)}% | ${bestByProfitFactor.maxLoss.toFixed(2).padStart(7)}% | ${bestByProfitFactor.profitFactor.toFixed(2).padStart(6)}
最小最大亏损   | ${bestByMaxLoss.hardStop > 0 ? bestByMaxLoss.hardStop + '%' : '无'.padStart(6)} | ${String(bestByMaxLoss.activate).padStart(8)}% | ${String(bestByMaxLoss.callback).padStart(8)}% | ${String(bestByMaxLoss.tradeCount).padStart(6)} | ${bestByMaxLoss.winRate.toFixed(1).padStart(5)}% | ${(bestByMaxLoss.totalPnL >= 0 ? '+' : '') + bestByMaxLoss.totalPnL.toFixed(2).padStart(7)}% | ${bestByMaxLoss.maxLoss.toFixed(2).padStart(7)}% | ${bestByMaxLoss.profitFactor.toFixed(2).padStart(6)}
`);

console.log("ETH 综合测试完成！");
