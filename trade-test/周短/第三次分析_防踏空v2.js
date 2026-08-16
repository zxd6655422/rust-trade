/**
 * 第三次分析v2: 防踏空策略 - 改进版
 *
 * 改进思路:
 * 1. 不用5m方向硬过滤 (太激进)
 * 2. 用5m的"趋势强度"来判断是否需要暂停
 * 3. 价格偏离MA488时，用5m确认是否真的踏空
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
console.log("第三次分析v2: 防踏空策略 - 改进版");
console.log("=".repeat(70));

const df_30m = loadCSV('kline_30m_202607222207.csv', 'open_time');
const df_5m = loadCSV('kline_5m_202607222208.csv', 'open_time');

function addIndicators(df) {
  const closes = df.map(r => r.close);
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

  // 价格偏离MA488的百分比
  const priceDevMa488 = new Array(df.length).fill(null);
  for (let i = 0; i < df.length; i++) {
    if (ma488[i] !== null && ma488[i] !== 0) {
      priceDevMa488[i] = (closes[i] - ma488[i]) / ma488[i] * 100;
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
  }
  return df;
}

console.log("\n计算技术指标...");
addIndicators(df_30m);
addIndicators(df_5m);

const df_30m_valid = df_30m.filter(r => r.ma288 !== null && r.ma488 !== null);
const df_5m_valid = df_5m.filter(r => r.ma288 !== null && r.ma488 !== null);

// 构建5m趋势索引 - 包含趋势强度
function build5mTrendMap(df5m) {
  const map = new Map();
  for (const r of df5m) {
    if (r.ma288 === null || r.ma488 === null) continue;
    const spread = (r.ma288 - r.ma488) / r.ma488 * 100; // 均线差值百分比
    const trend = r.ma288 > r.ma488 ? 'bullish' : 'bearish';
    map.set(r.open_time.getTime(), {
      trend,
      spread, // 趋势强度: 正值=多头强度, 负值=空头强度
      ma288: r.ma288,
      ma488: r.ma488,
      close: r.close,
      priceDevMa488: r.priceDevMa488
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
// 改进的防踏空策略
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【改进的防踏空策略】");
console.log("=".repeat(70));

function runAntiMissV2(df30, config) {
  const {
    slopeThreshold = 5,
    bbWidthThreshold = 2.0,
    // 5m过滤模式: 'none', 'strong_only', 'adaptive'
    filter5mMode = 'none',
    // 5m强烈反向阈值 (均线差值百分比)
    strong5mThreshold = 0.5,
    // 踏空检测
    priceDevThreshold = 5.0,
    // 止损
    stopLossPct = 2.0,
  } = config;

  const signals = [];
  let position = null;
  let entryPrice = 0, entryTime = null;
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

    let trend;
    if (ma288 < ma488) trend = 'bearish';
    else if (ma288 > ma488) trend = 'bullish';
    else continue;

    // 基础过滤
    if (slopeThreshold > 0 && slope !== null && Math.abs(slope) < slopeThreshold) continue;
    if (bbWidthThreshold > 0 && bbw !== null && bbw < bbWidthThreshold) continue;

    // 获取5m数据
    const data5m = get5mTrendAt(row.open_time);
    const trend5m = data5m ? data5m.trend : null;
    const spread5m = data5m ? data5m.spread : 0;

    // === 5m过滤逻辑 ===
    let skipBy5m = false;
    if (filter5mMode === 'strong_only') {
      // 只有当5m强烈反向时才过滤
      if (trend5m !== null && trend5m !== trend) {
        if (Math.abs(spread5m) > strong5mThreshold) {
          skipBy5m = true; // 5m强烈反向，暂停
        }
        // 如果5m只是轻微反向，不过滤
      }
    } else if (filter5mMode === 'adaptive') {
      // 自适应: 结合价格偏离和5m方向
      if (trend5m !== null && trend5m !== trend) {
        // 5m反向 + 价格偏离MA488 → 可能踏空
        if (dev !== null && Math.abs(dev) > priceDevThreshold) {
          skipBy5m = true;
        }
        // 5m反向 + 5m强烈 → 暂停
        if (Math.abs(spread5m) > strong5mThreshold) {
          skipBy5m = true;
        }
      }
    }

    if (skipBy5m) continue;

    // === 踏空翻转检测 ===
    if (dev !== null) {
      // 多头趋势但价格跌破MA488超过阈值 → 踏空
      if (trend === 'bullish' && dev < -priceDevThreshold && position === 'long') {
        signals.push({ time: row.open_time, type: 'FLIP', price: c, reason: `踏空翻转: 价格偏离${dev.toFixed(1)}%` });
        const pnl = (c - entryPrice) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
        else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ entry: entryPrice, exit: c, pnl, type: 'LONG', entryTime, exitTime: row.open_time });
        position = null; // 先平仓，不翻转
        continue;
      }
      // 空头趋势但价格涨破MA488超过阈值 → 踏空
      if (trend === 'bearish' && dev > priceDevThreshold && position === 'short') {
        signals.push({ time: row.open_time, type: 'FLIP', price: c, reason: `踏空翻转: 价格偏离${dev.toFixed(1)}%` });
        const pnl = (entryPrice - c) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
        else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ entry: entryPrice, exit: c, pnl, type: 'SHORT', entryTime, exitTime: row.open_time });
        position = null;
        continue;
      }
    }

    // === 止损检查 ===
    if (position === 'long') {
      const loss = (c - entryPrice) / entryPrice * 100;
      if (loss < -stopLossPct) {
        signals.push({ time: row.open_time, type: 'STOP', price: c });
        totalPnL += loss;
        lossCount++; maxLoss = Math.min(maxLoss, loss);
        trades.push({ entry: entryPrice, exit: c, pnl: loss, type: 'LONG', entryTime, exitTime: row.open_time });
        position = null;
        continue;
      }
    } else if (position === 'short') {
      const loss = (entryPrice - c) / entryPrice * 100;
      if (loss < -stopLossPct) {
        signals.push({ time: row.open_time, type: 'STOP', price: c });
        totalPnL += loss;
        lossCount++; maxLoss = Math.min(maxLoss, loss);
        trades.push({ entry: entryPrice, exit: c, pnl: loss, type: 'SHORT', entryTime, exitTime: row.open_time });
        position = null;
        continue;
      }
    }

    // === 正常信号 ===
    if (trend === 'bearish') {
      if (o > ma288 && c < ma288) {
        if (position === 'long') {
          const pnl = (c - entryPrice) / entryPrice * 100;
          totalPnL += pnl;
          if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
          else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
          trades.push({ entry: entryPrice, exit: c, pnl, type: 'LONG', entryTime, exitTime: row.open_time });
        }
        signals.push({ time: row.open_time, type: 'SHORT', price: c });
        position = 'short';
        entryPrice = c;
        entryTime = row.open_time;
      } else if (o < ma288 && c > ma288 && position === 'short') {
        const pnl = (entryPrice - c) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
        else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ entry: entryPrice, exit: c, pnl, type: 'SHORT', entryTime, exitTime: row.open_time });
        signals.push({ time: row.open_time, type: 'COVER', price: c });
        position = null;
      }
    } else if (trend === 'bullish') {
      if (o < ma288 && c > ma288) {
        if (position === 'short') {
          const pnl = (entryPrice - c) / entryPrice * 100;
          totalPnL += pnl;
          if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
          else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
          trades.push({ entry: entryPrice, exit: c, pnl, type: 'SHORT', entryTime, exitTime: row.open_time });
        }
        signals.push({ time: row.open_time, type: 'LONG', price: c });
        position = 'long';
        entryPrice = c;
        entryTime = row.open_time;
      } else if (o > ma288 && c < ma288 && position === 'long') {
        const pnl = (c - entryPrice) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
        else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ entry: entryPrice, exit: c, pnl, type: 'LONG', entryTime, exitTime: row.open_time });
        signals.push({ time: row.open_time, type: 'STOP', price: c });
        position = null;
      }
    }
  }

  return {
    signalCount: signals.length,
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
    signals
  };
}

// ============================================================
// 测试不同5m过滤模式
// ============================================================
console.log("\n--- 5m过滤模式对比 ---");
console.log("模式                    | 信号数 | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 最大亏");
console.log("-".repeat(100));

const modes = [
  { label: '基准(无5m过滤)', mode: 'none', threshold: 0 },
  { label: 'strong_only(0.3%)', mode: 'strong_only', threshold: 0.3 },
  { label: 'strong_only(0.5%)', mode: 'strong_only', threshold: 0.5 },
  { label: 'strong_only(1.0%)', mode: 'strong_only', threshold: 1.0 },
  { label: 'strong_only(1.5%)', mode: 'strong_only', threshold: 1.5 },
  { label: 'adaptive(0.3%)', mode: 'adaptive', threshold: 0.3 },
  { label: 'adaptive(0.5%)', mode: 'adaptive', threshold: 0.5 },
  { label: 'adaptive(1.0%)', mode: 'adaptive', threshold: 1.0 },
];

const results = [];
for (const m of modes) {
  const r = runAntiMissV2(df_30m_valid, {
    slopeThreshold: 5,
    bbWidthThreshold: 2.0,
    filter5mMode: m.mode,
    strong5mThreshold: m.threshold,
    priceDevThreshold: 5.0,
    stopLossPct: 2.0
  });
  results.push({ label: m.label, ...r });
  console.log(
    `${m.label.padEnd(23)} | ${String(r.signalCount).padStart(6)} | ${String(r.tradeCount).padStart(6)} | ` +
    `${r.winRate.toFixed(1).padStart(5)}% | ${r.totalPnL.toFixed(2).padStart(8)}% | ${r.avgPnL.toFixed(3).padStart(8)}% | ` +
    `${r.maxWin.toFixed(2).padStart(7)}% | ${r.maxLoss.toFixed(2).padStart(7)}%`
  );
}

// ============================================================
// 不同止损阈值测试
// ============================================================
console.log("\n--- 止损阈值测试 (strong_only 0.5%) ---");
console.log("止损(%)  | 信号数 | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 最大亏");
console.log("-".repeat(90));

const stopLosses = [1.0, 1.5, 2.0, 2.5, 3.0, 5.0];
for (const sl of stopLosses) {
  const r = runAntiMissV2(df_30m_valid, {
    slopeThreshold: 5,
    bbWidthThreshold: 2.0,
    filter5mMode: 'strong_only',
    strong5mThreshold: 0.5,
    priceDevThreshold: 5.0,
    stopLossPct: sl
  });
  console.log(
    `${String(sl).padStart(8)} | ${String(r.signalCount).padStart(6)} | ${String(r.tradeCount).padStart(6)} | ` +
    `${r.winRate.toFixed(1).padStart(5)}% | ${r.totalPnL.toFixed(2).padStart(8)}% | ${r.avgPnL.toFixed(3).padStart(8)}% | ` +
    `${r.maxWin.toFixed(2).padStart(7)}% | ${r.maxLoss.toFixed(2).padStart(7)}%`
  );
}

// ============================================================
// 最优配置详细分析
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【最优配置详细分析】");
console.log("=".repeat(70));

// 选择最佳组合
const bestConfig = runAntiMissV2(df_30m_valid, {
  slopeThreshold: 5,
  bbWidthThreshold: 2.0,
  filter5mMode: 'strong_only',
  strong5mThreshold: 0.5,
  priceDevThreshold: 5.0,
  stopLossPct: 2.0
});

console.log(`\n配置: slope=5bps + bbw=2% + 5m强反向过滤(0.5%) + 踏空翻转(5%) + 止损2%`);
console.log(`\n统计:`);
console.log(`  信号总数: ${bestConfig.signalCount}`);
console.log(`  完成交易: ${bestConfig.tradeCount} 笔`);
console.log(`  胜率: ${bestConfig.winRate.toFixed(1)}%`);
console.log(`  总收益: ${bestConfig.totalPnL.toFixed(2)}%`);
console.log(`  平均收益: ${bestConfig.avgPnL.toFixed(3)}%`);
console.log(`  最大盈利: ${bestConfig.maxWin.toFixed(2)}%`);
console.log(`  最大亏损: ${bestConfig.maxLoss.toFixed(2)}%`);
console.log(`  盈亏比: ${bestConfig.profitFactor.toFixed(2)}`);

// 最近交易
console.log("\n--- 最近15笔交易 ---");
for (const t of bestConfig.trades.slice(-15)) {
  const duration = (t.exitTime - t.entryTime) / 3600000;
  const pnlSign = t.pnl >= 0 ? '+' : '';
  console.log(`  ${t.type.padEnd(5)} ${t.entry.toFixed(2)} → ${t.exit.toFixed(2)} | PnL: ${pnlSign}${t.pnl.toFixed(4)}% | 持仓: ${duration.toFixed(1)}h`);
}

// ============================================================
// 策略演进对比
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【策略演进对比】");
console.log("=".repeat(70));

const baseline = runAntiMissV2(df_30m_valid, {
  slopeThreshold: 0, bbWidthThreshold: 0, filter5mMode: 'none', stopLossPct: 999
});

const v2 = runAntiMissV2(df_30m_valid, {
  slopeThreshold: 5, bbWidthThreshold: 2.0, filter5mMode: 'none', stopLossPct: 999
});

const v3 = bestConfig;

console.log(`
版本                | 信号数 | 交易数 | 胜率   | 总收益   | 平均收益 | 最大亏
--------------------|--------|--------|--------|----------|----------|--------
第一次(无过滤)      | ${String(baseline.signalCount).padStart(6)} | ${String(baseline.tradeCount).padStart(6)} | ${baseline.winRate.toFixed(1).padStart(5)}% | ${baseline.totalPnL >= 0 ? '+' : ''}${baseline.totalPnL.toFixed(2).padStart(7)}% | ${baseline.avgPnL >= 0 ? '+' : ''}${baseline.avgPnL.toFixed(3).padStart(7)}% | ${baseline.maxLoss.toFixed(2).padStart(7)}%
第二次(slope+bbw)   | ${String(v2.signalCount).padStart(6)} | ${String(v2.tradeCount).padStart(6)} | ${v2.winRate.toFixed(1).padStart(5)}% | ${v2.totalPnL >= 0 ? '+' : ''}${v2.totalPnL.toFixed(2).padStart(7)}% | ${v2.avgPnL >= 0 ? '+' : ''}${v2.avgPnL.toFixed(3).padStart(7)}% | ${v2.maxLoss.toFixed(2).padStart(7)}%
第三次(+5m强过滤)   | ${String(v3.signalCount).padStart(6)} | ${String(v3.tradeCount).padStart(6)} | ${v3.winRate.toFixed(1).padStart(5)}% | ${v3.totalPnL >= 0 ? '+' : ''}${v3.totalPnL.toFixed(2).padStart(7)}% | ${v3.avgPnL >= 0 ? '+' : ''}${v3.avgPnL.toFixed(3).padStart(7)}% | ${v3.maxLoss.toFixed(2).padStart(7)}%
`);

console.log("第三次分析v2完成！");
