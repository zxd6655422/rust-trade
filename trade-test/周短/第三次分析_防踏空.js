/**
 * 第三次分析: 防踏空策略
 * 核心思路:
 * 1. 检测踏空状态: 价格偏离MA488太远 → 暂停原方向交易
 * 2. 5m早期预警: 5m均线反应比30m快6倍，5m方向反转时提前警觉
 * 3. 价格跌破/突破MA488 → 直接翻转方向
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
console.log("第三次分析: 防踏空策略");
console.log("=".repeat(70));

const df_30m = loadCSV('kline_30m_202607222207.csv', 'open_time');
const df_5m = loadCSV('kline_5m_202607222208.csv', 'open_time');

// ============================================================
// 计算技术指标
// ============================================================
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
console.log(`30m有效数据: ${df_30m_valid.length} bars`);
console.log(`5m有效数据: ${df_5m_valid.length} bars`);

// ============================================================
// 构建5m趋势索引 (用于快速查找某个时间点的5m趋势)
// ============================================================
// 对于每个30m K线的时间，找到对应的5m状态
function build5mTrendMap(df5m) {
  const map = new Map(); // timestamp -> {trend, ma288, ma488, priceDevMa488}
  for (const r of df5m) {
    if (r.ma288 === null || r.ma488 === null) continue;
    const trend = r.ma288 > r.ma488 ? 'bullish' : 'bearish';
    map.set(r.open_time.getTime(), {
      trend,
      ma288: r.ma288,
      ma488: r.ma488,
      priceDevMa488: r.priceDevMa488,
      close: r.close
    });
  }
  return map;
}

const trendMap5m = build5mTrendMap(df_5m_valid);

// 获取某时间点最近的5m趋势
function get5mTrendAt(time) {
  const t = time.getTime();
  // 找最近的5m K线 (往前找)
  let best = null;
  let bestDiff = Infinity;
  for (const [ts, data] of trendMap5m) {
    const diff = t - ts;
    if (diff >= 0 && diff < bestDiff) {
      bestDiff = diff;
      best = data;
    }
    if (diff < 0) break; // 已经过了
  }
  return best;
}

// ============================================================
// 踏空检测逻辑
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【踏空状态分析】");
console.log("=".repeat(70));

// 统计价格偏离MA488的分布
const deviations = df_30m_valid.map(r => r.priceDevMa488).filter(v => v !== null);
const absDevs = deviations.map(Math.abs);
absDevs.sort((a, b) => a - b);

console.log("\n价格偏离MA488的分布:");
console.log(`  中位数: ${absDevs[Math.floor(absDevs.length/2)].toFixed(2)}%`);
console.log(`  75%分位: ${absDevs[Math.floor(absDevs.length*0.75)].toFixed(2)}%`);
console.log(`  90%分位: ${absDevs[Math.floor(absDevs.length*0.9)].toFixed(2)}%`);
console.log(`  95%分位: ${absDevs[Math.floor(absDevs.length*0.95)].toFixed(2)}%`);
console.log(`  最大值: ${absDevs[absDevs.length-1].toFixed(2)}%`);

// 检测历史上踏空的情况
let 踏空_count = 0;
let 踏空_details = [];
for (let i = 0; i < df_30m_valid.length; i++) {
  const r = df_30m_valid[i];
  const trend = r.ma288 > r.ma488 ? 'bullish' : 'bearish';
  const dev = r.priceDevMa488;

  // 踏空: 趋势看多但价格跌破MA488超过3%，或趋势看空但价格涨破MA488超过3%
  if (trend === 'bullish' && dev < -3) {
    踏空_count++;
    if (踏空_details.length < 10) {
      踏空_details.push({ time: r.open_time, trend, dev, price: r.close, ma488: r.ma488 });
    }
  } else if (trend === 'bearish' && dev > 3) {
    踏空_count++;
    if (踏空_details.length < 10) {
      踏空_details.push({ time: r.open_time, trend, dev, price: r.close, ma488: r.ma488 });
    }
  }
}

console.log(`\n踏空事件(偏离>3%): ${踏空_count} 次`);
console.log(`\n踏空案例:`);
for (const d of 踏空_details) {
  console.log(`  ${d.time.toISOString()} | 趋势:${d.trend} | 偏离:${d.dev.toFixed(2)}% | 价格:${d.price.toFixed(2)} | MA488:${d.ma488.toFixed(2)}`);
}

// ============================================================
// 策略回测: 防踏空版本
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【策略回测】防踏空版本");
console.log("=".repeat(70));

// 防踏空策略:
// 1. 基础: slope=5bps + bbw=2% (来自第二次分析)
// 2. 新增: 价格偏离MA488超过阈值时，暂停原方向交易
// 3. 新增: 5m趋势与30m相反时，暂停原方向交易
// 4. 新增: 价格跌破/突破MA488时，直接翻转方向

function runAntiMissBacktest(df30, config) {
  const {
    slopeThreshold = 5,
    bbWidthThreshold = 2.0,
    // 踏空检测参数
    priceDevThreshold = 3.0,    // 价格偏离MA488阈值(%)
    use5mFilter = true,         // 是否使用5m过滤
    useFlipOnMa488 = true,      // 是否在价格跌破MA488时翻转
    // 止损参数
    stopLossPct = 2.0,          // 止损百分比
  } = config;

  const signals = [];
  let position = null;
  let entryPrice = 0, entryTime = null, entryType = null;
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

    // === 基础过滤 (来自第二次分析) ===
    if (slopeThreshold > 0 && slope !== null && Math.abs(slope) < slopeThreshold) continue;
    if (bbWidthThreshold > 0 && bbw !== null && bbw < bbWidthThreshold) continue;

    // === 踏空检测 ===
    // 如果价格偏离MA488太远，暂停原方向交易
    let is踏空 = false;
    if (dev !== null) {
      if (trend === 'bullish' && dev < -priceDevThreshold) {
        is踏空 = true; // 趋势看多但价格跌破MA488
      } else if (trend === 'bearish' && dev > priceDevThreshold) {
        is踏空 = true; // 趋势看空但价格涨破MA488
      }
    }

    // === 5m趋势过滤 ===
    let trend5m = null;
    if (use5mFilter) {
      const data5m = get5mTrendAt(row.open_time);
      if (data5m) trend5m = data5m.trend;
    }

    // === 价格跌破/突破MA488翻转 ===
    if (useFlipOnMa488 && is踏空) {
      // 踏空状态下，如果持仓方向与踏空方向一致，强制平仓
      if (position === 'long' && trend === 'bullish' && dev < -priceDevThreshold) {
        signals.push({ time: row.open_time, type: 'FLIP_SHORT', price: c, reason: '踏空翻转: 价格跌破MA488' });
        const pnl = (c - entryPrice) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
        else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ entry: entryPrice, exit: c, pnl, type: 'LONG', entryTime, exitTime: row.open_time });
        // 翻转为空
        position = 'short';
        entryPrice = c;
        entryTime = row.open_time;
        entryType = 'SHORT';
        continue;
      } else if (position === 'short' && trend === 'bearish' && dev > priceDevThreshold) {
        signals.push({ time: row.open_time, type: 'FLIP_LONG', price: c, reason: '踏空翻转: 价格涨破MA488' });
        const pnl = (entryPrice - c) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
        else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ entry: entryPrice, exit: c, pnl, type: 'SHORT', entryTime, exitTime: row.open_time });
        // 翻转为多
        position = 'long';
        entryPrice = c;
        entryTime = row.open_time;
        entryType = 'LONG';
        continue;
      }
    }

    // 踏空状态下不开新仓
    if (is踏空) continue;

    // === 5m方向过滤: 如果5m方向与30m相反，不开仓 ===
    if (use5mFilter && trend5m !== null && trend5m !== trend) continue;

    // === 止损检查 ===
    if (position === 'long') {
      const loss = (c - entryPrice) / entryPrice * 100;
      if (loss < -stopLossPct) {
        signals.push({ time: row.open_time, type: 'STOP', price: c, reason: `止损: 亏损${loss.toFixed(2)}%` });
        totalPnL += loss;
        lossCount++; maxLoss = Math.min(maxLoss, loss);
        trades.push({ entry: entryPrice, exit: c, pnl: loss, type: 'LONG', entryTime, exitTime: row.open_time });
        position = null;
        continue;
      }
    } else if (position === 'short') {
      const loss = (entryPrice - c) / entryPrice * 100;
      if (loss < -stopLossPct) {
        signals.push({ time: row.open_time, type: 'STOP', price: c, reason: `止损: 亏损${loss.toFixed(2)}%` });
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
        signals.push({ time: row.open_time, type: 'SHORT', price: c, reason: '空头趋势反弹受阻MA288' });
        position = 'short';
        entryPrice = c;
        entryTime = row.open_time;
        entryType = 'SHORT';
      } else if (o < ma288 && c > ma288) {
        if (position === 'short') {
          const pnl = (entryPrice - c) / entryPrice * 100;
          totalPnL += pnl;
          if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
          else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
          trades.push({ entry: entryPrice, exit: c, pnl, type: 'SHORT', entryTime, exitTime: row.open_time });
          signals.push({ time: row.open_time, type: 'COVER', price: c, reason: '空头止损: 收盘站上MA288' });
          position = null;
        }
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
        signals.push({ time: row.open_time, type: 'LONG', price: c, reason: '多头趋势回落获撑MA288' });
        position = 'long';
        entryPrice = c;
        entryTime = row.open_time;
        entryType = 'LONG';
      } else if (o > ma288 && c < ma288) {
        if (position === 'long') {
          const pnl = (c - entryPrice) / entryPrice * 100;
          totalPnL += pnl;
          if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
          else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
          trades.push({ entry: entryPrice, exit: c, pnl, type: 'LONG', entryTime, exitTime: row.open_time });
          signals.push({ time: row.open_time, type: 'STOP', price: c, reason: '多头止损: 收盘跌破MA288' });
          position = null;
        }
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
// 对比测试: 不同踏空阈值
// ============================================================
console.log("\n--- 踏空阈值测试 ---");
console.log("偏离阈值(%) | 信号数 | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 最大亏");
console.log("-".repeat(90));

const devThresholds = [0, 2, 3, 4, 5, 6, 8];
for (const dev of devThresholds) {
  const result = runAntiMissBacktest(df_30m_valid, {
    slopeThreshold: 5,
    bbWidthThreshold: 2.0,
    priceDevThreshold: dev,
    use5mFilter: false,
    useFlipOnMa488: false,
    stopLossPct: 999 // 不止损，单独看踏空效果
  });
  console.log(
    `${String(dev).padStart(12)} | ${String(result.signalCount).padStart(6)} | ${String(result.tradeCount).padStart(6)} | ` +
    `${result.winRate.toFixed(1).padStart(5)}% | ${result.totalPnL.toFixed(2).padStart(8)}% | ${result.avgPnL.toFixed(3).padStart(8)}% | ` +
    `${result.maxWin.toFixed(2).padStart(7)}% | ${result.maxLoss.toFixed(2).padStart(7)}%`
  );
}

// ============================================================
// 对比测试: 5m过滤效果
// ============================================================
console.log("\n--- 5m趋势过滤测试 ---");
console.log("配置                          | 信号数 | 交易数 | 胜率   | 总收益   | 平均收益");
console.log("-".repeat(85));

const configs = [
  { label: '基准(slope=5,bbw=2)', slope: 5, bbw: 2, dev: 99, use5m: false, flip: false, sl: 999 },
  { label: '+踏空检测(3%)', slope: 5, bbw: 2, dev: 3, use5m: false, flip: false, sl: 999 },
  { label: '+5m过滤', slope: 5, bbw: 2, dev: 3, use5m: true, flip: false, sl: 999 },
  { label: '+MA488翻转', slope: 5, bbw: 2, dev: 3, use5m: true, flip: true, sl: 999 },
  { label: '+止损2%', slope: 5, bbw: 2, dev: 3, use5m: true, flip: true, sl: 2 },
  { label: '+止损1.5%', slope: 5, bbw: 2, dev: 3, use5m: true, flip: true, sl: 1.5 },
];

const results = [];
for (const cfg of configs) {
  const r = runAntiMissBacktest(df_30m_valid, {
    slopeThreshold: cfg.slope,
    bbWidthThreshold: cfg.bbw,
    priceDevThreshold: cfg.dev,
    use5mFilter: cfg.use5m,
    useFlipOnMa488: cfg.flip,
    stopLossPct: cfg.sl
  });
  results.push({ label: cfg.label, ...r });
  console.log(
    `${cfg.label.padEnd(29)} | ${String(r.signalCount).padStart(6)} | ${String(r.tradeCount).padStart(6)} | ` +
    `${r.winRate.toFixed(1).padStart(5)}% | ${r.totalPnL.toFixed(2).padStart(8)}% | ${r.avgPnL.toFixed(3).padStart(8)}%`
  );
}

// ============================================================
// 最优配置详细分析
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【最优配置详细分析】");
console.log("=".repeat(70));

const best = results[results.length - 1]; // +止损1.5%
console.log(`\n配置: slope=5bps + bbw=2% + 踏空检测3% + 5m过滤 + MA488翻转 + 止损1.5%`);
console.log(`\n统计:`);
console.log(`  信号总数: ${best.signalCount}`);
console.log(`  完成交易: ${best.tradeCount} 笔`);
console.log(`  胜率: ${best.winRate.toFixed(1)}%`);
console.log(`  总收益: ${best.totalPnL.toFixed(2)}%`);
console.log(`  平均收益: ${best.avgPnL.toFixed(3)}%`);
console.log(`  最大盈利: ${best.maxWin.toFixed(2)}%`);
console.log(`  最大亏损: ${best.maxLoss.toFixed(2)}%`);
console.log(`  盈亏比: ${best.profitFactor.toFixed(2)}`);

// ============================================================
// 信号类型统计
// ============================================================
console.log("\n--- 信号类型统计 ---");
const typeCounts = {};
for (const s of best.signals) {
  typeCounts[s.type] = (typeCounts[s.type] || 0) + 1;
}
for (const [type, count] of Object.entries(typeCounts)) {
  console.log(`  ${type}: ${count} 次`);
}

// ============================================================
// 最近交易明细
// ============================================================
console.log("\n--- 最近20笔交易 ---");
for (const t of best.trades.slice(-20)) {
  const duration = (t.exitTime - t.entryTime) / 3600000;
  const pnlSign = t.pnl >= 0 ? '+' : '';
  console.log(`  ${t.type.padEnd(5)} ${t.entry.toFixed(2)} → ${t.exit.toFixed(2)} | PnL: ${pnlSign}${t.pnl.toFixed(4)}% | 持仓: ${duration.toFixed(1)}h`);
}

// ============================================================
// 策略对比总结
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【策略演进对比】");
console.log("=".repeat(70));

console.log(`
版本              | 信号数 | 交易数 | 胜率   | 总收益   | 平均收益
------------------|--------|--------|--------|----------|----------
第一次(无过滤)    |    812 |    409 |  16.6% |  +30.26% |  +0.074%
第二次(slope+bbw) |    153 |     79 |  27.8% |  +26.74% |  +0.338%
第三次(+防踏空)   | ${String(best.signalCount).padStart(6)} | ${String(best.tradeCount).padStart(6)} | ${best.winRate.toFixed(1).padStart(5)}% | ${best.totalPnL >= 0 ? '+' : ''}${best.totalPnL.toFixed(2).padStart(7)}% | ${best.avgPnL >= 0 ? '+' : ''}${best.avgPnL.toFixed(3).padStart(7)}%
`);

console.log("第三次分析完成！");
