/**
 * 第六次分析: 在第四次最佳配置基础上加入成交量过滤
 * 基础配置: slope=5bps, bbw=2%, adaptive5m, trailing(3%+3%)
 * 新增: 成交量过滤
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
console.log("第六次分析: 成交量优化 (基于第四次最佳配置)");
console.log("=".repeat(70));

const df_30m = loadCSV('kline_30m_202607222207.csv', 'open_time');
const df_5m = loadCSV('kline_5m_202607222208.csv', 'open_time');

function addIndicators(df) {
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
  for (let i = 99; i < df.length; i++) {
    let sumSq = 0;
    for (let j = i - 99; j <= i; j++) sumSq += (closes[j] - bbMid[i]) ** 2;
    const std = Math.sqrt(sumSq / 100);
    bbUpper[i] = bbMid[i] + 2 * std;
    bbLower[i] = bbMid[i] - 2 * std;
    bbWidth[i] = (bbUpper[i] - bbLower[i]) / bbMid[i] * 100;
  }

  const ma288Slope = new Array(df.length).fill(null);
  for (let i = 5; i < df.length; i++) {
    if (ma288[i] !== null && ma288[i-5] !== null && ma288[i-5] !== 0) {
      ma288Slope[i] = (ma288[i] - ma288[i-5]) / ma288[i-5] * 10000;
    }
  }

  const priceDevMa488 = new Array(df.length).fill(null);
  for (let i = 0; i < df.length; i++) {
    if (ma488[i] !== null && ma488[i] !== 0) {
      priceDevMa488[i] = (closes[i] - ma488[i]) / ma488[i] * 100;
    }
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
    df[i].volMA = volMA[i];
    df[i].volRatio = volRatio[i];
  }
  return df;
}

console.log("\n计算技术指标...");
addIndicators(df_30m);
addIndicators(df_5m);

const df_30m_valid = df_30m.filter(r => r.ma288 !== null && r.ma488 !== null);
const df_5m_valid = df_5m.filter(r => r.ma288 !== null && r.ma488 !== null);

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

const trendMap5m = build5mTrendMap(df_5m_valid);

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

// ============================================================
// 策略回测 (基于第四次配置)
// ============================================================
function runStrategy(df30, config) {
  const {
    slopeThreshold = 5,
    bbWidthThreshold = 2.0,
    filter5mMode = 'adaptive',
    strong5mThreshold = 1.0,
    priceDevThreshold = 5.0,
    stopLossPct = 2.0,
    // 成交量配置
    volFilterEnabled = false,
    volFilterThreshold = 0.5,
    // 止盈配置
    trailingEnabled = true,
    trailingActivatePct = 3.0,
    trailingCallbackPct = 3.0,
  } = config;

  let position = null;
  let entryPrice = 0, entryTime = null;
  let maxProfitPct = 0;
  let totalPnL = 0, winCount = 0, lossCount = 0;
  let maxWin = 0, maxLoss = 0;
  const trades = [];
  let skipCount = 0;

  for (let i = 1; i < df30.length; i++) {
    const row = df30[i];
    const ma288 = row.ma288;
    const ma488 = row.ma488;
    const o = row.open, c = row.close;
    const slope = row.ma288Slope;
    const bbw = row.bbWidth;
    const dev = row.priceDevMa488;
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
      if (currentPnl < -stopLossPct) {
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
      if (currentPnl < -stopLossPct) {
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

    // 入场信号 - 先检查成交量
    let isEntrySignal = false;
    let entryType = '';

    if (trend === 'bearish' && o > ma288 && c < ma288) {
      isEntrySignal = true;
      entryType = 'SHORT';
    } else if (trend === 'bullish' && o < ma288 && c > ma288) {
      isEntrySignal = true;
      entryType = 'LONG';
    }

    if (isEntrySignal) {
      // 成交量过滤 - 只对入场信号生效
      if (volFilterEnabled && volRatio !== null && volRatio < volFilterThreshold) {
        skipCount++;
        continue; // 成交量不足，跳过入场
      }

      // 平掉反向持仓
      if (position === 'long' && entryType === 'SHORT') {
        const pnl = (c - entryPrice) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
        else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ entry: entryPrice, exit: c, pnl, type: 'long', entryTime, exitTime: row.open_time, reason: 'REVERSE' });
      } else if (position === 'short' && entryType === 'LONG') {
        const pnl = (entryPrice - c) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
        else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ entry: entryPrice, exit: c, pnl, type: 'short', entryTime, exitTime: row.open_time, reason: 'REVERSE' });
      }

      // 开新仓
      position = entryType === 'LONG' ? 'long' : 'short';
      entryPrice = c;
      entryTime = row.open_time;
      maxProfitPct = 0;
    }

    // 平仓信号
    if (trend === 'bearish' && o < ma288 && c > ma288 && position === 'short') {
      const pnl = (entryPrice - c) / entryPrice * 100;
      totalPnL += pnl;
      if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
      else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
      trades.push({ entry: entryPrice, exit: c, pnl, type: 'short', entryTime, exitTime: row.open_time, reason: 'COVER' });
      position = null;
    } else if (trend === 'bullish' && o > ma288 && c < ma288 && position === 'long') {
      const pnl = (c - entryPrice) / entryPrice * 100;
      totalPnL += pnl;
      if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
      else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
      trades.push({ entry: entryPrice, exit: c, pnl, type: 'long', entryTime, exitTime: row.open_time, reason: 'STOP' });
      position = null;
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
    trades,
    skipCount
  };
}

// ============================================================
// 测试不同成交量阈值
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【成交量阈值测试】");
console.log("=".repeat(70));

console.log("\n阈值  | 交易数 | 跳过数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 最大亏  | 盈亏比");
console.log("-".repeat(95));

const volThresholds = [0, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.2, 1.5];
const results = [];

for (const threshold of volThresholds) {
  const r = runStrategy(df_30m_valid, {
    slopeThreshold: 5,
    bbWidthThreshold: 2.0,
    filter5mMode: 'adaptive',
    strong5mThreshold: 1.0,
    priceDevThreshold: 5.0,
    stopLossPct: 2.0,
    volFilterEnabled: threshold > 0,
    volFilterThreshold: threshold,
    trailingEnabled: true,
    trailingActivatePct: 3.0,
    trailingCallbackPct: 3.0,
  });
  results.push({ ...r, label: `vol>${threshold}`, threshold });
  console.log(
    `${String(threshold > 0 ? '>' + threshold : '无').padEnd(5)} | ${String(r.tradeCount).padStart(6)} | ${String(r.skipCount).padStart(6)} | ` +
    `${r.winRate.toFixed(1).padStart(5)}% | ${r.totalPnL.toFixed(2).padStart(8)}% | ${r.avgPnL.toFixed(3).padStart(8)}% | ` +
    `${r.maxWin.toFixed(2).padStart(7)}% | ${r.maxLoss.toFixed(2).padStart(7)}% | ${r.profitFactor.toFixed(2).padStart(6)}`
  );
}

// ============================================================
// 找出最优阈值
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【最优阈值分析】");
console.log("=".repeat(70));

// 按总收益排名
const byReturn = [...results].sort((a, b) => b.totalPnL - a.totalPnL);
console.log("\n按总收益排名:");
for (const r of byReturn.slice(0, 5)) {
  console.log(`  ${r.label.padEnd(8)}: ${r.totalPnL.toFixed(2)}% (${r.tradeCount}笔, 胜率${r.winRate.toFixed(1)}%)`);
}

// 按平均收益排名
const byAvg = [...results].sort((a, b) => (b.avgPnl || 0) - (a.avgPnl || 0));
console.log("\n按平均收益排名:");
for (const r of byAvg.slice(0, 5)) {
  console.log(`  ${r.label.padEnd(8)}: ${(r.avgPnl || 0).toFixed(3)}%/笔 (${r.tradeCount}笔, 胜率${r.winRate.toFixed(1)}%)`);
}

// 按胜率排名
const byWinRate = [...results].sort((a, b) => b.winRate - a.winRate);
console.log("\n按胜率排名:");
for (const r of byWinRate.slice(0, 5)) {
  console.log(`  ${r.label.padEnd(8)}: ${r.winRate.toFixed(1)}% (${r.tradeCount}笔, 总收益${r.totalPnL.toFixed(2)}%)`);
}

// ============================================================
// 最优配置详细分析
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【最优配置详细分析】");
console.log("=".repeat(70));

// 选择vol>0.6作为推荐 (最佳平衡)
const optimal = results.find(r => r.threshold === 0.6) || results[0];

console.log(`\n推荐配置: slope=5bps + bbw=2% + adaptive5m + vol>0.5 + trailing(3%+3%)`);
console.log(`\n统计:`);
console.log(`  完成交易: ${optimal.tradeCount} 笔`);
console.log(`  跳过信号: ${optimal.skipCount} 个 (成交量不足)`);
console.log(`  胜率: ${optimal.winRate.toFixed(1)}%`);
console.log(`  总收益: ${optimal.totalPnL.toFixed(2)}%`);
console.log(`  平均收益: ${(optimal.totalPnL / optimal.tradeCount).toFixed(3)}%`);
console.log(`  最大盈利: ${optimal.maxWin.toFixed(2)}%`);
console.log(`  最大亏损: ${optimal.maxLoss.toFixed(2)}%`);
console.log(`  盈亏比: ${optimal.profitFactor.toFixed(2)}`);

// 出场类型统计
console.log("\n--- 出场类型统计 ---");
const typeCounts = {};
for (const t of optimal.trades) {
  typeCounts[t.reason] = (typeCounts[t.reason] || 0) + 1;
}
for (const [type, count] of Object.entries(typeCounts).sort((a,b) => b[1]-a[1])) {
  const avgPnl = optimal.trades.filter(t => t.reason === type).reduce((s,t) => s+t.pnl, 0) / count;
  console.log(`  ${type.padEnd(15)}: ${count} 次, 平均收益: ${avgPnl >= 0 ? '+' : ''}${avgPnl.toFixed(3)}%`);
}

// 最近交易
console.log("\n--- 最近15笔交易 ---");
for (const t of optimal.trades.slice(-15)) {
  const duration = (t.exitTime - t.entryTime) / 3600000;
  const pnlSign = t.pnl >= 0 ? '+' : '';
  console.log(`  ${t.type.padEnd(5)} ${t.entry.toFixed(2)} → ${t.exit.toFixed(2)} | PnL: ${pnlSign}${t.pnl.toFixed(4)}% | ${t.reason} | ${duration.toFixed(1)}h`);
}

// ============================================================
// 策略演进对比
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【策略演进对比】");
console.log("=".repeat(70));

const baseline = results.find(r => r.threshold === 0); // 无成交量过滤
const withVol = optimal;

const baselineAvg = baseline.totalPnL / baseline.tradeCount;
const withVolAvg = withVol.totalPnL / withVol.tradeCount;

console.log(`
版本                | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 最大亏
--------------------|--------|--------|----------|----------|---------|--------
第四次(无成交量)    | ${String(baseline.tradeCount).padStart(6)} | ${baseline.winRate.toFixed(1).padStart(5)}% | ${baseline.totalPnL >= 0 ? '+' : ''}${baseline.totalPnL.toFixed(2).padStart(8)}% | ${baselineAvg >= 0 ? '+' : ''}${baselineAvg.toFixed(3).padStart(8)}% | ${baseline.maxWin.toFixed(2).padStart(7)}% | ${baseline.maxLoss.toFixed(2).padStart(7)}%
第六次(+成交量)     | ${String(withVol.tradeCount).padStart(6)} | ${withVol.winRate.toFixed(1).padStart(5)}% | ${withVol.totalPnL >= 0 ? '+' : ''}${withVol.totalPnL.toFixed(2).padStart(8)}% | ${withVolAvg >= 0 ? '+' : ''}${withVolAvg.toFixed(3).padStart(8)}% | ${withVol.maxWin.toFixed(2).padStart(7)}% | ${withVol.maxLoss.toFixed(2).padStart(7)}%
`);

const improvementPct = ((withVolAvg - baselineAvg) / baselineAvg * 100).toFixed(1);
console.log(`平均收益提升: ${improvementPct}%`);

console.log("\n第六次分析完成！");
