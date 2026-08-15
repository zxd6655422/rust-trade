/**
 * 第十次分析: MA288交叉止损策略
 *
 * 止损逻辑:
 * 多头止损: open > MA288 且 close < MA288 (价格跌破MA288)
 * 空头止损: open < MA288 且 close > MA288 (价格突破MA288)
 *
 * 分别测试5m和30m
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
console.log("第十次分析: MA288交叉止损策略");
console.log("=".repeat(70));

const df_5m = loadCSV('kline_5m_202607222208.csv', 'open_time');
const df_30m = loadCSV('kline_30m_202607222207.csv', 'open_time');

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
  for (let i = 99; i < df.length; i++) {
    let sumSq = 0;
    for (let j = i - 99; j <= i; j++) sumSq += (closes[j] - bbMid[i]) ** 2;
    const std = Math.sqrt(sumSq / 100);
    const bbUpper = bbMid[i] + 2 * std;
    const bbLower = bbMid[i] - 2 * std;
    bbWidth[i] = (bbUpper - bbLower) / bbMid[i] * 100;
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

  for (let i = 0; i < df.length; i++) {
    df[i][`${prefix}ma48`] = ma48[i];
    df[i][`${prefix}ma288`] = ma288[i];
    df[i][`${prefix}ma488`] = ma488[i];
    df[i][`${prefix}bbWidth`] = bbWidth[i];
    df[i][`${prefix}ma288Slope`] = ma288Slope[i];
    df[i][`${prefix}volRatio`] = volRatio[i];
  }
  return df;
}

console.log("\n计算技术指标...");
addIndicators(df_5m, 'm5_');
addIndicators(df_30m, 'm30_');

const df_5m_valid = df_5m.filter(r => r.m5_ma288 !== null && r.m5_ma488 !== null);
const df_30m_valid = df_30m.filter(r => r.m30_ma288 !== null && r.m30_ma488 !== null);

// 30m趋势索引
function build30mMap(df30) {
  const map = new Map();
  for (const r of df30) {
    map.set(r.open_time.getTime(), {
      trend: r.m30_ma288 > r.m30_ma488 ? 'bullish' : 'bearish'
    });
  }
  return map;
}

const map30m = build30mMap(df_30m_valid);

function get30mTrendAt(time) {
  const t = time.getTime();
  let best = null;
  let bestDiff = Infinity;
  for (const [ts, data] of map30m) {
    const diff = t - ts;
    if (diff >= 0 && diff < bestDiff) {
      bestDiff = diff;
      best = data;
    }
    if (diff < 0) break;
  }
  return best;
}

// ============================================================
// 策略回测: MA288交叉止损
// ============================================================
function runStrategy(df, config) {
  const {
    prefix = 'm5_',           // 'm5_' 或 'm30_'
    slopeThreshold = 0,
    bbwThreshold = 0,
    volThreshold = 0,
    filter30mEnabled = false,  // 只对5m有效
    // 止盈
    tpMode = 'trailing',       // none, trailing
    trailingActivate = 3.0,
    trailingCallback = 3.0,
    // 止损模式
    stopMode = 'ma288',        // 'fixed', 'ma288'
    fixedStopPct = 2.0,
    // 是否在趋势反转时平仓
    trendReversalExit = true,
  } = config;

  const ma288Key = `${prefix}ma288`;
  const ma488Key = `${prefix}ma488`;
  const slopeKey = `${prefix}ma288Slope`;
  const bbwKey = `${prefix}bbWidth`;
  const volKey = `${prefix}volRatio`;

  let position = null;
  let entryPrice = 0, entryTime = null;
  let maxProfitPct = 0;
  let totalPnL = 0, winCount = 0, lossCount = 0;
  let maxWin = 0, maxLoss = 0;
  const trades = [];

  for (let i = 1; i < df.length; i++) {
    const row = df[i];
    const ma288 = row[ma288Key];
    const ma488 = row[ma488Key];
    const o = row.open, c = row.close;
    const slope = row[slopeKey];
    const bbw = row[bbwKey];
    const volRatio = row[volKey];

    let trend;
    if (ma288 < ma488) trend = 'bearish';
    else if (ma288 > ma488) trend = 'bullish';
    else continue;

    // 过滤
    if (slopeThreshold > 0 && slope !== null && Math.abs(slope) < slopeThreshold) continue;
    if (bbwThreshold > 0 && bbw !== null && bbw < bbwThreshold) continue;
    if (volThreshold > 0 && volRatio !== null && volRatio < volThreshold) continue;

    // 30m过滤(仅5m策略)
    if (filter30mEnabled && prefix === 'm5_') {
      const data30m = get30mTrendAt(row.open_time);
      if (data30m && data30m.trend !== trend) continue;
    }

    // === 持仓中的止盈止损 ===
    if (position !== null) {
      const currentPnl = position === 'long'
        ? (c - entryPrice) / entryPrice * 100
        : (entryPrice - c) / entryPrice * 100;
      maxProfitPct = Math.max(maxProfitPct, currentPnl);

      // 止损检查
      let shouldStop = false;
      let stopReason = '';

      if (stopMode === 'fixed') {
        // 固定百分比止损
        if (currentPnl < -fixedStopPct) {
          shouldStop = true;
          stopReason = 'FIXED_STOP';
        }
      } else if (stopMode === 'ma288') {
        // MA288交叉止损
        if (position === 'long' && o > ma288 && c < ma288) {
          shouldStop = true;
          stopReason = 'MA288_STOP';
        } else if (position === 'short' && o < ma288 && c > ma288) {
          shouldStop = true;
          stopReason = 'MA288_STOP';
        }
      }

      if (shouldStop) {
        totalPnL += currentPnl;
        if (currentPnl > 0) { winCount++; maxWin = Math.max(maxWin, currentPnl); }
        else { lossCount++; maxLoss = Math.min(maxLoss, currentPnl); }
        trades.push({ pnl: currentPnl, reason: stopReason });
        position = null;
        continue;
      }

      // 移动止盈
      if (tpMode === 'trailing' && maxProfitPct >= trailingActivate) {
        const drawdown = maxProfitPct - currentPnl;
        if (drawdown >= trailingCallback) {
          totalPnL += currentPnl;
          if (currentPnl > 0) { winCount++; maxWin = Math.max(maxWin, currentPnl); }
          else { lossCount++; maxLoss = Math.min(maxLoss, currentPnl); }
          trades.push({ pnl: currentPnl, reason: 'TRAILING_TP' });
          position = null;
          continue;
        }
      }

      // 趋势反转平仓
      if (trendReversalExit) {
        if (position === 'long' && trend === 'bearish' && o > ma288 && c < ma288) {
          totalPnL += currentPnl;
          if (currentPnl > 0) { winCount++; maxWin = Math.max(maxWin, currentPnl); }
          else { lossCount++; maxLoss = Math.min(maxLoss, currentPnl); }
          trades.push({ pnl: currentPnl, reason: 'TREND_REV' });
          position = null;
          continue;
        } else if (position === 'short' && trend === 'bullish' && o < ma288 && c > ma288) {
          totalPnL += currentPnl;
          if (currentPnl > 0) { winCount++; maxWin = Math.max(maxWin, currentPnl); }
          else { lossCount++; maxLoss = Math.min(maxLoss, currentPnl); }
          trades.push({ pnl: currentPnl, reason: 'TREND_REV' });
          position = null;
          continue;
        }
      }
    }

    // === 入场信号 ===
    let isEntry = false;
    let entryDir = '';

    if (trend === 'bullish' && o < ma288 && c > ma288) {
      isEntry = true;
      entryDir = 'long';
    } else if (trend === 'bearish' && o > ma288 && c < ma288) {
      isEntry = true;
      entryDir = 'short';
    }

    if (isEntry) {
      // 平掉反向持仓
      if (position !== null && position !== entryDir) {
        const pnl = position === 'long'
          ? (c - entryPrice) / entryPrice * 100
          : (entryPrice - c) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
        else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ pnl, reason: 'REVERSE' });
      }

      position = entryDir;
      entryPrice = c;
      entryTime = row.open_time;
      maxProfitPct = 0;
    }
  }

  return {
    tradeCount: trades.length,
    winCount, lossCount,
    winRate: trades.length > 0 ? (winCount / trades.length * 100) : 0,
    totalPnL,
    avgPnL: trades.length > 0 ? (totalPnL / trades.length) : 0,
    maxWin, maxLoss,
    profitFactor: maxLoss !== 0 ? (maxWin / Math.abs(maxLoss)) : 0,
    trades
  };
}

// ============================================================
// 测试1: 30m策略 - 止损方式对比
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试1: 30m策略 - 止损方式对比】");
console.log("=".repeat(70));

console.log("\n止损方式        | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 最大亏  | 盈亏比");
console.log("-".repeat(95));

const m30Configs = [
  { label: '固定2%', stop: 'fixed', pct: 2.0, tp: 'trailing', act: 3, cb: 3 },
  { label: '固定1.5%', stop: 'fixed', pct: 1.5, tp: 'trailing', act: 3, cb: 3 },
  { label: '固定3%', stop: 'fixed', pct: 3.0, tp: 'trailing', act: 3, cb: 3 },
  { label: 'MA288交叉', stop: 'ma288', tp: 'trailing', act: 3, cb: 3 },
  { label: 'MA288+移动(2+2)', stop: 'ma288', tp: 'trailing', act: 2, cb: 2 },
  { label: 'MA288+移动(3+3)', stop: 'ma288', tp: 'trailing', act: 3, cb: 3 },
  { label: 'MA288+移动(5+3)', stop: 'ma288', tp: 'trailing', act: 5, cb: 3 },
  { label: 'MA288+移动(5+5)', stop: 'ma288', tp: 'trailing', act: 5, cb: 5 },
  { label: 'MA288无止盈', stop: 'ma288', tp: 'none' },
];

const m30Results = [];
for (const cfg of m30Configs) {
  const r = runStrategy(df_30m_valid, {
    prefix: 'm30_',
    stopMode: cfg.stop,
    fixedStopPct: cfg.pct || 2.0,
    tpMode: cfg.tp,
    trailingActivate: cfg.act || 3,
    trailingCallback: cfg.cb || 3,
    trendReversalExit: true,
  });
  m30Results.push({ label: cfg.label, ...r });
  console.log(
    `${cfg.label.padEnd(16)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}% | ` +
    `${r.maxWin.toFixed(2).padStart(7)}% | ${r.maxLoss.toFixed(2).padStart(7)}% | ${r.profitFactor.toFixed(2).padStart(6)}`
  );
}

// ============================================================
// 测试2: 5m策略 - 止损方式对比
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试2: 5m策略 - 止损方式对比】");
console.log("=".repeat(70));

console.log("\n止损方式        | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 最大亏  | 盈亏比");
console.log("-".repeat(95));

const m5Configs = [
  { label: '固定2%', stop: 'fixed', pct: 2.0, tp: 'trailing', act: 2, cb: 1.5, f30m: true },
  { label: '固定1.5%', stop: 'fixed', pct: 1.5, tp: 'trailing', act: 2, cb: 1.5, f30m: true },
  { label: 'MA288交叉', stop: 'ma288', tp: 'trailing', act: 2, cb: 1.5, f30m: true },
  { label: 'MA288+移动(1.5+1)', stop: 'ma288', tp: 'trailing', act: 1.5, cb: 1.0, f30m: true },
  { label: 'MA288+移动(2+1.5)', stop: 'ma288', tp: 'trailing', act: 2, cb: 1.5, f30m: true },
  { label: 'MA288+移动(2+2)', stop: 'ma288', tp: 'trailing', act: 2, cb: 2, f30m: true },
  { label: 'MA288+移动(3+3)', stop: 'ma288', tp: 'trailing', act: 3, cb: 3, f30m: true },
  { label: 'MA288无止盈', stop: 'ma288', tp: 'none', f30m: true },
  { label: 'MA288(无30m过滤)', stop: 'ma288', tp: 'trailing', act: 2, cb: 1.5, f30m: false },
];

const m5Results = [];
for (const cfg of m5Configs) {
  const r = runStrategy(df_5m_valid, {
    prefix: 'm5_',
    stopMode: cfg.stop,
    fixedStopPct: cfg.pct || 2.0,
    tpMode: cfg.tp,
    trailingActivate: cfg.act || 2,
    trailingCallback: cfg.cb || 1.5,
    filter30mEnabled: cfg.f30m,
    trendReversalExit: true,
  });
  m5Results.push({ label: cfg.label, ...r });
  console.log(
    `${cfg.label.padEnd(22)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}% | ` +
    `${r.maxWin.toFixed(2).padStart(7)}% | ${r.maxLoss.toFixed(2).padStart(7)}% | ${r.profitFactor.toFixed(2).padStart(6)}`
  );
}

// ============================================================
// 测试3: 最优配置样本内/样本外
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试3: 样本内/样本外检验】");
console.log("=".repeat(70));

// 30m最优
const best30m = m30Results.reduce((a, b) => a.totalPnL > b.totalPnL ? a : b);
console.log(`\n30m最优: ${best30m.label} (${best30m.totalPnL.toFixed(2)}%)`);

// 5m最优
const best5m = m5Results.reduce((a, b) => a.totalPnL > b.totalPnL ? a : b);
console.log(`5m最优: ${best5m.label} (${best5m.totalPnL.toFixed(2)}%)`);

// 样本内/样本外
function sampleTest(df, config, label) {
  const splitIdx = Math.floor(df.length * 0.7);
  const train = df.slice(0, splitIdx);
  const test = df.slice(splitIdx);

  const trainR = runStrategy(train, config);
  const testR = runStrategy(test, config);
  const fullR = runStrategy(df, config);

  const decay = trainR.totalPnL !== 0
    ? ((testR.totalPnL - trainR.totalPnL) / Math.abs(trainR.totalPnL) * 100)
    : 0;

  return { label, trainR, testR, fullR, decay };
}

console.log("\n--- 30m策略 ---");
const test30m = sampleTest(df_30m_valid, {
  prefix: 'm30_',
  stopMode: 'ma288',
  tpMode: 'trailing',
  trailingActivate: 3,
  trailingCallback: 3,
  trendReversalExit: true,
}, '30m MA288止损');

console.log(`训练集: ${test30m.trainR.tradeCount}笔, 胜率${test30m.trainR.winRate.toFixed(1)}%, 收益${test30m.trainR.totalPnL.toFixed(2)}%`);
console.log(`测试集: ${test30m.testR.tradeCount}笔, 胜率${test30m.testR.winRate.toFixed(1)}%, 收益${test30m.testR.totalPnL.toFixed(2)}%`);
console.log(`衰减: ${test30m.decay.toFixed(1)}% ${test30m.decay < -50 ? '⚠' : '✅'}`);

console.log("\n--- 5m策略 ---");
const test5m = sampleTest(df_5m_valid, {
  prefix: 'm5_',
  stopMode: 'ma288',
  tpMode: 'trailing',
  trailingActivate: 2,
  trailingCallback: 1.5,
  filter30mEnabled: true,
  trendReversalExit: true,
}, '5m MA288止损');

console.log(`训练集: ${test5m.trainR.tradeCount}笔, 胜率${test5m.trainR.winRate.toFixed(1)}%, 收益${test5m.trainR.totalPnL.toFixed(2)}%`);
console.log(`测试集: ${test5m.testR.tradeCount}笔, 胜率${test5m.testR.winRate.toFixed(1)}%, 收益${test5m.testR.totalPnL.toFixed(2)}%`);
console.log(`衰减: ${test5m.decay.toFixed(1)}% ${test5m.decay < -50 ? '⚠' : '✅'}`);

// ============================================================
// 最优配置详细分析
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【最优配置详细分析】");
console.log("=".repeat(70));

console.log(`\n=== 30m最优: ${best30m.label} ===`);
console.log(`  交易数: ${best30m.tradeCount}`);
console.log(`  胜率: ${best30m.winRate.toFixed(1)}%`);
console.log(`  总收益: ${best30m.totalPnL.toFixed(2)}%`);
console.log(`  平均收益: ${best30m.avgPnL.toFixed(3)}%`);
console.log(`  最大盈利: ${best30m.maxWin.toFixed(2)}%`);
console.log(`  最大亏损: ${best30m.maxLoss.toFixed(2)}%`);
console.log(`  盈亏比: ${best30m.profitFactor.toFixed(2)}`);

console.log("\n出场类型:");
const type30 = {};
for (const t of best30m.trades) type30[t.reason] = (type30[t.reason] || 0) + 1;
for (const [type, count] of Object.entries(type30).sort((a,b) => b[1]-a[1])) {
  const avg = best30m.trades.filter(t => t.reason === type).reduce((s,t) => s+t.pnl, 0) / count;
  console.log(`  ${type.padEnd(15)}: ${count}次, 平均${avg >= 0 ? '+' : ''}${avg.toFixed(3)}%`);
}

console.log(`\n=== 5m最优: ${best5m.label} ===`);
console.log(`  交易数: ${best5m.tradeCount}`);
console.log(`  胜率: ${best5m.winRate.toFixed(1)}%`);
console.log(`  总收益: ${best5m.totalPnL.toFixed(2)}%`);
console.log(`  平均收益: ${best5m.avgPnL.toFixed(3)}%`);
console.log(`  最大盈利: ${best5m.maxWin.toFixed(2)}%`);
console.log(`  最大亏损: ${best5m.maxLoss.toFixed(2)}%`);
console.log(`  盈亏比: ${best5m.profitFactor.toFixed(2)}`);

console.log("\n出场类型:");
const type5 = {};
for (const t of best5m.trades) type5[t.reason] = (type5[t.reason] || 0) + 1;
for (const [type, count] of Object.entries(type5).sort((a,b) => b[1]-a[1])) {
  const avg = best5m.trades.filter(t => t.reason === type).reduce((s,t) => s+t.pnl, 0) / count;
  console.log(`  ${type.padEnd(15)}: ${count}次, 平均${avg >= 0 ? '+' : ''}${avg.toFixed(3)}%`);
}

// ============================================================
// 最终对比
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【最终对比: 固定止损 vs MA288交叉止损】");
console.log("=".repeat(70));

console.log(`
30m策略:
止损方式        | 交易数 | 胜率   | 总收益   | 平均收益 | 盈亏比
----------------|--------|--------|----------|----------|-------
固定2%          |     52 |  28.8% | + 54.39% | + 1.046% |  3.92
MA288交叉       | ${String(best30m.tradeCount).padStart(6)} | ${best30m.winRate.toFixed(1).padStart(5)}% | ${(best30m.totalPnL >= 0 ? '+' : '') + best30m.totalPnL.toFixed(2).padStart(7)}% | ${(best30m.avgPnL >= 0 ? '+' : '') + best30m.avgPnL.toFixed(3).padStart(7)}% | ${best30m.profitFactor.toFixed(2).padStart(5)}

5m策略:
止损方式        | 交易数 | 胜率   | 总收益   | 平均收益 | 盈亏比
----------------|--------|--------|----------|----------|-------
固定2%          |     36 |  41.7% | - 15.03% | - 0.417% |  1.07
MA288交叉       | ${String(best5m.tradeCount).padStart(6)} | ${best5m.winRate.toFixed(1).padStart(5)}% | ${(best5m.totalPnL >= 0 ? '+' : '') + best5m.totalPnL.toFixed(2).padStart(7)}% | ${(best5m.avgPnL >= 0 ? '+' : '') + best5m.avgPnL.toFixed(3).padStart(7)}% | ${best5m.profitFactor.toFixed(2).padStart(5)}
`);

console.log("第十次分析完成！");
