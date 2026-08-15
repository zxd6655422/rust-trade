/**
 * 第十三次分析: 30m趋势策略 + 5m扩散过滤
 *
 * 基础策略不变 (来自第十一次):
 * - 30m: ma288 > ma488 = 多头趋势
 * - 入场: 价格穿越MA288 (open < ma288 && close > ma288)
 * - 止损: MA288交叉 (open > ma288 && close < ma288)
 * - 止盈: 移动止盈 / BB止盈 / MA48止盈
 *
 * 新增优化:
 * - 5m双均线扩散形态作为额外过滤条件
 * - 测试不同扩散参数对策略的影响
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
console.log("SOL 第十三次分析: 30m趋势策略 + 5m扩散过滤");
console.log("=".repeat(70));

const df_5m = loadCSV('kline_5m_202607232011.csv', 'open_time');
const df_30m = loadCSV('kline_30m_202607232010.csv', 'open_time');

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

  // 布林带 (100, 2)
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

  // 双均线扩散指标
  const spread = new Array(df.length).fill(null);
  const spreadDelta = new Array(df.length).fill(null);
  const isExpanding = new Array(df.length).fill(null);
  const angleApprox = new Array(df.length).fill(null);
  for (let i = 0; i < df.length; i++) {
    if (ma288[i] !== null && ma488[i] !== null) {
      spread[i] = ma288[i] - ma488[i];
    }
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
    df[i][`${prefix}bbMid`] = bbMid[i];
    df[i][`${prefix}bbUpper`] = bbUpper[i];
    df[i][`${prefix}bbLower`] = bbLower[i];
    df[i][`${prefix}bbWidth`] = bbWidth[i];
    df[i][`${prefix}bbPos`] = bbPos[i];
    df[i][`${prefix}ma288Slope`] = ma288Slope[i];
    df[i][`${prefix}volRatio`] = volRatio[i];
    df[i][`${prefix}spread`] = spread[i];
    df[i][`${prefix}spreadDelta`] = spreadDelta[i];
    df[i][`${prefix}isExpanding`] = isExpanding[i];
    df[i][`${prefix}angleApprox`] = angleApprox[i];
  }
  return df;
}

console.log("\n计算技术指标...");
addIndicators(df_5m, 'm5_');
addIndicators(df_30m, 'm30_');

const df_5m_valid = df_5m.filter(r => r.m5_ma288 !== null && r.m5_ma488 !== null);
const df_30m_valid = df_30m.filter(r => r.m30_ma288 !== null && r.m30_ma488 !== null);

// ============================================================
// 30m趋势索引
// ============================================================
function build30mMap(df30) {
  const map = new Map();
  for (const r of df30) {
    map.set(r.open_time.getTime(), {
      trend: r.m30_ma288 > r.m30_ma488 ? 'bullish' : 'bearish',
      isExpanding: r.m30_isExpanding,
      angleApprox: r.m30_angleApprox,
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

// 5m数据索引 (用于30m入场时查找5m扩散状态)
function build5mMap(df5) {
  const map = new Map();
  for (const r of df5) {
    map.set(r.open_time.getTime(), {
      isExpanding: r.m5_isExpanding,
      angleApprox: r.m5_angleApprox,
    });
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
// 策略回测: 30m趋势 + 5m入场 + 5m扩散过滤 (基础策略来自第十一次)
// ============================================================
function runStrategy(df, config) {
  const {
    stopLossPct = 2.0,
    stopMode = 'ma288',        // 'fixed' 或 'ma288'
    tpMode = 'trailing',
    trailingActivate = 5.0,
    trailingCallback = 5.0,
    bbTpPct = 90,
    bbTpEnabled = false,
    ma48TpEnabled = false,
    ma48TpBars = 2,
    // 入场过滤 (第十一次的参数)
    slopeThreshold = 5,
    bbwThreshold = 2.0,
    volThreshold = 0.6,
    // 新增: 5m扩散过滤
    use5mExpanding = false,
    use30mExpanding = false,
    minAngle5m = 0,
    minAngle30m = 0,
    // 入场周期
    entryTimeframe = '30m',    // '30m' 或 '5m'
  } = config;

  let position = null;
  let entryPrice = 0, entryTime = null;
  let maxProfitPct = 0;
  let ma48CrossCount = 0;
  let totalPnL = 0, winCount = 0, lossCount = 0;
  let maxWin = 0, maxLoss = 0;
  const trades = [];

  // 选择入场数据源
  const dfEntry = entryTimeframe === '5m' ? df_5m_valid : df;
  const prefix = entryTimeframe === '5m' ? 'm5_' : 'm30_';

  for (let i = 1; i < dfEntry.length; i++) {
    const row = dfEntry[i];
    const ma288 = row[`${prefix}ma288`];
    const ma488 = row[`${prefix}ma488`];
    const ma48 = row[`${prefix}ma48`];
    const o = row.open;
    const c = row.close;
    const slope = row[`${prefix}ma288Slope`];
    const bbw = row[`${prefix}bbWidth`];
    const volRatio = row[`${prefix}volRatio`];
    const bbPos = row[`${prefix}bbPos`];

    // 30m趋势
    let trend;
    let expanding30m = null;
    if (entryTimeframe === '5m') {
      const data30m = get30mAt(row.open_time);
      if (!data30m) continue;
      trend = data30m.trend;
      expanding30m = data30m.isExpanding;
    } else {
      if (ma288 < ma488) trend = 'bearish';
      else if (ma288 > ma488) trend = 'bullish';
      else continue;
      // 30m入场时，30m扩散数据就是当前行的数据
      expanding30m = row.m30_isExpanding;
    }

    // 30m扩散过滤
    if (use30mExpanding && expanding30m !== null && !expanding30m) continue;

    // 5m扩散过滤 (30m入场时需要查找对应时间点的5m数据)
    if (use5mExpanding) {
      let expanding5m = null;
      if (entryTimeframe === '5m') {
        expanding5m = row.m5_isExpanding;
      } else {
        const data5m = get5mAt(row.open_time);
        if (data5m) expanding5m = data5m.isExpanding;
      }
      if (expanding5m !== null && !expanding5m) continue;
    }

    if (minAngle5m > 0) {
      let angle5m = null;
      if (entryTimeframe === '5m') {
        angle5m = row.m5_angleApprox;
      } else {
        const data5m = get5mAt(row.open_time);
        if (data5m) angle5m = data5m.angleApprox;
      }
      if (angle5m !== null && Math.abs(angle5m) < minAngle5m) continue;
    }

    // 入场过滤 (第十一次的参数)
    if (slopeThreshold > 0 && slope !== null && Math.abs(slope) < slopeThreshold) continue;
    if (bbwThreshold > 0 && bbw !== null && bbw < bbwThreshold) continue;
    if (volThreshold > 0 && volRatio !== null && volRatio < volThreshold) continue;

    // 持仓止盈止损
    if (position !== null) {
      const currentPnl = position === 'long'
        ? (c - entryPrice) / entryPrice * 100
        : (entryPrice - c) / entryPrice * 100;
      maxProfitPct = Math.max(maxProfitPct, currentPnl);

      let shouldStop = false;
      let stopReason = '';

      if (stopMode === 'fixed') {
        if (currentPnl < -stopLossPct) {
          shouldStop = true;
          stopReason = 'FIXED_STOP';
        }
      } else if (stopMode === 'ma288') {
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

      // BB止盈
      if (bbTpEnabled && bbPos !== null) {
        if (position === 'long' && bbPos >= bbTpPct) {
          totalPnL += currentPnl;
          if (currentPnl > 0) { winCount++; maxWin = Math.max(maxWin, currentPnl); }
          else { lossCount++; maxLoss = Math.min(maxLoss, currentPnl); }
          trades.push({ pnl: currentPnl, reason: 'BB_TP' });
          position = null;
          continue;
        }
        if (position === 'short' && bbPos <= (100 - bbTpPct)) {
          totalPnL += currentPnl;
          if (currentPnl > 0) { winCount++; maxWin = Math.max(maxWin, currentPnl); }
          else { lossCount++; maxLoss = Math.min(maxLoss, currentPnl); }
          trades.push({ pnl: currentPnl, reason: 'BB_TP' });
          position = null;
          continue;
        }
      }

      // MA48止盈
      if (ma48TpEnabled && ma48 !== null) {
        if (position === 'long' && c < ma48) {
          ma48CrossCount++;
          if (ma48CrossCount >= ma48TpBars) {
            totalPnL += currentPnl;
            if (currentPnl > 0) { winCount++; maxWin = Math.max(maxWin, currentPnl); }
            else { lossCount++; maxLoss = Math.min(maxLoss, currentPnl); }
            trades.push({ pnl: currentPnl, reason: 'MA48_TP' });
            position = null;
            continue;
          }
        } else if (position === 'short' && c > ma48) {
          ma48CrossCount++;
          if (ma48CrossCount >= ma48TpBars) {
            totalPnL += currentPnl;
            if (currentPnl > 0) { winCount++; maxWin = Math.max(maxWin, currentPnl); }
            else { lossCount++; maxLoss = Math.min(maxLoss, currentPnl); }
            trades.push({ pnl: currentPnl, reason: 'MA48_TP' });
            position = null;
            continue;
          }
        } else {
          ma48CrossCount = 0;
        }
      }

      // 移动止盈
      if (tpMode === 'trailing' || tpMode === 'bb_trailing') {
        if (maxProfitPct >= trailingActivate) {
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
      }
    }

    // 入场 (不反手，只平仓)
    let isEntry = false;
    let entryDir = '';
    if (trend === 'bullish' && o < ma288 && c > ma288) { isEntry = true; entryDir = 'long'; }
    else if (trend === 'bearish' && o > ma288 && c < ma288) { isEntry = true; entryDir = 'short'; }

    if (isEntry) {
      // 如果有反向持仓，先平仓（不开新仓）
      if (position !== null && position !== entryDir) {
        const pnl = position === 'long' ? (c - entryPrice) / entryPrice * 100 : (entryPrice - c) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); } else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ pnl, reason: 'REVERSE_CLOSE' });
        position = null;
        entryPrice = 0;
        entryTime = null;
        maxProfitPct = 0;
        ma48CrossCount = 0;
      }
      // 只在无持仓时开新仓
      if (position === null) {
        position = entryDir;
        entryPrice = c;
        entryTime = row.open_time;
        maxProfitPct = 0;
        ma48CrossCount = 0;
      }
    }

    // 趋势反转平仓
    if (position === 'long' && trend === 'bearish' && o > ma288 && c < ma288) {
      const pnl = (c - entryPrice) / entryPrice * 100;
      totalPnL += pnl;
      if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); } else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
      trades.push({ pnl, reason: 'TREND_REV' });
      position = null;
    } else if (position === 'short' && trend === 'bullish' && o < ma288 && c > ma288) {
      const pnl = (entryPrice - c) / entryPrice * 100;
      totalPnL += pnl;
      if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); } else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
      trades.push({ pnl, reason: 'TREND_REV' });
      position = null;
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
// 测试1: 30m入场 + 5m扩散过滤 (固定止损)
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试1: 30m入场 + 5m扩散过滤 (固定止损2%)】");
console.log("=".repeat(70));

console.log("\n配置                      | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 盈亏比");
console.log("-".repeat(95));

const test1Configs = [
  { label: '基准(无扩散)', use5m: false, use30m: false },
  { label: '+5m扩散过滤', use5m: true, use30m: false },
  { label: '+5m扩散+夹角>0.3', use5m: true, use30m: false, angle5m: 0.3 },
  { label: '+5m扩散+夹角>0.5', use5m: true, use30m: false, angle5m: 0.5 },
  { label: '+5m扩散+夹角>1.0', use5m: true, use30m: false, angle5m: 1.0 },
  { label: '+5m扩散+夹角>2.0', use5m: true, use30m: false, angle5m: 2.0 },
  { label: '+30m扩散过滤', use5m: false, use30m: true },
  { label: '+5m+30m双扩散', use5m: true, use30m: true },
];

const test1Results = [];
for (const cfg of test1Configs) {
  const r = runStrategy(df_30m_valid, {
    stopLossPct: 2.0,
    stopMode: 'fixed',
    tpMode: 'trailing',
    trailingActivate: 5,
    trailingCallback: 5,
    slopeThreshold: 5,
    bbwThreshold: 2.0,
    volThreshold: 0.6,
    use5mExpanding: cfg.use5m,
    use30mExpanding: cfg.use30m,
    minAngle5m: cfg.angle5m || 0,
    entryTimeframe: '30m',
  });
  test1Results.push({ label: cfg.label, ...r });
  console.log(
    `${cfg.label.padEnd(25)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}% | ` +
    `${r.maxWin.toFixed(2).padStart(7)}% | ${r.profitFactor.toFixed(2).padStart(6)}`
  );
}

// ============================================================
// 测试2: 30m入场 + 5m扩散过滤 (MA288止损)
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试2: 30m入场 + 5m扩散过滤 (MA288止损)】");
console.log("=".repeat(70));

console.log("\n配置                      | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 盈亏比");
console.log("-".repeat(95));

const test2Configs = [
  { label: '基准(无扩散)', use5m: false, use30m: false },
  { label: '+5m扩散过滤', use5m: true, use30m: false },
  { label: '+5m扩散+夹角>0.3', use5m: true, use30m: false, angle5m: 0.3 },
  { label: '+5m扩散+夹角>0.5', use5m: true, use30m: false, angle5m: 0.5 },
  { label: '+5m扩散+夹角>1.0', use5m: true, use30m: false, angle5m: 1.0 },
  { label: '+5m扩散+夹角>2.0', use5m: true, use30m: false, angle5m: 2.0 },
  { label: '+30m扩散过滤', use5m: false, use30m: true },
  { label: '+5m+30m双扩散', use5m: true, use30m: true },
];

const test2Results = [];
for (const cfg of test2Configs) {
  const r = runStrategy(df_30m_valid, {
    stopMode: 'ma288',
    tpMode: 'trailing',
    trailingActivate: 5,
    trailingCallback: 5,
    slopeThreshold: 0,
    bbwThreshold: 0,
    volThreshold: 0,
    use5mExpanding: cfg.use5m,
    use30mExpanding: cfg.use30m,
    minAngle5m: cfg.angle5m || 0,
    entryTimeframe: '30m',
  });
  test2Results.push({ label: cfg.label, ...r });
  console.log(
    `${cfg.label.padEnd(25)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}% | ` +
    `${r.maxWin.toFixed(2).padStart(7)}% | ${r.profitFactor.toFixed(2).padStart(6)}`
  );
}

// ============================================================
// 测试3: 5m入场 + 30m趋势 + 5m扩散过滤
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试3: 5m入场 + 30m趋势 + 5m扩散过滤】");
console.log("=".repeat(70));

console.log("\n配置                      | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 盈亏比");
console.log("-".repeat(95));

const test3Configs = [
  { label: '基准(无扩散)', use5m: false, use30m: false },
  { label: '+5m扩散过滤', use5m: true, use30m: false },
  { label: '+5m扩散+夹角>0.3', use5m: true, use30m: false, angle5m: 0.3 },
  { label: '+5m扩散+夹角>0.5', use5m: true, use30m: false, angle5m: 0.5 },
  { label: '+5m扩散+夹角>1.0', use5m: true, use30m: false, angle5m: 1.0 },
  { label: '+30m扩散过滤', use5m: false, use30m: true },
  { label: '+5m+30m双扩散', use5m: true, use30m: true },
];

const test3Results = [];
for (const cfg of test3Configs) {
  const r = runStrategy(df_30m_valid, {
    stopMode: 'ma288',
    tpMode: 'trailing',
    trailingActivate: 1.5,
    trailingCallback: 1.0,
    slopeThreshold: 0,
    bbwThreshold: 0,
    volThreshold: 0,
    use5mExpanding: cfg.use5m,
    use30mExpanding: cfg.use30m,
    minAngle5m: cfg.angle5m || 0,
    entryTimeframe: '5m',
  });
  test3Results.push({ label: cfg.label, ...r });
  console.log(
    `${cfg.label.padEnd(25)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}% | ` +
    `${r.maxWin.toFixed(2).padStart(7)}% | ${r.profitFactor.toFixed(2).padStart(6)}`
  );
}

// ============================================================
// 测试4: 止盈优化 (30m入场 + 5m扩散 + MA288止损)
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试4: 止盈优化 (30m入场 + 5m扩散 + MA288止损)】");
console.log("=".repeat(70));

console.log("\n止盈配置                  | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 盈亏比");
console.log("-".repeat(95));

const tpConfigs = [
  { label: '移动(5+5)', tp: 'trailing', act: 5, cb: 5 },
  { label: '移动(8+5)', tp: 'trailing', act: 8, cb: 5 },
  { label: '移动(10+5)', tp: 'trailing', act: 10, cb: 5 },
  { label: '移动(10+8)', tp: 'trailing', act: 10, cb: 8 },
  { label: 'BB止盈(90%)', tp: 'trailing', act: 5, cb: 5, bb: true, bbPct: 90 },
  { label: 'BB止盈(95%)', tp: 'trailing', act: 5, cb: 5, bb: true, bbPct: 95 },
  { label: 'MA48止盈(2根)', tp: 'trailing', act: 5, cb: 5, ma48: true, ma48Bars: 2 },
  { label: 'MA48止盈(3根)', tp: 'trailing', act: 5, cb: 5, ma48: true, ma48Bars: 3 },
  { label: 'BB+MA48', tp: 'trailing', act: 5, cb: 5, bb: true, bbPct: 90, ma48: true, ma48Bars: 2 },
];

const bestTest2 = test2Results.reduce((a, b) => a.totalPnL > b.totalPnL ? a : b);
console.log(`\n使用最优扩散配置: ${bestTest2.label}\n`);

for (const cfg of tpConfigs) {
  const r = runStrategy(df_30m_valid, {
    stopMode: 'ma288',
    tpMode: cfg.tp,
    trailingActivate: cfg.act,
    trailingCallback: cfg.cb,
    bbTpEnabled: cfg.bb || false,
    bbTpPct: cfg.bbPct || 90,
    ma48TpEnabled: cfg.ma48 || false,
    ma48TpBars: cfg.ma48Bars || 2,
    slopeThreshold: 0,
    bbwThreshold: 0,
    volThreshold: 0,
    use5mExpanding: true,
    entryTimeframe: '30m',
  });
  console.log(
    `${cfg.label.padEnd(25)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}% | ` +
    `${r.maxWin.toFixed(2).padStart(7)}% | ${r.profitFactor.toFixed(2).padStart(6)}`
  );
}

// ============================================================
// 最终对比
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【最终对比: 30m趋势+5m扩散 vs 第十一次基准】");
console.log("=".repeat(70));

const bestTest1 = test1Results.reduce((a, b) => a.totalPnL > b.totalPnL ? a : b);
const bestTest3 = test3Results.reduce((a, b) => a.totalPnL > b.totalPnL ? a : b);

console.log(`
策略                         | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 盈亏比
-----------------------------|--------|--------|----------|----------|---------|-------
30m+固定止损(第11次基准)      |     52 |  28.8% | + 54.39% | + 1.046% |  20.60% |   3.92
30m+MA288止损(第11次基准)     |    402 |  15.2% | + 42.79% | + 0.106% |  25.24% |  10.75
5m+MA288止损(第11次基准)      |    416 |  16.8% | +  8.86% | + 0.021% |   6.59% |   1.09

30m固定+5m扩散(本次最优)      | ${String(bestTest1.tradeCount).padStart(6)} | ${bestTest1.winRate.toFixed(1).padStart(5)}% | ${(bestTest1.totalPnL >= 0 ? '+' : '') + bestTest1.totalPnL.toFixed(2).padStart(7)}% | ${(bestTest1.avgPnL >= 0 ? '+' : '') + bestTest1.avgPnL.toFixed(3).padStart(7)}% | ${bestTest1.maxWin.toFixed(2).padStart(7)}% | ${bestTest1.profitFactor.toFixed(2).padStart(6)}
30m MA288+5m扩散(本次最优)    | ${String(bestTest2.tradeCount).padStart(6)} | ${bestTest2.winRate.toFixed(1).padStart(5)}% | ${(bestTest2.totalPnL >= 0 ? '+' : '') + bestTest2.totalPnL.toFixed(2).padStart(7)}% | ${(bestTest2.avgPnL >= 0 ? '+' : '') + bestTest2.avgPnL.toFixed(3).padStart(7)}% | ${bestTest2.maxWin.toFixed(2).padStart(7)}% | ${bestTest2.profitFactor.toFixed(2).padStart(6)}
5m入场+30m趋势+5m扩散(最优)   | ${String(bestTest3.tradeCount).padStart(6)} | ${bestTest3.winRate.toFixed(1).padStart(5)}% | ${(bestTest3.totalPnL >= 0 ? '+' : '') + bestTest3.totalPnL.toFixed(2).padStart(7)}% | ${(bestTest3.avgPnL >= 0 ? '+' : '') + bestTest3.avgPnL.toFixed(3).padStart(7)}% | ${bestTest3.maxWin.toFixed(2).padStart(7)}% | ${bestTest3.profitFactor.toFixed(2).padStart(6)}
`);

console.log("SOL 第十三次分析完成！");
