/**
 * ETH 第十四次分析: 双层止损 (硬止损 + MA288趋势止损)
 *
 * 基于第十三次ETH最优结果:
 * - 30m入场 + MA288止损 + 5m扩散过滤
 * - slope=5, bbw=2, vol=0.6
 * - 移动止盈(5+5)
 *
 * 第十四次优化:
 * 1. 双层止损: 硬止损(2%) + MA288趋势止损
 * 2. 止损只平仓，不开反向单
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
console.log("ETH 第十四次分析: 双层止损 (硬止损 + MA288趋势止损)");
console.log("=".repeat(70));

const df_5m = loadCSV('../kline_5m_202607232006.csv', 'open_time');
const df_30m = loadCSV('../kline_30m_202607232006.csv', 'open_time');

// ============================================================
// 计算指标
// ============================================================
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
    df[i][`${prefix}bbWidth`] = bbWidth[i];
    df[i][`${prefix}bbPos`] = bbPos[i];
    df[i][`${prefix}ma288Slope`] = ma288Slope[i];
    df[i][`${prefix}volRatio`] = volRatio[i];
    df[i][`${prefix}spread`] = spread[i];
    df[i][`${prefix}spreadDelta`] = spreadDelta[i];
    df[i][`${prefix}isExpanding`] = isExpanding[i];
  }
  return df;
}

console.log("\n计算技术指标...");
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
// 策略回测: 双层止损
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
    use30mExpanding = false,
    minAngle5m = 0,
    entryTimeframe = '30m',
  } = config;

  let position = null;
  let entryPrice = 0, entryTime = null, hardStopPrice = 0;
  let maxProfitPct = 0;
  let totalPnL = 0, winCount = 0, lossCount = 0;
  let maxWin = 0, maxLoss = 0;
  const trades = [];

  for (let i = 1; i < df.length; i++) {
    const row = df[i];
    const ma288 = row.m30_ma288;
    const ma488 = row.m30_ma488;
    const ma48 = row.m30_ma48;
    const o = row.open, h = row.high, l = row.low, c = row.close;
    const slope = row.m30_ma288Slope;
    const bbw = row.m30_bbWidth;
    const volRatio = row.m30_volRatio;
    const bbPos = row.m30_bbPos;

    let trend;
    if (ma288 < ma488) trend = 'bearish';
    else if (ma288 > ma488) trend = 'bullish';
    else continue;

    // 入场过滤
    if (slopeThreshold > 0 && slope !== null && Math.abs(slope) < slopeThreshold) continue;
    if (bbwThreshold > 0 && bbw !== null && bbw < bbwThreshold) continue;
    if (volThreshold > 0 && volRatio !== null && volRatio < volThreshold) continue;

    // 30m扩散过滤
    if (use30mExpanding && row.m30_isExpanding === false) continue;

    // 5m扩散过滤
    if (use5mExpanding) {
      const data5m = get5mAt(row.open_time);
      if (data5m && !data5m.isExpanding) continue;
    }

    if (position !== null) {
      const currentPnl = position === 'long' ? (c - entryPrice) / entryPrice * 100 : (entryPrice - c) / entryPrice * 100;
      maxProfitPct = Math.max(maxProfitPct, currentPnl);

      let shouldStop = false, stopReason = '', exitPrice = c;

      // 硬止损
      if (useHardStop) {
        if (position === 'long' && l <= hardStopPrice) { shouldStop = true; stopReason = 'HARD_STOP'; exitPrice = hardStopPrice; }
        else if (position === 'short' && h >= hardStopPrice) { shouldStop = true; stopReason = 'HARD_STOP'; exitPrice = hardStopPrice; }
      }

      // MA288止损
      if (!shouldStop) {
        if (position === 'long' && o > ma288 && c < ma288) { shouldStop = true; stopReason = 'MA288_STOP'; }
        else if (position === 'short' && o < ma288 && c > ma288) { shouldStop = true; stopReason = 'MA288_STOP'; }
      }

      if (shouldStop) {
        const pnl = position === 'long' ? (exitPrice - entryPrice) / entryPrice * 100 : (entryPrice - exitPrice) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); } else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ pnl, reason: stopReason });
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

    // 入场
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
        maxProfitPct = 0;
        if (useHardStop) {
          hardStopPrice = entryDir === 'long' ? entryPrice * (1 - hardStopPct / 100) : entryPrice * (1 + hardStopPct / 100);
        }
      }
    }

    // 趋势反转平仓
    if (position === 'long' && trend === 'bearish' && o > ma288 && c < ma288) {
      const pnl = (c - entryPrice) / entryPrice * 100;
      totalPnL += pnl;
      if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); } else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
      trades.push({ pnl, reason: 'TREND_REV' });
      position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0; ma48CrossCount = 0;
    } else if (position === 'short' && trend === 'bullish' && o < ma288 && c > ma288) {
      const pnl = (entryPrice - c) / entryPrice * 100;
      totalPnL += pnl;
      if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); } else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
      trades.push({ pnl, reason: 'TREND_REV' });
      position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0; ma48CrossCount = 0;
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
// 测试1: 硬止损百分比对比
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试1: 硬止损百分比对比 (合约场景)】");
console.log("=".repeat(70));

console.log("\n配置                      | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 最大亏  | 盈亏比");
console.log("-".repeat(105));

const test1Configs = [
  { label: '基准(无硬止损)', hardStop: false },
  { label: '硬止损1.0%', hardStop: true, pct: 1.0 },
  { label: '硬止损1.5%', hardStop: true, pct: 1.5 },
  { label: '硬止损2.0%', hardStop: true, pct: 2.0 },
  { label: '硬止损2.5%', hardStop: true, pct: 2.5 },
  { label: '硬止损3.0%', hardStop: true, pct: 3.0 },
  { label: '硬止损5.0%', hardStop: true, pct: 5.0 },
];

const test1Results = [];
for (const cfg of test1Configs) {
  const r = runStrategy(df_30m_valid, {
    useHardStop: cfg.hardStop,
    hardStopPct: cfg.pct || 2.0,
    stopMode: 'ma288',
    tpMode: 'trailing',
    trailingActivate: 5,
    trailingCallback: 5,
    slopeThreshold: 0,
    bbwThreshold: 0,
    volThreshold: 0,
    use5mExpanding: true,
    use30mExpanding: true,
    entryTimeframe: '30m',
  });
  test1Results.push({ label: cfg.label, ...r });
  console.log(
    `${cfg.label.padEnd(25)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}% | ` +
    `${r.maxWin.toFixed(2).padStart(7)}% | ${r.maxLoss.toFixed(2).padStart(7)}% | ${r.profitFactor.toFixed(2).padStart(6)}`
  );
}

// ============================================================
// 测试2: 硬止损触发统计
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试2: 硬止损触发统计分析】");
console.log("=".repeat(70));

const test2Result = runStrategy(df_30m_valid, {
  useHardStop: true,
  hardStopPct: 2.0,
  stopMode: 'ma288',
  tpMode: 'trailing',
  trailingActivate: 5,
  trailingCallback: 5,
  slopeThreshold: 5,
  bbwThreshold: 2.0,
  volThreshold: 0.6,
  use5mExpanding: true,
  entryTimeframe: '30m',
});

const stopReasons = {};
for (const t of test2Result.trades) {
  if (!stopReasons[t.reason]) stopReasons[t.reason] = { count: 0, totalPnl: 0, wins: 0, losses: 0 };
  stopReasons[t.reason].count++;
  stopReasons[t.reason].totalPnl += t.pnl;
  if (t.pnl > 0) stopReasons[t.reason].wins++;
  else stopReasons[t.reason].losses++;
}

console.log("\n止损/止盈原因统计:");
console.log("原因              | 次数 | 胜率   | 总收益   | 平均收益");
console.log("-".repeat(65));
for (const [reason, stats] of Object.entries(stopReasons)) {
  const winRate = stats.count > 0 ? (stats.wins / stats.count * 100) : 0;
  const avgPnl = stats.count > 0 ? (stats.totalPnl / stats.count) : 0;
  console.log(
    `${reason.padEnd(18)} | ${String(stats.count).padStart(4)} | ${winRate.toFixed(1).padStart(5)}% | ` +
    `${(stats.totalPnl >= 0 ? '+' : '') + stats.totalPnl.toFixed(2).padStart(7)}% | ${(avgPnl >= 0 ? '+' : '') + avgPnl.toFixed(3).padStart(7)}%`
  );
}

// ============================================================
// 最终对比
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【最终对比: ETH 第十四次(双层止损) vs 第十三次】");
console.log("=".repeat(70));

const bestTest1 = test1Results.reduce((a, b) => a.totalPnL > b.totalPnL ? a : b);
const noHardStop = test1Results.find(r => r.label.includes('无硬止损'));

console.log(`
策略                         | 交易数 | 胜率   | 总收益   | 最大亏  | 盈亏比
-----------------------------|--------|--------|----------|---------|-------
第十三次(基准)                | 参考   | 参考   | +68.17%  |  参考   |  参考
${noHardStop.label.padEnd(28)} | ${String(noHardStop.tradeCount).padStart(6)} | ${noHardStop.winRate.toFixed(1).padStart(5)}% | ${(noHardStop.totalPnL >= 0 ? '+' : '') + noHardStop.totalPnL.toFixed(2).padStart(7)}% | ${noHardStop.maxLoss.toFixed(2).padStart(7)}% | ${noHardStop.profitFactor.toFixed(2).padStart(6)}
${bestTest1.label.padEnd(28)} | ${String(bestTest1.tradeCount).padStart(6)} | ${bestTest1.winRate.toFixed(1).padStart(5)}% | ${(bestTest1.totalPnL >= 0 ? '+' : '') + bestTest1.totalPnL.toFixed(2).padStart(7)}% | ${bestTest1.maxLoss.toFixed(2).padStart(7)}% | ${bestTest1.profitFactor.toFixed(2).padStart(6)}
`);

if (noHardStop && bestTest1) {
  const maxLossImproved = Math.abs(noHardStop.maxLoss) - Math.abs(bestTest1.maxLoss);
  console.log(`硬止损保护效果:`);
  console.log(`  最大单笔亏损改善: ${maxLossImproved > 0 ? '+' : ''}${maxLossImproved.toFixed(2)}%`);
}

console.log("\nETH 第十四次分析完成！");
