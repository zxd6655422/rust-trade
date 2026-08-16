/**
 * ETH验证: 第十一次分析_深度优化
 * 使用ETH数据验证策略通用性
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
console.log("ETH验证: 第十一次分析_深度优化");
console.log("=".repeat(70));

const df_5m = loadCSV('kline_5m_202607232006.csv', 'open_time');
const df_30m = loadCSV('kline_30m_202607232006.csv', 'open_time');

console.log(`ETH数据量: 5m=${df_5m.length}条, 30m=${df_30m.length}条`);

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
  }
  return df;
}

console.log("\n计算技术指标...");
addIndicators(df_5m, 'm5_');
addIndicators(df_30m, 'm30_');

const df_5m_valid = df_5m.filter(r => r.m5_ma288 !== null && r.m5_ma488 !== null);
const df_30m_valid = df_30m.filter(r => r.m30_ma288 !== null && r.m30_ma488 !== null);

console.log(`有效数据: 5m=${df_5m_valid.length}条, 30m=${df_30m_valid.length}条`);

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
    if (diff >= 0 && diff < bestDiff) { bestDiff = diff; best = data; }
    if (diff < 0) break;
  }
  return best;
}

// ============================================================
// 策略回测: 30m + 固定止损
// ============================================================
function run30mFixedStop(df, config) {
  const {
    stopLossPct = 2.0,
    stopMode = 'fixed',
    tpMode = 'trailing',
    trailingActivate = 5.0,
    trailingCallback = 5.0,
    bbTpPct = 90,
    bbTpEnabled = false,
    ma48TpEnabled = false,
    ma48TpBars = 2,
    slopeThreshold = 5,
    bbwThreshold = 2.0,
    volThreshold = 0.6,
  } = config;

  let position = null;
  let entryPrice = 0;
  let maxProfitPct = 0;
  let ma48CrossCount = 0;
  let totalPnL = 0, winCount = 0, lossCount = 0;
  let maxWin = 0, maxLoss = 0;
  const trades = [];

  for (let i = 1; i < df.length; i++) {
    const row = df[i];
    const {m30_ma288: ma288, m30_ma488: ma488, m30_ma48: ma48, open: o, close: c,
           m30_ma288Slope: slope, m30_bbWidth: bbw, m30_volRatio: volRatio,
           m30_bbUpper: bbUpper, m30_bbLower: bbLower, m30_bbPos: bbPos} = row;

    let trend;
    if (ma288 < ma488) trend = 'bearish';
    else if (ma288 > ma488) trend = 'bullish';
    else continue;

    if (slopeThreshold > 0 && slope !== null && Math.abs(slope) < slopeThreshold) continue;
    if (bbwThreshold > 0 && bbw !== null && bbw < bbwThreshold) continue;
    if (volThreshold > 0 && volRatio !== null && volRatio < volThreshold) continue;

    if (position !== null) {
      const currentPnl = position === 'long'
        ? (c - entryPrice) / entryPrice * 100
        : (entryPrice - c) / entryPrice * 100;
      maxProfitPct = Math.max(maxProfitPct, currentPnl);

      let shouldStop = false;
      if (stopMode === 'fixed' && currentPnl < -stopLossPct) shouldStop = true;
      else if (stopMode === 'ma288') {
        if (position === 'long' && o > ma288 && c < ma288) shouldStop = true;
        else if (position === 'short' && o < ma288 && c > ma288) shouldStop = true;
      }

      if (shouldStop) {
        totalPnL += currentPnl;
        if (currentPnl > 0) { winCount++; maxWin = Math.max(maxWin, currentPnl); }
        else { lossCount++; maxLoss = Math.min(maxLoss, currentPnl); }
        trades.push({ pnl: currentPnl, reason: stopMode === 'fixed' ? 'FIXED_STOP' : 'MA288_STOP' });
        position = null;
        continue;
      }

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
    }

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
        maxProfitPct = 0;
        ma48CrossCount = 0;
      }
      // 只在无持仓时开新仓
      if (position === null) {
        position = entryDir;
        entryPrice = c;
        maxProfitPct = 0;
        ma48CrossCount = 0;
      }
    }

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
// 策略回测: 5m + MA288止损
// ============================================================
function run5mBB(df, config) {
  const {
    slopeThreshold = 0,
    bbwThreshold = 0,
    volThreshold = 0,
    filter30m = true,
    bbEntryEnabled = false,
    bbEntryLongMax = 50,
    bbEntryShortMin = 50,
    tpMode = 'trailing',
    trailingActivate = 1.5,
    trailingCallback = 1.0,
    bbTpEnabled = false,
    bbTpPct = 90,
  } = config;

  let position = null;
  let entryPrice = 0;
  let maxProfitPct = 0;
  let totalPnL = 0, winCount = 0, lossCount = 0;
  let maxWin = 0, maxLoss = 0;
  const trades = [];

  for (let i = 1; i < df.length; i++) {
    const row = df[i];
    const {m5_ma288: ma288, m5_ma488: ma488, open: o, close: c,
           m5_ma288Slope: slope, m5_bbWidth: bbw, m5_volRatio: volRatio,
           m5_bbPos: bbPos} = row;

    let trend5m;
    if (ma288 < ma488) trend5m = 'bearish';
    else if (ma288 > ma488) trend5m = 'bullish';
    else continue;

    if (slopeThreshold > 0 && slope !== null && Math.abs(slope) < slopeThreshold) continue;
    if (bbwThreshold > 0 && bbw !== null && bbw < bbwThreshold) continue;
    if (volThreshold > 0 && volRatio !== null && volRatio < volThreshold) continue;

    if (filter30m) {
      const data30m = get30mTrendAt(row.open_time);
      if (data30m && data30m.trend !== trend5m) continue;
    }

    if (position !== null) {
      const currentPnl = position === 'long'
        ? (c - entryPrice) / entryPrice * 100
        : (entryPrice - c) / entryPrice * 100;
      maxProfitPct = Math.max(maxProfitPct, currentPnl);

      if (position === 'long' && o > ma288 && c < ma288) {
        totalPnL += currentPnl;
        if (currentPnl > 0) { winCount++; maxWin = Math.max(maxWin, currentPnl); }
        else { lossCount++; maxLoss = Math.min(maxLoss, currentPnl); }
        trades.push({ pnl: currentPnl, reason: 'MA288_STOP' });
        position = null;
        continue;
      }
      if (position === 'short' && o < ma288 && c > ma288) {
        totalPnL += currentPnl;
        if (currentPnl > 0) { winCount++; maxWin = Math.max(maxWin, currentPnl); }
        else { lossCount++; maxLoss = Math.min(maxLoss, currentPnl); }
        trades.push({ pnl: currentPnl, reason: 'MA288_STOP' });
        position = null;
        continue;
      }

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
    }

    let isEntry = false;
    let entryDir = '';
    if (trend5m === 'bullish' && o < ma288 && c > ma288) {
      if (bbEntryEnabled && bbPos !== null && bbPos > bbEntryLongMax) continue;
      isEntry = true; entryDir = 'long';
    } else if (trend5m === 'bearish' && o > ma288 && c < ma288) {
      if (bbEntryEnabled && bbPos !== null && bbPos < bbEntryShortMin) continue;
      isEntry = true; entryDir = 'short';
    }

    if (isEntry) {
      // 如果有反向持仓，先平仓（不开新仓）
      if (position !== null && position !== entryDir) {
        const pnl = position === 'long' ? (c - entryPrice) / entryPrice * 100 : (entryPrice - c) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); } else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ pnl, reason: 'REVERSE_CLOSE' });
        position = null;
        entryPrice = 0;
        maxProfitPct = 0;
      }
      // 只在无持仓时开新仓
      if (position === null) {
        position = entryDir;
        entryPrice = c;
        maxProfitPct = 0;
      }
    }

    if (position === 'long' && trend5m === 'bearish' && o > ma288 && c < ma288) {
      const pnl = (c - entryPrice) / entryPrice * 100;
      totalPnL += pnl;
      if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); } else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
      trades.push({ pnl, reason: 'TREND_REV' });
      position = null;
    } else if (position === 'short' && trend5m === 'bullish' && o < ma288 && c > ma288) {
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
// 测试1: 30m + 固定止损 + 止盈优化
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试1: 30m + 固定止损 + 止盈优化】");
console.log("=".repeat(70));

console.log("\n止盈方式              | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 盈亏比");
console.log("-".repeat(90));

const m30FixedConfigs = [
  { label: '基准(移动5+5)', tp: 'trailing', act: 5, cb: 5, bb: false, ma48: false },
  { label: '移动(8+5)', tp: 'trailing', act: 8, cb: 5, bb: false, ma48: false },
  { label: '移动(10+5)', tp: 'trailing', act: 10, cb: 5, bb: false, ma48: false },
  { label: '移动(10+8)', tp: 'trailing', act: 10, cb: 8, bb: false, ma48: false },
  { label: 'BB止盈(90%)', tp: 'trailing', act: 5, cb: 5, bb: true, bbPct: 90 },
  { label: 'BB止盈(95%)', tp: 'trailing', act: 5, cb: 5, bb: true, bbPct: 95 },
  { label: 'MA48止盈(2根)', tp: 'trailing', act: 5, cb: 5, bb: false, ma48: true, ma48Bars: 2 },
  { label: 'MA48止盈(3根)', tp: 'trailing', act: 5, cb: 5, bb: false, ma48: true, ma48Bars: 3 },
  { label: 'BB+MA48', tp: 'trailing', act: 5, cb: 5, bb: true, bbPct: 90, ma48: true, ma48Bars: 2 },
  { label: 'BB+移动(8+5)', tp: 'trailing', act: 8, cb: 5, bb: true, bbPct: 90 },
];

const m30FixedResults = [];
for (const cfg of m30FixedConfigs) {
  const r = run30mFixedStop(df_30m_valid, {
    stopLossPct: 2.0,
    tpMode: cfg.tp,
    trailingActivate: cfg.act,
    trailingCallback: cfg.cb,
    bbTpEnabled: cfg.bb,
    bbTpPct: cfg.bbPct || 90,
    ma48TpEnabled: cfg.ma48,
    ma48TpBars: cfg.ma48Bars || 2,
    slopeThreshold: 5,
    bbwThreshold: 2.0,
    volThreshold: 0.6,
  });
  m30FixedResults.push({ label: cfg.label, ...r });
  console.log(
    `${cfg.label.padEnd(20)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}% | ` +
    `${r.maxWin.toFixed(2).padStart(7)}% | ${r.profitFactor.toFixed(2).padStart(6)}`
  );
}

// ============================================================
// 测试2: 30m + MA288止损 + 止盈优化
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试2: 30m + MA288止损 + 止盈优化】");
console.log("=".repeat(70));

console.log("\n止盈方式              | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 盈亏比");
console.log("-".repeat(90));

const m30MA288Configs = [
  { label: '基准(移动5+5)', tp: 'trailing', act: 5, cb: 5, bb: false, ma48: false },
  { label: '移动(8+5)', tp: 'trailing', act: 8, cb: 5, bb: false, ma48: false },
  { label: '移动(10+8)', tp: 'trailing', act: 10, cb: 8, bb: false, ma48: false },
  { label: 'BB止盈(90%)', tp: 'trailing', act: 5, cb: 5, bb: true, bbPct: 90 },
  { label: 'BB止盈(95%)', tp: 'trailing', act: 5, cb: 5, bb: true, bbPct: 95 },
  { label: 'MA48止盈(3根)', tp: 'trailing', act: 5, cb: 5, bb: false, ma48: true, ma48Bars: 3 },
  { label: 'BB+MA48', tp: 'trailing', act: 5, cb: 5, bb: true, bbPct: 90, ma48: true, ma48Bars: 3 },
  { label: 'BB+移动(8+5)', tp: 'trailing', act: 8, cb: 5, bb: true, bbPct: 90 },
  { label: '无止盈', tp: 'none' },
];

const m30MA288Results = [];
for (const cfg of m30MA288Configs) {
  const r = run30mFixedStop(df_30m_valid, {
    stopMode: 'ma288',
    tpMode: cfg.tp,
    trailingActivate: cfg.act || 5,
    trailingCallback: cfg.cb || 5,
    bbTpEnabled: cfg.bb,
    bbTpPct: cfg.bbPct || 90,
    ma48TpEnabled: cfg.ma48,
    ma48TpBars: cfg.ma48Bars || 3,
    slopeThreshold: 0,
    bbwThreshold: 0,
    volThreshold: 0,
  });
  m30MA288Results.push({ label: cfg.label, ...r });
  console.log(
    `${cfg.label.padEnd(20)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}% | ` +
    `${r.maxWin.toFixed(2).padStart(7)}% | ${r.profitFactor.toFixed(2).padStart(6)}`
  );
}

// ============================================================
// 测试3: 5m + MA288止损 + 布林带优化
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试3: 5m + MA288止损 + 布林带优化】");
console.log("=".repeat(70));

console.log("\n配置                    | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 盈亏比");
console.log("-".repeat(90));

const m5BBConfigs = [
  { label: '基准(移动1.5+1)', bbEntry: false, bbTp: false, bbw: 0, tp: 'trailing', act: 1.5, cb: 1 },
  { label: '+BB入场过滤', bbEntry: true, bbLongMax: 50, bbShortMin: 50, bbTp: false, tp: 'trailing', act: 1.5, cb: 1 },
  { label: '+BB入场(40/60)', bbEntry: true, bbLongMax: 40, bbShortMin: 60, bbTp: false, tp: 'trailing', act: 1.5, cb: 1 },
  { label: '+BB止盈(90%)', bbEntry: false, bbTp: true, bbTpPct: 90, tp: 'trailing', act: 1.5, cb: 1 },
  { label: '+BB止盈(85%)', bbEntry: false, bbTp: true, bbTpPct: 85, tp: 'trailing', act: 1.5, cb: 1 },
  { label: '+BBW>1', bbEntry: false, bbTp: false, bbw: 1, tp: 'trailing', act: 1.5, cb: 1 },
  { label: '+BBW>1.5', bbEntry: false, bbTp: false, bbw: 1.5, tp: 'trailing', act: 1.5, cb: 1 },
  { label: '+BB入场+BBW>1', bbEntry: true, bbLongMax: 50, bbShortMin: 50, bbw: 1, tp: 'trailing', act: 1.5, cb: 1 },
  { label: '+BB入场+止盈', bbEntry: true, bbLongMax: 50, bbShortMin: 50, bbTp: true, bbTpPct: 90, tp: 'trailing', act: 1.5, cb: 1 },
  { label: '全配置', bbEntry: true, bbLongMax: 40, bbShortMin: 60, bbw: 1, bbTp: true, bbTpPct: 85, tp: 'trailing', act: 1.5, cb: 1 },
];

const m5BBResults = [];
for (const cfg of m5BBConfigs) {
  const r = run5mBB(df_5m_valid, {
    slopeThreshold: 0,
    bbwThreshold: cfg.bbw || 0,
    volThreshold: 0,
    filter30m: true,
    bbEntryEnabled: cfg.bbEntry,
    bbEntryLongMax: cfg.bbLongMax || 50,
    bbEntryShortMin: cfg.bbShortMin || 50,
    tpMode: cfg.tp,
    trailingActivate: cfg.act,
    trailingCallback: cfg.cb,
    bbTpEnabled: cfg.bbTp,
    bbTpPct: cfg.bbTpPct || 90,
  });
  m5BBResults.push({ label: cfg.label, ...r });
  console.log(
    `${cfg.label.padEnd(23)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}% | ` +
    `${r.maxWin.toFixed(2).padStart(7)}% | ${r.profitFactor.toFixed(2).padStart(6)}`
  );
}

// ============================================================
// 最优配置详细分析
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【ETH最优配置详细分析】");
console.log("=".repeat(70));

const best30mFixed = m30FixedResults.reduce((a, b) => a.totalPnL > b.totalPnL ? a : b);
const best30mMA288 = m30MA288Results.reduce((a, b) => a.totalPnL > b.totalPnL ? a : b);
const best5mBB = m5BBResults.reduce((a, b) => a.totalPnL > b.totalPnL ? a : b);

console.log(`\n30m+固定止损最优: ${best30mFixed.label}`);
console.log(`  交易数: ${best30mFixed.tradeCount}, 胜率: ${best30mFixed.winRate.toFixed(1)}%, 总收益: ${best30mFixed.totalPnL.toFixed(2)}%, 盈亏比: ${best30mFixed.profitFactor.toFixed(2)}`);

console.log(`\n30m+MA288止损最优: ${best30mMA288.label}`);
console.log(`  交易数: ${best30mMA288.tradeCount}, 胜率: ${best30mMA288.winRate.toFixed(1)}%, 总收益: ${best30mMA288.totalPnL.toFixed(2)}%, 盈亏比: ${best30mMA288.profitFactor.toFixed(2)}`);

console.log(`\n5m+MA288止损最优: ${best5mBB.label}`);
console.log(`  交易数: ${best5mBB.tradeCount}, 胜率: ${best5mBB.winRate.toFixed(1)}%, 总收益: ${best5mBB.totalPnL.toFixed(2)}%, 盈亏比: ${best5mBB.profitFactor.toFixed(2)}`);

// ============================================================
// ETH vs BTC 对比
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【ETH vs BTC 策略对比】");
console.log("=".repeat(70));

console.log(`
策略                    | 币种 | 交易数 | 胜率   | 总收益   | 平均收益 | 盈亏比
------------------------|------|--------|--------|----------|----------|-------
30m+固定止损(最优)      | BTC  |     50 |  50.0% | + 14.37% | + 0.287% |   2.04
30m+固定止损(最优)      | ETH  | ${String(best30mFixed.tradeCount).padStart(6)} | ${best30mFixed.winRate.toFixed(1).padStart(5)}% | ${(best30mFixed.totalPnL >= 0 ? '+' : '') + best30mFixed.totalPnL.toFixed(2).padStart(7)}% | ${(best30mFixed.avgPnL >= 0 ? '+' : '') + best30mFixed.avgPnL.toFixed(3).padStart(7)}% | ${best30mFixed.profitFactor.toFixed(2).padStart(5)}

30m+MA288止损(最优)     | BTC  |    402 |  15.2% | + 42.79% | + 0.106% |  10.75
30m+MA288止损(最优)     | ETH  | ${String(best30mMA288.tradeCount).padStart(6)} | ${best30mMA288.winRate.toFixed(1).padStart(5)}% | ${(best30mMA288.totalPnL >= 0 ? '+' : '') + best30mMA288.totalPnL.toFixed(2).padStart(7)}% | ${(best30mMA288.avgPnL >= 0 ? '+' : '') + best30mMA288.avgPnL.toFixed(3).padStart(7)}% | ${best30mMA288.profitFactor.toFixed(2).padStart(5)}

5m+MA288止损(最优)      | BTC  |    416 |  16.8% | +  8.86% | + 0.021% |   1.09
5m+MA288止损(最优)      | ETH  | ${String(best5mBB.tradeCount).padStart(6)} | ${best5mBB.winRate.toFixed(1).padStart(5)}% | ${(best5mBB.totalPnL >= 0 ? '+' : '') + best5mBB.totalPnL.toFixed(2).padStart(7)}% | ${(best5mBB.avgPnL >= 0 ? '+' : '') + best5mBB.avgPnL.toFixed(3).padStart(7)}% | ${best5mBB.profitFactor.toFixed(2).padStart(5)}
`);

console.log("ETH验证完成！");
