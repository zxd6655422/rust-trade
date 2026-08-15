/**
 * 第五次分析: 三项优化测试
 * 1. ATR止损 (替代固定2%止损)
 * 2. 成交量确认 (信号K线成交量过滤)
 * 3. 4h方向过滤 (大周期确认)
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
console.log("第五次分析: 三项优化测试");
console.log("=".repeat(70));

const df_30m = loadCSV('kline_30m_202607222207.csv', 'open_time');
const df_5m = loadCSV('kline_5m_202607222208.csv', 'open_time');
const df_4h = loadCSV('kline_4h_202607222213.csv', 'open_time');

// ============================================================
// 计算技术指标 (含ATR和成交量)
// ============================================================
function addIndicators(df) {
  const closes = df.map(r => r.close);
  const highs = df.map(r => r.high);
  const lows = df.map(r => r.low);
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

  // 布林带
  const bbMid = calcMA(100);
  const bbUpper = new Array(df.length).fill(null);
  const bbLower = new Array(df.length).fill(null);
  const bbWidth = new Array(df.length).fill(null);
  for (let i = 99; i < df.length; i++) {
    let sumSq = 0;
    for (let j = i - 99; j <= i; j++) sumSq += (closes[j] - bbMid[i]) ** 2;
    const std = Math.sqrt(sumSq / 100);
    bbUpper[i] = bbMid[i] + 2 * std;
    bbLower[i] = bbMid[i] - 2 * std;
    bbWidth[i] = (bbUpper[i] - bbLower[i]) / bbMid[i] * 100;
  }

  // MA288斜率
  const ma288Slope = new Array(df.length).fill(null);
  for (let i = 5; i < df.length; i++) {
    if (ma288[i] !== null && ma288[i-5] !== null && ma288[i-5] !== 0) {
      ma288Slope[i] = (ma288[i] - ma288[i-5]) / ma288[i-5] * 10000;
    }
  }

  // 价格偏离MA488
  const priceDevMa488 = new Array(df.length).fill(null);
  for (let i = 0; i < df.length; i++) {
    if (ma488[i] !== null && ma488[i] !== 0) {
      priceDevMa488[i] = (closes[i] - ma488[i]) / ma488[i] * 100;
    }
  }

  // ATR (14周期)
  const atr = new Array(df.length).fill(null);
  for (let i = 14; i < df.length; i++) {
    let sum = 0;
    for (let j = i - 13; j <= i; j++) {
      const tr = Math.max(
        highs[j] - lows[j],
        Math.abs(highs[j] - closes[j-1]),
        Math.abs(lows[j] - closes[j-1])
      );
      sum += tr;
    }
    atr[i] = sum / 14;
  }

  // 成交量MA (20周期)
  const volMA = new Array(df.length).fill(null);
  for (let i = 19; i < df.length; i++) {
    let sum = 0;
    for (let j = i - 19; j <= i; j++) sum += volumes[j];
    volMA[i] = sum / 20;
  }

  // 成交量比率
  const volRatio = new Array(df.length).fill(null);
  for (let i = 0; i < df.length; i++) {
    if (volMA[i] !== null && volMA[i] > 0) {
      volRatio[i] = volumes[i] / volMA[i];
    }
  }

  for (let i = 0; i < df.length; i++) {
    df[i].ma48 = ma48[i];
    df[i].ma288 = ma288[i];
    df[i].ma488 = ma488[i];
    df[i].bbMid = bbMid[i];
    df[i].bbUpper = bbUpper[i];
    df[i].bbLower = bbLower[i];
    df[i].bbWidth = bbWidth[i];
    df[i].ma288Slope = ma288Slope[i];
    df[i].priceDevMa488 = priceDevMa488[i];
    df[i].atr = atr[i];
    df[i].atrPct = atr[i] ? (atr[i] / closes[i] * 100) : null; // ATR百分比
    df[i].volMA = volMA[i];
    df[i].volRatio = volRatio[i];
  }
  return df;
}

console.log("\n计算技术指标...");
addIndicators(df_30m);
addIndicators(df_5m);
addIndicators(df_4h);

const df_30m_valid = df_30m.filter(r => r.ma288 !== null && r.ma488 !== null);
const df_5m_valid = df_5m.filter(r => r.ma288 !== null && r.ma488 !== null);
const df_4h_valid = df_4h.filter(r => r.ma288 !== null && r.ma488 !== null);

// 5m趋势索引
function build5mTrendMap(df5m) {
  const map = new Map();
  for (const r of df5m) {
    if (r.ma288 === null || r.ma488 === null) continue;
    const spread = (r.ma288 - r.ma488) / r.ma488 * 100;
    map.set(r.open_time.getTime(), {
      trend: r.ma288 > r.ma488 ? 'bullish' : 'bearish',
      spread,
      close: r.close
    });
  }
  return map;
}

// 4h趋势索引
function build4hTrendMap(df4h) {
  const map = new Map();
  for (const r of df4h) {
    if (r.ma288 === null || r.ma488 === null) continue;
    map.set(r.open_time.getTime(), {
      trend: r.ma288 > r.ma488 ? 'bullish' : 'bearish',
      ma288: r.ma288,
      ma488: r.ma488
    });
  }
  return map;
}

const trendMap5m = build5mTrendMap(df_5m_valid);
const trendMap4h = build4hTrendMap(df_4h_valid);

function get5mTrendAt(time) {
  const t = time.getTime();
  let best = null;
  let bestDiff = Infinity;
  for (const [ts, data] of trendMap5m) {
    const diff = t - ts;
    if (diff >= 0 && diff < bestDiff) {
      bestDiff = diff;
      best = data;
    }
    if (diff < 0) break;
  }
  return best;
}

function get4hTrendAt(time) {
  const t = time.getTime();
  let best = null;
  let bestDiff = Infinity;
  for (const [ts, data] of trendMap4h) {
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
// ATR统计
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【ATR统计】");
console.log("=".repeat(70));

const atrValues = df_30m_valid.map(r => r.atrPct).filter(v => v !== null);
atrValues.sort((a, b) => a - b);
console.log(`\n30m ATR百分比分布:`);
console.log(`  最小值: ${atrValues[0].toFixed(3)}%`);
console.log(`  25%分位: ${atrValues[Math.floor(atrValues.length*0.25)].toFixed(3)}%`);
console.log(`  中位数: ${atrValues[Math.floor(atrValues.length*0.5)].toFixed(3)}%`);
console.log(`  75%分位: ${atrValues[Math.floor(atrValues.length*0.75)].toFixed(3)}%`);
console.log(`  最大值: ${atrValues[atrValues.length-1].toFixed(3)}%`);

// ============================================================
// 成交量统计
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【成交量统计】");
console.log("=".repeat(70));

const volRatios = df_30m_valid.map(r => r.volRatio).filter(v => v !== null);
volRatios.sort((a, b) => a - b);
console.log(`\n30m 成交量比率分布 (当前/MA20):`);
console.log(`  最小值: ${volRatios[0].toFixed(2)}`);
console.log(`  25%分位: ${volRatios[Math.floor(volRatios.length*0.25)].toFixed(2)}`);
console.log(`  中位数: ${volRatios[Math.floor(volRatios.length*0.5)].toFixed(2)}`);
console.log(`  75%分位: ${volRatios[Math.floor(volRatios.length*0.75)].toFixed(2)}`);
console.log(`  最大值: ${volRatios[volRatios.length-1].toFixed(2)}`);

// ============================================================
// 4h趋势统计
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【4h趋势统计】");
console.log("=".repeat(70));

let bullCount = 0, bearCount = 0;
for (const r of df_4h_valid) {
  if (r.ma288 > r.ma488) bullCount++;
  else bearCount++;
}
console.log(`\n4h趋势分布:`);
console.log(`  多头: ${bullCount} 根 (${(bullCount/df_4h_valid.length*100).toFixed(1)}%)`);
console.log(`  空头: ${bearCount} 根 (${(bearCount/df_4h_valid.length*100).toFixed(1)}%)`);

// ============================================================
// 策略回测函数
// ============================================================
function runStrategy(df30, config) {
  const {
    slopeThreshold = 5,
    bbWidthThreshold = 2.0,
    filter5mMode = 'adaptive',
    strong5mThreshold = 1.0,
    priceDevThreshold = 5.0,
    // 止损配置
    stopLossMode = 'fixed', // 'fixed', 'atr'
    stopLossPct = 2.0,
    atrStopMultiplier = 2.0, // ATR止损倍数
    // 成交量配置
    volFilterEnabled = false,
    volFilterThreshold = 1.0, // 成交量比率阈值
    // 4h过滤配置
    filter4hEnabled = false,
    filter4hMode = 'same', // 'same' = 同方向才交易, 'opposite_pause' = 反向暂停
    // 止盈配置
    trailingEnabled = true,
    trailingActivatePct = 3.0,
    trailingCallbackPct = 3.0,
  } = config;

  const signals = [];
  let position = null;
  let entryPrice = 0, entryTime = null;
  let maxProfitPct = 0;
  let totalPnL = 0, winCount = 0, lossCount = 0;
  let maxWin = 0, maxLoss = 0;
  const trades = [];

  for (let i = 1; i < df30.length; i++) {
    const row = df30[i];
    const ma288 = row.ma288;
    const ma488 = row.ma488;
    const o = row.open, c = row.close;
    const slope = row.ma288Slope;
    const bbw = row.bbWidth;
    const dev = row.priceDevMa488;
    const atrPct = row.atrPct;
    const volRatio = row.volRatio;

    let trend;
    if (ma288 < ma488) trend = 'bearish';
    else if (ma288 > ma488) trend = 'bullish';
    else continue;

    // 基础过滤
    if (slopeThreshold > 0 && slope !== null && Math.abs(slope) < slopeThreshold) continue;
    if (bbWidthThreshold > 0 && bbw !== null && bbw < bbWidthThreshold) continue;

    // 5m过滤
    const data5m = get5mTrendAt(row.open_time);
    const trend5m = data5m ? data5m.trend : null;
    const spread5m = data5m ? data5m.spread : 0;

    if (filter5mMode === 'adaptive' && trend5m !== null && trend5m !== trend) {
      if (dev !== null && Math.abs(dev) > priceDevThreshold) continue;
      if (Math.abs(spread5m) > strong5mThreshold) continue;
    }

    // 4h过滤
    if (filter4hEnabled) {
      const data4h = get4hTrendAt(row.open_time);
      const trend4h = data4h ? data4h.trend : null;
      if (trend4h !== null) {
        if (filter4hMode === 'same' && trend4h !== trend) continue;
        if (filter4hMode === 'opposite_pause' && trend4h !== trend) {
          // 4h反向时，如果有持仓则平仓
          if (position !== null) {
            const pnl = position === 'long'
              ? (c - entryPrice) / entryPrice * 100
              : (entryPrice - c) / entryPrice * 100;
            totalPnL += pnl;
            if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
            else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
            trades.push({ entry: entryPrice, exit: c, pnl, type: position, entryTime, exitTime: row.open_time, reason: '4H_REVERSE' });
            position = null;
          }
          continue;
        }
      }
    }

    // 成交量过滤
    if (volFilterEnabled && volRatio !== null && volRatio < volFilterThreshold) continue;

    // 踏空翻转
    if (dev !== null) {
      if (trend === 'bullish' && dev < -priceDevThreshold && position === 'long') {
        const pnl = (c - entryPrice) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
        else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ entry: entryPrice, exit: c, pnl, type: 'long', entryTime, exitTime: row.open_time, reason: 'FLIP' });
        position = null;
        continue;
      }
      if (trend === 'bearish' && dev > priceDevThreshold && position === 'short') {
        const pnl = (entryPrice - c) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
        else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ entry: entryPrice, exit: c, pnl, type: 'short', entryTime, exitTime: row.open_time, reason: 'FLIP' });
        position = null;
        continue;
      }
    }

    // 持仓中的止盈止损
    if (position === 'long') {
      const currentPnl = (c - entryPrice) / entryPrice * 100;
      maxProfitPct = Math.max(maxProfitPct, currentPnl);

      // 止损
      let stopLevel = -stopLossPct;
      if (stopLossMode === 'atr' && atrPct !== null) {
        stopLevel = -(atrPct * atrStopMultiplier);
      }
      if (currentPnl < stopLevel) {
        totalPnL += currentPnl;
        lossCount++; maxLoss = Math.min(maxLoss, currentPnl);
        trades.push({ entry: entryPrice, exit: c, pnl: currentPnl, type: 'long', entryTime, exitTime: row.open_time, reason: 'STOP' });
        position = null;
        continue;
      }

      // 移动止盈
      if (trailingEnabled && maxProfitPct >= trailingActivatePct) {
        const drawdown = maxProfitPct - currentPnl;
        if (drawdown >= trailingCallbackPct) {
          totalPnL += currentPnl;
          if (currentPnl > 0) { winCount++; maxWin = Math.max(maxWin, currentPnl); }
          else { lossCount++; maxLoss = Math.min(maxLoss, currentPnl); }
          trades.push({ entry: entryPrice, exit: c, pnl: currentPnl, type: 'long', entryTime, exitTime: row.open_time, reason: 'TRAILING_TP' });
          position = null;
          continue;
        }
      }
    } else if (position === 'short') {
      const currentPnl = (entryPrice - c) / entryPrice * 100;
      maxProfitPct = Math.max(maxProfitPct, currentPnl);

      // 止损
      let stopLevel = -stopLossPct;
      if (stopLossMode === 'atr' && atrPct !== null) {
        stopLevel = -(atrPct * atrStopMultiplier);
      }
      if (currentPnl < stopLevel) {
        totalPnL += currentPnl;
        lossCount++; maxLoss = Math.min(maxLoss, currentPnl);
        trades.push({ entry: entryPrice, exit: c, pnl: currentPnl, type: 'short', entryTime, exitTime: row.open_time, reason: 'STOP' });
        position = null;
        continue;
      }

      // 移动止盈
      if (trailingEnabled && maxProfitPct >= trailingActivatePct) {
        const drawdown = maxProfitPct - currentPnl;
        if (drawdown >= trailingCallbackPct) {
          totalPnL += currentPnl;
          if (currentPnl > 0) { winCount++; maxWin = Math.max(maxWin, currentPnl); }
          else { lossCount++; maxLoss = Math.min(maxLoss, currentPnl); }
          trades.push({ entry: entryPrice, exit: c, pnl: currentPnl, type: 'short', entryTime, exitTime: row.open_time, reason: 'TRAILING_TP' });
          position = null;
          continue;
        }
      }
    }

    // 入场信号
    if (trend === 'bearish') {
      if (o > ma288 && c < ma288) {
        if (position === 'long') {
          const pnl = (c - entryPrice) / entryPrice * 100;
          totalPnL += pnl;
          if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
          else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
          trades.push({ entry: entryPrice, exit: c, pnl, type: 'long', entryTime, exitTime: row.open_time, reason: 'REVERSE' });
        }
        position = 'short';
        entryPrice = c;
        entryTime = row.open_time;
        maxProfitPct = 0;
      } else if (o < ma288 && c > ma288 && position === 'short') {
        const pnl = (entryPrice - c) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
        else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ entry: entryPrice, exit: c, pnl, type: 'short', entryTime, exitTime: row.open_time, reason: 'COVER' });
        position = null;
      }
    } else if (trend === 'bullish') {
      if (o < ma288 && c > ma288) {
        if (position === 'short') {
          const pnl = (entryPrice - c) / entryPrice * 100;
          totalPnL += pnl;
          if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
          else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
          trades.push({ entry: entryPrice, exit: c, pnl, type: 'short', entryTime, exitTime: row.open_time, reason: 'REVERSE' });
        }
        position = 'long';
        entryPrice = c;
        entryTime = row.open_time;
        maxProfitPct = 0;
      } else if (o > ma288 && c < ma288 && position === 'long') {
        const pnl = (c - entryPrice) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
        else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ entry: entryPrice, exit: c, pnl, type: 'long', entryTime, exitTime: row.open_time, reason: 'STOP' });
        position = null;
      }
    }
  }

  return {
    tradeCount: trades.length,
    winCount,
    lossCount,
    winRate: trades.length > 0 ? (winCount / trades.length * 100) : 0,
    totalPnL,
    avgPnL: trades.length > 0 ? (totalPnL / trades.length) : 0,
    maxWin,
    maxLoss,
    profitFactor: maxLoss !== 0 ? (maxWin / Math.abs(maxLoss)) : 0,
    trades
  };
}

// ============================================================
// 测试1: ATR止损
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试1: ATR止损】");
console.log("=".repeat(70));

console.log("\n止损模式      | 倍数/百分比 | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 最大亏");
console.log("-".repeat(95));

const atrTests = [
  { label: '固定2%', mode: 'fixed', pct: 2.0 },
  { label: '固定1.5%', mode: 'fixed', pct: 1.5 },
  { label: '固定3%', mode: 'fixed', pct: 3.0 },
  { label: 'ATR 1.5x', mode: 'atr', mult: 1.5 },
  { label: 'ATR 2.0x', mode: 'atr', mult: 2.0 },
  { label: 'ATR 2.5x', mode: 'atr', mult: 2.5 },
  { label: 'ATR 3.0x', mode: 'atr', mult: 3.0 },
  { label: 'ATR 4.0x', mode: 'atr', mult: 4.0 },
];

const atrResults = [];
for (const t of atrTests) {
  const r = runStrategy(df_30m_valid, {
    slopeThreshold: 5,
    bbWidthThreshold: 2.0,
    filter5mMode: 'adaptive',
    strong5mThreshold: 1.0,
    priceDevThreshold: 5.0,
    stopLossMode: t.mode,
    stopLossPct: t.pct || 2.0,
    atrStopMultiplier: t.mult || 2.0,
    trailingEnabled: true,
    trailingActivatePct: 3.0,
    trailingCallbackPct: 3.0,
  });
  atrResults.push({ label: t.label, ...r });
  console.log(
    `${t.label.padEnd(13)} | ${String(t.mode === 'atr' ? t.mult + 'x' : t.pct + '%').padEnd(11)} | ` +
    `${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${r.totalPnL.toFixed(2).padStart(8)}% | ${r.avgPnL.toFixed(3).padStart(8)}% | ` +
    `${r.maxWin.toFixed(2).padStart(7)}% | ${r.maxLoss.toFixed(2).padStart(7)}%`
  );
}

// ============================================================
// 测试2: 成交量确认
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试2: 成交量确认】");
console.log("=".repeat(70));

console.log("\n成交量阈值 | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 最大亏");
console.log("-".repeat(80));

const volTests = [0, 0.5, 0.8, 1.0, 1.2, 1.5, 2.0];

const volResults = [];
for (const threshold of volTests) {
  const r = runStrategy(df_30m_valid, {
    slopeThreshold: 5,
    bbWidthThreshold: 2.0,
    filter5mMode: 'adaptive',
    strong5mThreshold: 1.0,
    priceDevThreshold: 5.0,
    stopLossMode: 'atr',
    atrStopMultiplier: 2.0,
    volFilterEnabled: threshold > 0,
    volFilterThreshold: threshold,
    trailingEnabled: true,
    trailingActivatePct: 3.0,
    trailingCallbackPct: 3.0,
  });
  volResults.push({ label: `vol>${threshold}`, ...r });
  console.log(
    `${String(threshold > 0 ? '>' + threshold : '无').padEnd(10)} | ${String(r.tradeCount).padStart(6)} | ` +
    `${r.winRate.toFixed(1).padStart(5)}% | ${r.totalPnL.toFixed(2).padStart(8)}% | ` +
    `${r.avgPnL.toFixed(3).padStart(8)}% | ${r.maxWin.toFixed(2).padStart(7)}% | ${r.maxLoss.toFixed(2).padStart(7)}%`
  );
}

// ============================================================
// 测试3: 4h方向过滤
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试3: 4h方向过滤】");
console.log("=".repeat(70));

console.log("\n模式            | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 最大亏");
console.log("-".repeat(80));

const h4Tests = [
  { label: '无过滤', mode: 'none' },
  { label: '同方向才交易', mode: 'same' },
  { label: '反向暂停', mode: 'opposite_pause' },
];

const h4Results = [];
for (const t of h4Tests) {
  const r = runStrategy(df_30m_valid, {
    slopeThreshold: 5,
    bbWidthThreshold: 2.0,
    filter5mMode: 'adaptive',
    strong5mThreshold: 1.0,
    priceDevThreshold: 5.0,
    stopLossMode: 'atr',
    atrStopMultiplier: 2.0,
    filter4hEnabled: t.mode !== 'none',
    filter4hMode: t.mode === 'none' ? 'same' : t.mode,
    trailingEnabled: true,
    trailingActivatePct: 3.0,
    trailingCallbackPct: 3.0,
  });
  h4Results.push({ label: t.label, ...r });
  console.log(
    `${t.label.padEnd(15)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${r.totalPnL.toFixed(2).padStart(8)}% | ${r.avgPnL.toFixed(3).padStart(8)}% | ` +
    `${r.maxWin.toFixed(2).padStart(7)}% | ${r.maxLoss.toFixed(2).padStart(7)}%`
  );
}

// ============================================================
// 组合测试: ATR + 成交量 + 4h
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【组合测试: ATR + 成交量 + 4h】");
console.log("=".repeat(70));

console.log("\n组合                           | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 最大亏");
console.log("-".repeat(95));

const comboTests = [
  { label: '基准(ATR2x)', atr: 2.0, vol: 0, h4: 'none' },
  { label: '+成交量(>0.8)', atr: 2.0, vol: 0.8, h4: 'none' },
  { label: '+4h同方向', atr: 2.0, vol: 0, h4: 'same' },
  { label: '+成交量+4h', atr: 2.0, vol: 0.8, h4: 'same' },
  { label: 'ATR2.5x+vol0.8+4h', atr: 2.5, vol: 0.8, h4: 'same' },
  { label: 'ATR3x+vol0.8+4h', atr: 3.0, vol: 0.8, h4: 'same' },
];

const comboResults = [];
for (const t of comboTests) {
  const r = runStrategy(df_30m_valid, {
    slopeThreshold: 5,
    bbWidthThreshold: 2.0,
    filter5mMode: 'adaptive',
    strong5mThreshold: 1.0,
    priceDevThreshold: 5.0,
    stopLossMode: 'atr',
    atrStopMultiplier: t.atr,
    volFilterEnabled: t.vol > 0,
    volFilterThreshold: t.vol,
    filter4hEnabled: t.h4 !== 'none',
    filter4hMode: t.h4,
    trailingEnabled: true,
    trailingActivatePct: 3.0,
    trailingCallbackPct: 3.0,
  });
  comboResults.push({ label: t.label, ...r });
  console.log(
    `${t.label.padEnd(29)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${r.totalPnL.toFixed(2).padStart(8)}% | ${r.avgPnL.toFixed(3).padStart(8)}% | ` +
    `${r.maxWin.toFixed(2).padStart(7)}% | ${r.maxLoss.toFixed(2).padStart(7)}%`
  );
}

// ============================================================
// 最优配置分析
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【最优配置分析】");
console.log("=".repeat(70));

const bestCombo = comboResults.reduce((a, b) => a.totalPnL > b.totalPnL ? a : b);
console.log(`\n最优组合: ${bestCombo.label}`);
console.log(`\n统计:`);
console.log(`  完成交易: ${bestCombo.tradeCount} 笔`);
console.log(`  胜率: ${bestCombo.winRate.toFixed(1)}%`);
console.log(`  总收益: ${bestCombo.totalPnL.toFixed(2)}%`);
console.log(`  平均收益: ${bestCombo.avgPnL.toFixed(3)}%`);
console.log(`  最大盈利: ${bestCombo.maxWin.toFixed(2)}%`);
console.log(`  最大亏损: ${bestCombo.maxLoss.toFixed(2)}%`);
console.log(`  盈亏比: ${bestCombo.profitFactor.toFixed(2)}`);

// 止盈止损类型统计
console.log("\n--- 出场类型统计 ---");
const typeCounts = {};
for (const t of bestCombo.trades) {
  typeCounts[t.reason] = (typeCounts[t.reason] || 0) + 1;
}
for (const [type, count] of Object.entries(typeCounts).sort((a,b) => b[1]-a[1])) {
  const avgPnl = bestCombo.trades.filter(t => t.reason === type).reduce((s,t) => s+t.pnl, 0) / count;
  console.log(`  ${type.padEnd(15)}: ${count} 次, 平均收益: ${avgPnl >= 0 ? '+' : ''}${avgPnl.toFixed(3)}%`);
}

// ============================================================
// 策略演进对比
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【策略演进对比】");
console.log("=".repeat(70));

const baseline = runStrategy(df_30m_valid, {
  slopeThreshold: 0, bbWidthThreshold: 0, filter5mMode: 'none',
  stopLossMode: 'fixed', stopLossPct: 999, trailingEnabled: false
});

const v4 = runStrategy(df_30m_valid, {
  slopeThreshold: 5, bbWidthThreshold: 2.0, filter5mMode: 'adaptive',
  strong5mThreshold: 1.0, priceDevThreshold: 5.0,
  stopLossMode: 'fixed', stopLossPct: 2.0,
  trailingEnabled: true, trailingActivatePct: 3.0, trailingCallbackPct: 3.0
});

const v5 = bestCombo;

console.log(`
版本                | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 最大亏
--------------------|--------|--------|----------|----------|---------|--------
第一次(无过滤)      | ${String(baseline.tradeCount).padStart(6)} | ${baseline.winRate.toFixed(1).padStart(5)}% | ${baseline.totalPnL >= 0 ? '+' : ''}${baseline.totalPnL.toFixed(2).padStart(8)}% | ${baseline.avgPnL >= 0 ? '+' : ''}${baseline.avgPnL.toFixed(3).padStart(8)}% | ${baseline.maxWin.toFixed(2).padStart(7)}% | ${baseline.maxLoss.toFixed(2).padStart(7)}%
第四次(+止盈优化)   | ${String(v4.tradeCount).padStart(6)} | ${v4.winRate.toFixed(1).padStart(5)}% | ${v4.totalPnL >= 0 ? '+' : ''}${v4.totalPnL.toFixed(2).padStart(8)}% | ${v4.avgPnL >= 0 ? '+' : ''}${v4.avgPnL.toFixed(3).padStart(8)}% | ${v4.maxWin.toFixed(2).padStart(7)}% | ${v4.maxLoss.toFixed(2).padStart(7)}%
第五次(+ATR/量/4h)  | ${String(v5.tradeCount).padStart(6)} | ${v5.winRate.toFixed(1).padStart(5)}% | ${v5.totalPnL >= 0 ? '+' : ''}${v5.totalPnL.toFixed(2).padStart(8)}% | ${v5.avgPnL >= 0 ? '+' : ''}${v5.avgPnL.toFixed(3).padStart(8)}% | ${v5.maxWin.toFixed(2).padStart(7)}% | ${v5.maxLoss.toFixed(2).padStart(7)}%
`);

console.log("第五次分析完成！");
