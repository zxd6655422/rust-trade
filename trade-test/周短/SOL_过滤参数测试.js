/**
 * SOL 过滤参数调优测试
 * 测试不同 slope/bbw/vol 组合，找到最优平衡点
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
console.log("SOL 过滤参数调优测试");
console.log("=".repeat(70));

const df_5m = loadCSV('kline_5m_202607232011.csv', 'open_time');
const df_30m = loadCSV('kline_30m_202607232010.csv', 'open_time');

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
  for (let i = 0; i < df.length; i++) {
    df[i][`${prefix}ma48`] = ma48[i];
    df[i][`${prefix}ma288`] = ma288[i];
    df[i][`${prefix}ma488`] = ma488[i];
    df[i][`${prefix}bbWidth`] = bbWidth[i];
    df[i][`${prefix}bbPos`] = bbPos[i];
    df[i][`${prefix}ma288Slope`] = ma288Slope[i];
    df[i][`${prefix}volRatio`] = volRatio[i];
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

// 计算5m扩散状态
function calc5mExpanding() {
  const expanding = new Array(df_5m_valid.length).fill(null);
  for (let i = 293; i < df_5m_valid.length; i++) {
    const fastMa = df_5m_valid.slice(i - 287, i + 1).reduce((s, r) => s + r.close, 0) / 288;
    const slowMa = df_5m_valid.slice(i - 487, i + 1).reduce((s, r) => s + r.close, 0) / 488;
    const prevFastMa = df_5m_valid.slice(i - 292, i - 4).reduce((s, r) => s + r.close, 0) / 288;
    const prevSlowMa = df_5m_valid.slice(i - 492, i - 4).reduce((s, r) => s + r.close, 0) / 488;
    const spread = fastMa - slowMa;
    const prevSpread = prevFastMa - prevSlowMa;
    expanding[i] = Math.abs(spread) > Math.abs(prevSpread);
  }
  return expanding;
}

const expanding5m = calc5mExpanding();

function runStrategy(df, config) {
  const {
    stopMode = 'ma288',
    tpMode = 'trailing',
    trailingActivate = 5.0,
    trailingCallback = 5.0,
    slopeThreshold = 0,
    bbwThreshold = 0,
    volThreshold = 0,
    use5mExpanding = false,
    entryTimeframe = '30m',
  } = config;

  let position = null;
  let entryPrice = 0, maxProfitPct = 0, ma48CrossCount = 0;
  let totalPnL = 0, winCount = 0, lossCount = 0;
  let maxWin = 0, maxLoss = 0;
  const trades = [];

  for (let i = 1; i < df.length; i++) {
    const row = df[i];
    const ma288 = row.m30_ma288;
    const ma488 = row.m30_ma488;
    const ma48 = row.m30_ma48;
    const o = row.open, c = row.close;
    const slope = row.m30_ma288Slope;
    const bbw = row.m30_bbWidth;
    const volRatio = row.m30_volRatio;

    if (ma288 < ma488) continue;
    else if (ma288 > ma488) {} else continue;

    // 入场过滤
    if (slopeThreshold > 0 && slope !== null && Math.abs(slope) < slopeThreshold) continue;
    if (bbwThreshold > 0 && bbw !== null && bbw < bbwThreshold) continue;
    if (volThreshold > 0 && volRatio !== null && volRatio < volThreshold) continue;

    if (position !== null) {
      const currentPnl = position === 'long' ? (c - entryPrice) / entryPrice * 100 : (entryPrice - c) / entryPrice * 100;
      maxProfitPct = Math.max(maxProfitPct, currentPnl);

      let shouldStop = false, exitPrice = c;

      if (position === 'long' && o > ma288 && c < ma288) { shouldStop = true; }
      else if (position === 'short' && o < ma288 && c > ma288) { shouldStop = true; }

      if (shouldStop) {
        const pnl = position === 'long' ? (exitPrice - entryPrice) / entryPrice * 100 : (entryPrice - exitPrice) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); } else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ pnl, reason: 'STOP' });
        position = null; entryPrice = 0; maxProfitPct = 0; ma48CrossCount = 0;
        continue;
      }

      if (tpMode === 'trailing') {
        if (maxProfitPct >= trailingActivate) {
          const drawdown = maxProfitPct - currentPnl;
          if (drawdown >= trailingCallback) {
            totalPnL += currentPnl;
            if (currentPnl > 0) { winCount++; maxWin = Math.max(maxWin, currentPnl); } else { lossCount++; maxLoss = Math.min(maxLoss, currentPnl); }
            trades.push({ pnl: currentPnl, reason: 'TP' });
            position = null; entryPrice = 0; maxProfitPct = 0; ma48CrossCount = 0;
            continue;
          }
        }
      }
    }

    let isEntry = false, entryDir = '';
    if (ma288 > ma488 && o < ma288 && c > ma288) { isEntry = true; entryDir = 'long'; }
    else if (ma288 < ma488 && o > ma288 && c < ma288) { isEntry = true; entryDir = 'short'; }

    if (isEntry) {
      if (position !== null && position !== entryDir) {
        const pnl = position === 'long' ? (c - entryPrice) / entryPrice * 100 : (entryPrice - c) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); } else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ pnl, reason: 'CLOSE' });
        position = null; entryPrice = 0; maxProfitPct = 0; ma48CrossCount = 0;
      }
      if (position === null) {
        position = entryDir;
        entryPrice = c;
        maxProfitPct = 0;
        ma48CrossCount = 0;
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
// 测试1: 单参数测试
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试1: 单参数测试】");
console.log("=".repeat(70));

console.log("\n参数              | 交易数 | 胜率   | 总收益   | 平均收益 | 盈亏比");
console.log("-".repeat(75));

const singleTests = [
  { label: '无过滤(基准)', slope: 0, bbw: 0, vol: 0 },
  { label: 'slope>1', slope: 1, bbw: 0, vol: 0 },
  { label: 'slope>2', slope: 2, bbw: 0, vol: 0 },
  { label: 'slope>3', slope: 3, bbw: 0, vol: 0 },
  { label: 'slope>5', slope: 5, bbw: 0, vol: 0 },
  { label: 'bbw>0.5', slope: 0, bbw: 0.5, vol: 0 },
  { label: 'bbw>1.0', slope: 0, bbw: 1.0, vol: 0 },
  { label: 'bbw>1.5', slope: 0, bbw: 1.5, vol: 0 },
  { label: 'bbw>2.0', slope: 0, bbw: 2.0, vol: 0 },
  { label: 'vol>0.3', slope: 0, bbw: 0, vol: 0.3 },
  { label: 'vol>0.5', slope: 0, bbw: 0, vol: 0.5 },
  { label: 'vol>0.8', slope: 0, bbw: 0, vol: 0.8 },
];

const singleResults = [];
for (const cfg of singleTests) {
  const r = runStrategy(df_30m_valid, {
    slopeThreshold: cfg.slope,
    bbwThreshold: cfg.bbw,
    volThreshold: cfg.vol,
    tpMode: 'trailing',
    trailingActivate: 5,
    trailingCallback: 5,
  });
  singleResults.push({ label: cfg.label, ...r });
  console.log(
    `${cfg.label.padEnd(18)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}% | ${r.profitFactor.toFixed(2).padStart(6)}`
  );
}

// ============================================================
// 测试2: 组合测试 (宽松)
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试2: 组合测试 (宽松配置)】");
console.log("=".repeat(70));

console.log("\n配置                      | 交易数 | 胜率   | 总收益   | 平均收益 | 盈亏比");
console.log("-".repeat(80));

const comboTests = [
  { label: 'slope>1+vol>0.3', slope: 1, bbw: 0, vol: 0.3 },
  { label: 'slope>1+vol>0.5', slope: 1, bbw: 0, vol: 0.5 },
  { label: 'slope>2+vol>0.3', slope: 2, bbw: 0, vol: 0.3 },
  { label: 'slope>2+vol>0.5', slope: 2, bbw: 0, vol: 0.5 },
  { label: 'slope>1+bbw>0.5', slope: 1, bbw: 0.5, vol: 0 },
  { label: 'slope>1+bbw>1.0', slope: 1, bbw: 1.0, vol: 0 },
  { label: 'slope>2+bbw>0.5', slope: 2, bbw: 0.5, vol: 0 },
  { label: 'slope>2+bbw>1.0', slope: 2, bbw: 1.0, vol: 0 },
  { label: 'bbw>0.5+vol>0.3', slope: 0, bbw: 0.5, vol: 0.3 },
  { label: 'bbw>1.0+vol>0.3', slope: 0, bbw: 1.0, vol: 0.3 },
  { label: '全开(1+0.5+0.3)', slope: 1, bbw: 0.5, vol: 0.3 },
  { label: '全开(1+1.0+0.3)', slope: 1, bbw: 1.0, vol: 0.3 },
  { label: '全开(2+0.5+0.3)', slope: 2, bbw: 0.5, vol: 0.3 },
  { label: '全开(2+1.0+0.5)', slope: 2, bbw: 1.0, vol: 0.5 },
];

const comboResults = [];
for (const cfg of comboTests) {
  const r = runStrategy(df_30m_valid, {
    slopeThreshold: cfg.slope,
    bbwThreshold: cfg.bbw,
    volThreshold: cfg.vol,
    tpMode: 'trailing',
    trailingActivate: 5,
    trailingCallback: 5,
  });
  comboResults.push({ label: cfg.label, ...r });
  console.log(
    `${cfg.label.padEnd(25)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}% | ${r.profitFactor.toFixed(2).padStart(6)}`
  );
}

// ============================================================
// 测试3: 止盈参数配合
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试3: 最优过滤 + 止盈参数】");
console.log("=".repeat(70));

console.log("\n止盈配置          | 过滤配置           | 交易数 | 胜率   | 总收益   | 平均收益 | 盈亏比");
console.log("-".repeat(95));

const tpTests = [
  { tp: '移动(5+5)', act: 5, cb: 5 },
  { tp: '移动(3+3)', act: 3, cb: 3 },
  { tp: '移动(5+3)', act: 5, cb: 3 },
  { tp: '移动(8+5)', act: 8, cb: 5 },
  { tp: '移动(10+5)', act: 10, cb: 5 },
];

const bestCombo = comboResults.reduce((a, b) => a.totalPnL > b.totalPnL ? a : b);
const bestSlope = bestCombo.label.match(/slope>(\d+)/)?.[1] || 0;
const bestBbw = bestCombo.label.match(/bbw>(\d+\.?\d*)/)?.[1] || 0;
const bestVol = bestCombo.label.match(/vol>(\d+\.?\d*)/)?.[1] || 0;

console.log(`\n使用最优过滤配置: ${bestCombo.label}\n`);

for (const cfg of tpTests) {
  const r = runStrategy(df_30m_valid, {
    slopeThreshold: parseFloat(bestSlope),
    bbwThreshold: parseFloat(bestBbw),
    volThreshold: parseFloat(bestVol),
    tpMode: 'trailing',
    trailingActivate: cfg.act,
    trailingCallback: cfg.cb,
  });
  console.log(
    `${cfg.tp.padEnd(18)} | ${bestCombo.label.padEnd(18)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}% | ${r.profitFactor.toFixed(2).padStart(6)}`
  );
}

// ============================================================
// 最终推荐
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【最终推荐】");
console.log("=".repeat(70));

const bestOverall = [...singleResults, ...comboResults].reduce((a, b) => a.totalPnL > b.totalPnL ? a : b);
const baseline = singleResults.find(r => r.label === '无过滤(基准)');

console.log(`
基准(无过滤): ${baseline.totalPnL.toFixed(2)}%, 交易数: ${baseline.tradeCount}, 胜率: ${baseline.winRate.toFixed(1)}%
最优配置:     ${bestOverall.totalPnL.toFixed(2)}%, 交易数: ${bestOverall.tradeCount}, 胜率: ${bestOverall.winRate.toFixed(1)}%
配置:         ${bestOverall.label}
收益提升:     ${(bestOverall.totalPnL - baseline.totalPnL).toFixed(2)}%
`);

console.log("测试完成！");
