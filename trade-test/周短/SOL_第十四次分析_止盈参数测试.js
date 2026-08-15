/**
 * SOL 第十四次分析: 止盈参数测试
 *
 * 基于第十三次SOL最优结果 + 硬止损2%
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
console.log("SOL 第十四次分析: 止盈参数测试");
console.log("=".repeat(70));

const df_5m = loadCSV('../kline_5m_202608010054_SOLUSDT.csv', 'open_time');
const df_30m = loadCSV('../kline_30m_202608010054_SOLUSDT.csv', 'open_time');

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
  const ma288Slope = new Array(df.length).fill(null);
  for (let i = 5; i < df.length; i++) {
    if (ma288[i] !== null && ma288[i-5] !== null && ma288[i-5] !== 0) {
      ma288Slope[i] = (ma288[i] - ma288[i-5]) / ma288[i-5] * 10000;
    }
  }
  const bbMid = calcMA(100);
  const bbWidth = new Array(df.length).fill(null);
  const bbPos = new Array(df.length).fill(null);
  for (let i = 99; i < df.length; i++) {
    let sumSq = 0;
    for (let j = i - 99; j <= i; j++) sumSq += (closes[j] - bbMid[i]) ** 2;
    const std = Math.sqrt(sumSq / 100);
    const upper = bbMid[i] + 2 * std;
    const lower = bbMid[i] - 2 * std;
    bbWidth[i] = (upper - lower) / bbMid[i] * 100;
    bbPos[i] = (closes[i] - lower) / (upper - lower) * 100;
  }
  const volMA = new Array(df.length).fill(null);
  const volRatio = new Array(df.length).fill(null);
  for (let i = 19; i < df.length; i++) {
    let sum = 0;
    for (let j = i - 19; j <= i; j++) sum += volumes[j];
    volMA[i] = sum / 20;
    if (volMA[i] > 0) volRatio[i] = volumes[i] / volMA[i];
  }
  // 计算30m扩散指标
  const spread = new Array(df.length).fill(null);
  const spreadDelta = new Array(df.length).fill(null);
  const isExpanding = new Array(df.length).fill(null);
  for (let i = 0; i < df.length; i++) {
    if (ma288[i] !== null && ma488[i] !== null) spread[i] = ma288[i] - ma488[i];
  }
  const anglePeriod = 5;
  for (let i = anglePeriod; i < df.length; i++) {
    if (spread[i] !== null && spread[i - anglePeriod] !== null) {
      spreadDelta[i] = spread[i] - spread[i - anglePeriod];
      isExpanding[i] = Math.abs(spread[i]) > Math.abs(spread[i - anglePeriod]);
    }
  }
  for (let i = 0; i < df.length; i++) {
    df[i][`${prefix}ma48`] = ma48[i];
    df[i][`${prefix}ma288`] = ma288[i];
    df[i][`${prefix}ma488`] = ma488[i];
    df[i][`${prefix}ma288Slope`] = ma288Slope[i];
    df[i][`${prefix}bbWidth`] = bbWidth[i];
    df[i][`${prefix}bbPos`] = bbPos[i];
    df[i][`${prefix}volRatio`] = volRatio[i];
    df[i][`${prefix}spread`] = spread[i];
    df[i][`${prefix}spreadDelta`] = spreadDelta[i];
    df[i][`${prefix}isExpanding`] = isExpanding[i];
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
    map.set(r.open_time.getTime(), { isExpanding: r.m30_isExpanding });
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
    map.set(r.open_time.getTime(), { isExpanding: r.m5_isExpanding !== undefined ? r.m5_isExpanding : true });
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
    slopeThreshold = 0, bbwThreshold = 0, volThreshold = 0,
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
    const slope = row.m30_ma288Slope, bbw = row.m30_bbWidth, volRatio = row.m30_volRatio;

    // 趋势方向判断（支持多空双向）
    const trend = ma288 > ma488 ? 'bullish' : (ma288 < ma488 ? 'bearish' : null);
    if (!trend) continue;

    if (slopeThreshold > 0 && slope !== null && Math.abs(slope) < slopeThreshold) continue;
    if (bbwThreshold > 0 && bbw !== null && bbw < bbwThreshold) continue;
    if (volThreshold > 0 && volRatio !== null && volRatio < volThreshold) continue;

    // 30m扩散过滤 (直接用当前行的数据)
    if (use30mExpanding && row.m30_isExpanding === false) continue;

    // 5m扩散过滤
    if (use5mExpanding) {
      const data5m = get5mAt(row.open_time);
      if (data5m && !data5m.isExpanding) continue;
    }

    if (position !== null) {
      const currentPnl = position === 'long' ? (c - entryPrice) / entryPrice * 100 : (entryPrice - c) / entryPrice * 100;
      maxProfitPct = Math.max(maxProfitPct, currentPnl);

      let shouldStop = false, exitPrice = c;

      // 硬止损
      if (useHardStop) {
        if (position === 'long' && l <= hardStopPrice) { shouldStop = true; exitPrice = hardStopPrice; }
        else if (position === 'short' && h >= hardStopPrice) { shouldStop = true; exitPrice = hardStopPrice; }
      }

      // MA288止损
      if (!shouldStop) {
        if (position === 'long' && o > ma288 && c < ma288) { shouldStop = true; }
        else if (position === 'short' && o < ma288 && c > ma288) { shouldStop = true; }
      }

      if (shouldStop) {
        const pnl = position === 'long' ? (exitPrice - entryPrice) / entryPrice * 100 : (entryPrice - exitPrice) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); } else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ pnl, side: position });
        position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0;
        continue;
      }

      if (tpMode === 'trailing' && maxProfitPct >= trailingActivate) {
        if (maxProfitPct - currentPnl >= trailingCallback) {
          totalPnL += currentPnl;
          if (currentPnl > 0) { winCount++; maxWin = Math.max(maxWin, currentPnl); } else { lossCount++; maxLoss = Math.min(maxLoss, currentPnl); }
          trades.push({ pnl: currentPnl, side: position });
          position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0;
          continue;
        }
      }

      // 趋势反转退出
      if (position === 'long' && trend === 'bearish' && o > ma288 && c < ma288) {
        const pnl = (c - entryPrice) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); } else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ pnl, side: 'long' });
        position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0;
        continue;
      } else if (position === 'short' && trend === 'bullish' && o < ma288 && c > ma288) {
        const pnl = (entryPrice - c) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); } else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ pnl, side: 'short' });
        position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0;
        continue;
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
        trades.push({ pnl, side: position });
        position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0;
      }
      if (position === null) {
        position = entryDir; entryPrice = c; maxProfitPct = 0;
        hardStopPrice = entryDir === 'long' ? entryPrice * (1 - hardStopPct / 100) : entryPrice * (1 + hardStopPct / 100);
      }
    }
  }

  // 分别统计多空
  const longTrades = trades.filter(t => t.side === 'long');
  const shortTrades = trades.filter(t => t.side === 'short');
  const longPnL = longTrades.reduce((sum, t) => sum + t.pnl, 0);
  const shortPnL = shortTrades.reduce((sum, t) => sum + t.pnl, 0);
  const longWins = longTrades.filter(t => t.pnl > 0).length;
  const shortWins = shortTrades.filter(t => t.pnl > 0).length;

  return {
    tradeCount: trades.length, winCount, lossCount,
    winRate: trades.length > 0 ? (winCount / trades.length * 100) : 0,
    totalPnL, avgPnL: trades.length > 0 ? (totalPnL / trades.length) : 0,
    maxWin, maxLoss,
    profitFactor: maxLoss !== 0 ? (maxWin / Math.abs(maxLoss)) : 0,
    longCount: longTrades.length, longPnL, longWinRate: longTrades.length > 0 ? (longWins / longTrades.length * 100) : 0,
    shortCount: shortTrades.length, shortPnL, shortWinRate: shortTrades.length > 0 ? (shortWins / shortTrades.length * 100) : 0,
  };
}

// ============================================================
// 测试1: trailingActivate 变化
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试1: trailingActivate 变化 (固定 callback=5%)】");
console.log("=".repeat(70));

console.log("\nactivate | callback | 交易数 | 胜率   | 总收益   | 平均收益 | 最大亏  | 盈亏比");
console.log("-".repeat(90));

const activateTests = [1, 2, 3, 4, 5, 6, 8, 10, 15, 20];
const test1Results = [];

for (const act of activateTests) {
  const r = runStrategy(df_30m_valid, {
    useHardStop: true, hardStopPct: 2.5,
    tpMode: 'trailing', trailingActivate: act, trailingCallback: 5,
  });
  test1Results.push({ activate: act, ...r });
  console.log(
    `${String(act).padStart(4)}%    |    5%    | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}% | ` +
    `${r.maxLoss.toFixed(2).padStart(7)}% | ${r.profitFactor.toFixed(2).padStart(6)}`
  );
}

// ============================================================
// 测试2: trailingCallback 变化
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试2: trailingCallback 变化 (固定 activate=5%)】");
console.log("=".repeat(70));

console.log("\nactivate | callback | 交易数 | 胜率   | 总收益   | 平均收益 | 最大亏  | 盈亏比");
console.log("-".repeat(90));

const callbackTests = [1, 2, 3, 4, 5, 6, 8, 10];
const test2Results = [];

for (const cb of callbackTests) {
  const r = runStrategy(df_30m_valid, {
    useHardStop: true, hardStopPct: 2.5,
    tpMode: 'trailing', trailingActivate: 5, trailingCallback: cb,
  });
  test2Results.push({ callback: cb, ...r });
  console.log(
    `   5%    | ${String(cb).padStart(4)}%     | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}% | ` +
    `${r.maxLoss.toFixed(2).padStart(7)}% | ${r.profitFactor.toFixed(2).padStart(6)}`
  );
}

// ============================================================
// 测试3: 组合矩阵
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试3: activate × callback 组合矩阵】");
console.log("=".repeat(70));

const matrixActivates = [2, 3, 4, 5, 6, 8, 10];
const matrixCallbacks = [1, 2, 3, 4, 5, 6, 8, 10];

let header = "activate\\cb |";
for (const cb of matrixCallbacks) header += `  ${cb}%    |`;
console.log("\n" + header);
console.log("-".repeat(header.length));

const matrixResults = [];

for (const act of matrixActivates) {
  let row = `    ${String(act).padStart(2)}%     |`;
  for (const cb of matrixCallbacks) {
    const r = runStrategy(df_30m_valid, {
      useHardStop: true, hardStopPct: 2.5,
      tpMode: 'trailing', trailingActivate: act, trailingCallback: cb,
    });
    matrixResults.push({ activate: act, callback: cb, ...r });
    row += `${r.totalPnL >= 0 ? '+' : ''}${r.totalPnL.toFixed(1).padStart(5)}% |`;
  }
  console.log(row);
}

// ============================================================
// 最优组合
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【最优组合】");
console.log("=".repeat(70));

const allResults = [...test1Results.map(r => ({...r, activate: r.activate, callback: 5})),
                     ...test2Results.map(r => ({...r, activate: 5, callback: r.callback})),
                     ...matrixResults];
const bestByReturn = allResults.reduce((a, b) => a.totalPnL > b.totalPnL ? a : b);

console.log(`
最优配置: activate=${bestByReturn.activate}%, callback=${bestByReturn.callback}%
交易数: ${bestByReturn.tradeCount}
胜率: ${bestByReturn.winRate.toFixed(1)}%
总收益: ${bestByReturn.totalPnL >= 0 ? '+' : ''}${bestByReturn.totalPnL.toFixed(2)}%
最大亏损: ${bestByReturn.maxLoss.toFixed(2)}%
盈亏比: ${bestByReturn.profitFactor.toFixed(2)}

--- 多空分析 ---
做多: ${bestByReturn.longCount}笔, 收益=${bestByReturn.longPnL >= 0 ? '+' : ''}${bestByReturn.longPnL.toFixed(2)}%, 胜率=${bestByReturn.longWinRate.toFixed(1)}%
做空: ${bestByReturn.shortCount}笔, 收益=${bestByReturn.shortPnL >= 0 ? '+' : ''}${bestByReturn.shortPnL.toFixed(2)}%, 胜率=${bestByReturn.shortWinRate.toFixed(1)}%
`);

// ============================================================
// 验证信号: 2026-07-30 14:35:13 的做空信号是否正确
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【验证信号: 2026-07-30 14:35 的做空信号】");
console.log("=".repeat(70));

const signalTime = new Date('2026-07-30T14:35:13+08:00');
const signalPrice = 73.58;
const ma288Val = 73.59684027777783;
const ma488Val = 73.65122950819668;

console.log(`\n信号时间: ${signalTime.toISOString()}`);
console.log(`入场价: ${signalPrice}`);
console.log(`MA288: ${ma288Val}`);
console.log(`MA488: ${ma488Val}`);
console.log(`趋势: ${ma288Val < ma488Val ? 'bearish (MA288 < MA488)' : 'bullish'}`);

// 找到信号时间对应的30m K线
console.log("\n--- 30m K线验证 ---");
const target30m = df_30m.filter(r => {
  const t = r.open_time.getTime();
  // 14:00 开始的30m K线覆盖 14:00-14:30
  // 14:30 开始的30m K线覆盖 14:30-15:00
  return t >= new Date('2026-07-30T13:30:00+08:00').getTime() &&
         t <= new Date('2026-07-30T15:00:00+08:00').getTime();
});

for (const r of target30m) {
  const o = r.open, c = r.close;
  const isEntry = ma288Val < ma488Val && o > ma288Val && c < ma288Val;
  console.log(`${r.open_time.toISOString()}: O=${o}, H=${r.high}, L=${r.low}, C=${c}, ` +
    `O>MA288(${o > ma288Val}), C<MA288(${c < ma288Val}), 入场条件=${isEntry ? '✅满足' : '❌不满足'}`);
}

// 找到信号时间对应的5m K线
console.log("\n--- 5m K线验证 ---");
const target5m = df_5m.filter(r => {
  const t = r.open_time.getTime();
  return t >= new Date('2026-07-30T14:00:00+08:00').getTime() &&
         t <= new Date('2026-07-30T15:00:00+08:00').getTime();
});

for (const r of target5m) {
  const o = r.open, c = r.close;
  const isEntry = ma288Val < ma488Val && o > ma288Val && c < ma288Val;
  console.log(`${r.open_time.toISOString()}: O=${o}, C=${c}, ` +
    `O>MA288(${o > ma288Val}), C<MA288(${c < ma288Val}), 入场条件=${isEntry ? '✅满足' : '❌不满足'}`);
}

// 检查30m扩散
console.log("\n--- 30m扩散检查 ---");
const m30AtSignal = get30mAt(signalTime);
console.log(`30m扩散状态: ${m30AtSignal ? (m30AtSignal.isExpanding ? '✅正在扩散' : '❌未扩散') : '无数据'}`);

// 检查5m扩散
console.log("\n--- 5m扩散检查 ---");
const m5AtSignal = get5mAt(signalTime);
console.log(`5m扩散状态: ${m5AtSignal ? (m5AtSignal.isExpanding ? '✅正在扩散' : '❌未扩散') : '无数据'}`);

// 结论
console.log("\n--- 结论 ---");
const trendOK = ma288Val < ma488Val;
console.log(`1. 趋势方向: ${trendOK ? '✅ MA288 < MA488 = bearish' : '❌'}`);

// 检查14:00-14:30的30m K线
const candle1400 = df_30m.find(r => r.open_time.getTime() === new Date('2026-07-30T14:00:00+08:00').getTime());
if (candle1400) {
  const entryOK = candle1400.open > ma288Val && candle1400.close < ma288Val;
  console.log(`2. 30m入场条件 (14:00 K线): O=${candle1400.open}>MA288=${candle1400.open > ma288Val}, C=${candle1400.close}<MA288=${candle1400.close < ma288Val} → ${entryOK ? '✅满足' : '❌不满足'}`);
}

// 检查14:30的30m K线
const candle1430 = df_30m.find(r => r.open_time.getTime() === new Date('2026-07-30T14:30:00+08:00').getTime());
if (candle1430) {
  const entryOK = candle1430.open > ma288Val && candle1430.close < ma288Val;
  console.log(`3. 30m入场条件 (14:30 K线): O=${candle1430.open}>MA288=${candle1430.open > ma288Val}, C=${candle1430.close}<MA288=${candle1430.close < ma288Val} → ${entryOK ? '✅满足' : '❌不满足'}`);
}

console.log(`4. 30m扩散: ${m30AtSignal ? (m30AtSignal.isExpanding ? '✅' : '❌') : '❓'}`);
console.log(`5. 5m扩散: ${m5AtSignal ? (m5AtSignal.isExpanding ? '✅' : '❌') : '❓'}`);

console.log("\nSOL 信号验证完成！");
