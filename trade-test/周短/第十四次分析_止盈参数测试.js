/**
 * 第十四次分析补充: 移动止盈参数测试
 *
 * 测试不同 trailingActivate 和 trailingCallback 组合
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
console.log("移动止盈参数测试");
console.log("=".repeat(70));

const df_5m = loadCSV('../kline_5m_202607222208.csv', 'open_time');
const df_30m = loadCSV('../kline_30m_202607222207.csv', 'open_time');

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
  const ma48 = calcMA(48);
  const ma288 = calcMA(288);
  const ma488 = calcMA(488);
  const bbMid = calcMA(100);
  const bbUpper = new Array(df.length).fill(null);
  const bbLower = new Array(df.length).fill(null);
  const bbWidth = new Array(df.length).fill(null);
  const bbPos = new Array(df.length).fill(null);
  for (let i = 99; i < df.length; i++) {
    let sumSq = 0;
    for (let j = i - 99; j <= i; j++) sumSq += (closes[j] - bbMid[i]) ** 2;
    const std = Math.sqrt(sumSq / 100);
    bbUpper[i] = bbMid[i] + 2 * std;
    bbLower[i] = bbMid[i] - 2 * std;
    bbWidth[i] = (bbUpper[i] - bbLower[i]) / bbMid[i] * 100;
    bbPos[i] = (closes[i] - bbLower[i]) / (bbUpper[i] - bbLower[i]) * 100;
  }
  const ma288Slope = new Array(df.length).fill(null);
  for (let i = 5; i < df.length; i++) {
    if (ma288[i] !== null && ma288[i-5] !== null && ma288[i-5] !== 0) {
      ma288Slope[i] = (ma288[i] - ma288[i-5]) / ma288[i-5] * 10000;
    }
  }
  const volMA = new Array(df.length).fill(null);
  const volRatio = new Array(df.length).fill(null);
  for (let i = 19; i < df.length; i++) {
    let sum = 0;
    for (let j = i - 19; j <= i; j++) sum += volumes[j];
    volMA[i] = sum / 20;
    if (volMA[i] > 0) volRatio[i] = volumes[i] / volMA[i];
  }
  const spread = new Array(df.length).fill(null);
  const spreadDelta = new Array(df.length).fill(null);
  const isExpanding = new Array(df.length).fill(null);
  const angleApprox = new Array(df.length).fill(null);
  for (let i = 0; i < df.length; i++) {
    if (ma288[i] !== null && ma488[i] !== null) spread[i] = ma288[i] - ma488[i];
  }
  const anglePeriod = 5;
  for (let i = anglePeriod; i < df.length; i++) {
    if (spread[i] !== null && spread[i - anglePeriod] !== null) {
      spreadDelta[i] = spread[i] - spread[i - anglePeriod];
      isExpanding[i] = Math.abs(spread[i]) > Math.abs(spread[i - anglePeriod]);
      angleApprox[i] = Math.atan2(spreadDelta[i], anglePeriod) * (180 / Math.PI);
    }
  }
  for (let i = 0; i < df.length; i++) {
    df[i][`${prefix}ma48`] = ma48[i];
    df[i][`${prefix}ma288`] = ma288[i];
    df[i][`${prefix}ma488`] = ma488[i];
    df[i][`${prefix}bbWidth`] = bbWidth[i];
    df[i][`${prefix}bbPos`] = bbPos[i];
    df[i][`${prefix}ma288Slope`] = ma288Slope[i];
    df[i][`${prefix}volRatio`] = volRatio[i];
    df[i][`${prefix}isExpanding`] = isExpanding[i];
    df[i][`${prefix}angleApprox`] = angleApprox[i];
  }
  return df;
}

addIndicators(df_5m, 'm5_');
addIndicators(df_30m, 'm30_');

const df_5m_valid = df_5m.filter(r => r.m5_ma288 !== null && r.m5_ma488 !== null);
const df_30m_valid = df_30m.filter(r => r.m30_ma288 !== null && r.m30_ma488 !== null);

function build30mMap(df30) {
  const map = new Map();
  for (const r of df30) {
    map.set(r.open_time.getTime(), {
      trend: r.m30_ma288 > r.m30_ma488 ? 'bullish' : 'bearish',
      isExpanding: r.m30_isExpanding,
    });
  }
  return map;
}
const map30m = build30mMap(df_30m_valid);

function get30mAt(time) {
  const t = time.getTime();
  let best = null;
  let bestDiff = Infinity;
  for (const [ts, data] of map30m) {
    const diff = t - ts;
    if (diff >= 0 && diff < bestDiff) { bestDiff = diff; best = data; }
    if (diff < 0) break;
  }
  return best;
}

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

// ============================================================
// 策略回测 (与第十四次相同)
// ============================================================
function runStrategy(df, config) {
  const {
    useHardStop = true,
    hardStopPct = 2.0,
    stopMode = 'ma288',
    tpMode = 'trailing',
    trailingActivate = 5.0,
    trailingCallback = 5.0,
    slopeThreshold = 5,
    bbwThreshold = 2.0,
    volThreshold = 0.6,
    use5mExpanding = true,
    entryTimeframe = '30m',
  } = config;

  let position = null;
  let entryPrice = 0, entryTime = null, hardStopPrice = 0;
  let maxProfitPct = 0;
  let totalPnL = 0, winCount = 0, lossCount = 0;
  let maxWin = 0, maxLoss = 0;
  const trades = [];

  const dfEntry = entryTimeframe === '5m' ? df_5m_valid : df;
  const prefix = entryTimeframe === '5m' ? 'm5_' : 'm30_';

  for (let i = 1; i < dfEntry.length; i++) {
    const row = dfEntry[i];
    const ma288 = row[`${prefix}ma288`];
    const ma488 = row[`${prefix}ma488`];
    const ma48 = row[`${prefix}ma48`];
    const o = row.open, h = row.high, l = row.low, c = row.close;
    const slope = row[`${prefix}ma288Slope`];
    const bbw = row[`${prefix}bbWidth`];
    const volRatio = row[`${prefix}volRatio`];
    const bbPos = row[`${prefix}bbPos`];

    let trend;
    if (entryTimeframe === '5m') {
      const data30m = get30mAt(row.open_time);
      if (!data30m) continue;
      trend = data30m.trend;
    } else {
      if (ma288 < ma488) trend = 'bearish';
      else if (ma288 > ma488) trend = 'bullish';
      else continue;
    }

    if (use5mExpanding) {
      let expanding5m = null;
      if (entryTimeframe === '5m') expanding5m = row.m5_isExpanding;
      else { const d = get5mAt(row.open_time); if (d) expanding5m = d.isExpanding; }
      if (expanding5m !== null && !expanding5m) continue;
    }

    if (slopeThreshold > 0 && slope !== null && Math.abs(slope) < slopeThreshold) continue;
    if (bbwThreshold > 0 && bbw !== null && bbw < bbwThreshold) continue;
    if (volThreshold > 0 && volRatio !== null && volRatio < volThreshold) continue;

    if (position !== null) {
      const currentPnl = position === 'long' ? (c - entryPrice) / entryPrice * 100 : (entryPrice - c) / entryPrice * 100;
      maxProfitPct = Math.max(maxProfitPct, currentPnl);

      let shouldStop = false, stopReason = '', exitPrice = c;

      if (useHardStop) {
        if (position === 'long' && l <= hardStopPrice) { shouldStop = true; stopReason = 'HARD_STOP'; exitPrice = hardStopPrice; }
        else if (position === 'short' && h >= hardStopPrice) { shouldStop = true; stopReason = 'HARD_STOP'; exitPrice = hardStopPrice; }
      }

      if (!shouldStop) {
        if (position === 'long' && o > ma288 && c < ma288) { shouldStop = true; stopReason = 'MA288_STOP'; }
        else if (position === 'short' && o < ma288 && c > ma288) { shouldStop = true; stopReason = 'MA288_STOP'; }
      }

      if (shouldStop) {
        const actualPnl = position === 'long' ? (exitPrice - entryPrice) / entryPrice * 100 : (entryPrice - exitPrice) / entryPrice * 100;
        totalPnL += actualPnl;
        if (actualPnl > 0) { winCount++; maxWin = Math.max(maxWin, actualPnl); } else { lossCount++; maxLoss = Math.min(maxLoss, actualPnl); }
        trades.push({ pnl: actualPnl, reason: stopReason });
        position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0;
        continue;
      }

      // 移动止盈
      if (tpMode === 'trailing') {
        if (maxProfitPct >= trailingActivate) {
          const drawdown = maxProfitPct - currentPnl;
          if (drawdown >= trailingCallback) {
            totalPnL += currentPnl;
            if (currentPnl > 0) { winCount++; maxWin = Math.max(maxWin, currentPnl); } else { lossCount++; maxLoss = Math.min(maxLoss, currentPnl); }
            trades.push({ pnl: currentPnl, reason: 'TRAILING_TP' });
            position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0; 
            continue;
          }
        }
      }
    }

    let isEntry = false, entryDir = '';
    if (trend === 'bullish' && o < ma288 && c > ma288) { isEntry = true; entryDir = 'long'; }
    else if (trend === 'bearish' && o > ma288 && c < ma288) { isEntry = true; entryDir = 'short'; }

    if (isEntry) {
      if (position !== null && position !== entryDir) {
        const pnl = position === 'long' ? (c - entryPrice) / entryPrice * 100 : (entryPrice - c) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); } else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ pnl, reason: 'REVERSE_CLOSE' });
        position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0; 
      }
      if (position === null) {
        position = entryDir;
        entryPrice = c;
        entryTime = row.open_time;
        maxProfitPct = 0;
        
        if (useHardStop) {
          hardStopPrice = entryDir === 'long' ? entryPrice * (1 - hardStopPct / 100) : entryPrice * (1 + hardStopPct / 100);
        }
      }
    }

    if (position === 'long' && trend === 'bearish' && o > ma288 && c < ma288) {
      const pnl = (c - entryPrice) / entryPrice * 100;
      totalPnL += pnl;
      if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); } else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
      trades.push({ pnl, reason: 'TREND_REV' });
      position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0; 
    } else if (position === 'short' && trend === 'bullish' && o < ma288 && c > ma288) {
      const pnl = (entryPrice - c) / entryPrice * 100;
      totalPnL += pnl;
      if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); } else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
      trades.push({ pnl, reason: 'TREND_REV' });
      position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0; 
    }
  }

  return {
    tradeCount: trades.length, winCount, lossCount,
    winRate: trades.length > 0 ? (winCount / trades.length * 100) : 0,
    totalPnL, avgPnL: trades.length > 0 ? (totalPnL / trades.length) : 0,
    maxWin, maxLoss,
    profitFactor: maxLoss !== 0 ? (maxWin / Math.abs(maxLoss)) : 0,
    trades
  };
}

// ============================================================
// 测试1: trailingActivate 单独测试 (固定callback=5%)
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试1: trailingActivate 变化 (固定 callback=5%)】");
console.log("=".repeat(70));

console.log("\nactivate | callback | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 最大亏  | 盈亏比");
console.log("-".repeat(100));

const activateTests = [1, 2, 3, 4, 5, 6, 8, 10, 15, 20];
const test1Results = [];

for (const act of activateTests) {
  const r = runStrategy(df_30m_valid, {
    useHardStop: true, hardStopPct: 2.0,
    tpMode: 'trailing', trailingActivate: act, trailingCallback: 5,
    slopeThreshold: 5, bbwThreshold: 2.0, volThreshold: 0.6,
    use5mExpanding: true, entryTimeframe: '30m',
  });
  test1Results.push({ activate: act, ...r });
  console.log(
    `${String(act).padStart(4)}%    |    5%    | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}% | ` +
    `${r.maxWin.toFixed(2).padStart(7)}% | ${r.maxLoss.toFixed(2).padStart(7)}% | ${r.profitFactor.toFixed(2).padStart(6)}`
  );
}

// ============================================================
// 测试2: trailingCallback 单独测试 (固定activate=5%)
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试2: trailingCallback 变化 (固定 activate=5%)】");
console.log("=".repeat(70));

console.log("\nactivate | callback | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 最大亏  | 盈亏比");
console.log("-".repeat(100));

const callbackTests = [1, 2, 3, 4, 5, 6, 8, 10];
const test2Results = [];

for (const cb of callbackTests) {
  const r = runStrategy(df_30m_valid, {
    useHardStop: true, hardStopPct: 2.0,
    tpMode: 'trailing', trailingActivate: 5, trailingCallback: cb,
    slopeThreshold: 5, bbwThreshold: 2.0, volThreshold: 0.6,
    use5mExpanding: true, entryTimeframe: '30m',
  });
  test2Results.push({ callback: cb, ...r });
  console.log(
    `   5%    | ${String(cb).padStart(4)}%     | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}% | ` +
    `${r.maxWin.toFixed(2).padStart(7)}% | ${r.maxLoss.toFixed(2).padStart(7)}% | ${r.profitFactor.toFixed(2).padStart(6)}`
  );
}

// ============================================================
// 测试3: activate + callback 组合矩阵
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试3: activate × callback 组合矩阵】");
console.log("=".repeat(70));

const matrixActivates = [2, 3, 4, 5, 6, 8, 10];
const matrixCallbacks = [1, 2, 3, 4, 5, 6, 8, 10];

// 打印表头
let header = "activate\\cb |";
for (const cb of matrixCallbacks) header += `  ${cb}%    |`;
console.log("\n" + header);
console.log("-".repeat(header.length));

const matrixResults = [];

for (const act of matrixActivates) {
  let row = `    ${String(act).padStart(2)}%     |`;
  for (const cb of matrixCallbacks) {
    const r = runStrategy(df_30m_valid, {
      useHardStop: true, hardStopPct: 2.0,
      tpMode: 'trailing', trailingActivate: act, trailingCallback: cb,
      slopeThreshold: 5, bbwThreshold: 2.0, volThreshold: 0.6,
      use5mExpanding: true, entryTimeframe: '30m',
    });
    matrixResults.push({ activate: act, callback: cb, ...r });
    row += `${r.totalPnL >= 0 ? '+' : ''}${r.totalPnL.toFixed(1).padStart(5)}% |`;
  }
  console.log(row);
}

// ============================================================
// 测试4: 最优组合详细分析
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试4: 最优组合详细分析】");
console.log("=".repeat(70));

// 找最优
const allResults = [...test1Results.map(r => ({...r, label: `act=${r.activate}% cb=5%`})),
                     ...test2Results.map(r => ({...r, label: `act=5% cb=${r.callback}%`})),
                     ...matrixResults.map(r => ({...r, label: `act=${r.activate}% cb=${r.callback}%`}))];

const bestByReturn = allResults.reduce((a, b) => a.totalPnL > b.totalPnL ? a : b);
const bestByProfitFactor = allResults.reduce((a, b) => a.profitFactor > b.profitFactor ? a : b);
const bestByMaxLoss = allResults.reduce((a, b) => a.maxLoss > b.maxLoss ? a : b);

console.log(`
指标          | 最优配置              | 交易数 | 胜率   | 总收益   | 最大亏  | 盈亏比
-------------|----------------------|--------|--------|----------|---------|-------
最高总收益     | ${bestByReturn.label.padEnd(20)} | ${String(bestByReturn.tradeCount).padStart(6)} | ${bestByReturn.winRate.toFixed(1).padStart(5)}% | ${(bestByReturn.totalPnL >= 0 ? '+' : '') + bestByReturn.totalPnL.toFixed(2).padStart(7)}% | ${bestByReturn.maxLoss.toFixed(2).padStart(7)}% | ${bestByReturn.profitFactor.toFixed(2).padStart(6)}
最高盈亏比     | ${bestByProfitFactor.label.padEnd(20)} | ${String(bestByProfitFactor.tradeCount).padStart(6)} | ${bestByProfitFactor.winRate.toFixed(1).padStart(5)}% | ${(bestByProfitFactor.totalPnL >= 0 ? '+' : '') + bestByProfitFactor.totalPnL.toFixed(2).padStart(7)}% | ${bestByProfitFactor.maxLoss.toFixed(2).padStart(7)}% | ${bestByProfitFactor.profitFactor.toFixed(2).padStart(6)}
最小最大亏损   | ${bestByMaxLoss.label.padEnd(20)} | ${String(bestByMaxLoss.tradeCount).padStart(6)} | ${bestByMaxLoss.winRate.toFixed(1).padStart(5)}% | ${(bestByMaxLoss.totalPnL >= 0 ? '+' : '') + bestByMaxLoss.totalPnL.toFixed(2).padStart(7)}% | ${bestByMaxLoss.maxLoss.toFixed(2).padStart(7)}% | ${bestByMaxLoss.profitFactor.toFixed(2).padStart(6)}
`);

// 最优组合的止盈原因统计
console.log("最优组合止盈原因统计:");
const reasonStats = {};
for (const t of bestByReturn.trades) {
  if (!reasonStats[t.reason]) reasonStats[t.reason] = { count: 0, pnl: 0, wins: 0 };
  reasonStats[t.reason].count++;
  reasonStats[t.reason].pnl += t.pnl;
  if (t.pnl > 0) reasonStats[t.reason].wins++;
}
console.log("原因              | 次数 | 胜率   | 总收益");
console.log("-".repeat(50));
for (const [reason, s] of Object.entries(reasonStats)) {
  console.log(`${reason.padEnd(18)} | ${String(s.count).padStart(4)} | ${(s.wins/s.count*100).toFixed(1).padStart(5)}% | ${(s.pnl >= 0 ? '+' : '') + s.pnl.toFixed(2).padStart(7)}%`);
}

console.log("\n测试完成！");
