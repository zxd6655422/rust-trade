/**
 * 第二次分析: 信号摩擦过滤优化
 * 在第一次分析基础上增加:
 * 1. MA288倾斜率过滤 - 倾斜率太小说明趋势不明确
 * 2. 布林带带宽过滤 - 带宽收窄说明震荡市，不适合交易
 * 3. 对比过滤前后的信号数量和质量
 */

const fs = require('fs');

// ============================================================
// 1. 加载数据
// ============================================================
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
console.log("第二次分析: 信号摩擦过滤优化");
console.log("=".repeat(70));

const df_30m = loadCSV('kline_30m_202607222207.csv', 'open_time');
const df_5m = loadCSV('kline_5m_202607222208.csv', 'open_time');

// ============================================================
// 2. 计算技术指标
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

  // 布林带 (100, 2)
  const bbMid = calcMA(100);
  const bbUpper = new Array(df.length).fill(null);
  const bbLower = new Array(df.length).fill(null);
  const bbWidth = new Array(df.length).fill(null);  // 带宽百分比
  for (let i = 99; i < df.length; i++) {
    let sumSq = 0;
    for (let j = i - 99; j <= i; j++) {
      sumSq += (closes[j] - bbMid[i]) ** 2;
    }
    const std = Math.sqrt(sumSq / 100);
    bbUpper[i] = bbMid[i] + 2 * std;
    bbLower[i] = bbMid[i] - 2 * std;
    bbWidth[i] = (bbUpper[i] - bbLower[i]) / bbMid[i] * 100;  // 带宽百分比
  }

  // MA288 斜率 (5期变化率, bps)
  const ma288Slope = new Array(df.length).fill(null);
  for (let i = 5; i < df.length; i++) {
    if (ma288[i] !== null && ma288[i-5] !== null && ma288[i-5] !== 0) {
      ma288Slope[i] = (ma288[i] - ma288[i-5]) / ma288[i-5] * 10000;
    }
  }

  // MA288 与 MA488 的差值百分比 (用于判断扩散程度)
  const maSpread = new Array(df.length).fill(null);
  for (let i = 0; i < df.length; i++) {
    if (ma288[i] !== null && ma488[i] !== null && ma488[i] !== 0) {
      maSpread[i] = (ma288[i] - ma488[i]) / ma488[i] * 100;
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
    df[i].maSpread = maSpread[i];
  }
  return df;
}

console.log("\n计算技术指标...");
addIndicators(df_30m);
addIndicators(df_5m);

const df_30m_valid = df_30m.filter(r => r.ma288 !== null && r.ma488 !== null);
console.log(`30m有效数据: ${df_30m_valid.length} bars`);

// ============================================================
// 3. 不同过滤参数的对比测试
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【参数优化对比】不同过滤阈值的效果");
console.log("=".repeat(70));

// 测试不同的倾斜率阈值
const slopeThresholds = [0, 1, 2, 3, 5, 8, 10];
// 测试不同的带宽阈值
const bbWidthThresholds = [0, 1.0, 1.5, 2.0, 2.5, 3.0];

// 基准: 无过滤
function runBacktest(df, slopeThreshold, bbWidthThreshold, label) {
  const signals = [];
  let position = null;
  let entryPrice = 0, entryTime = null, entryType = null;
  let totalPnL = 0, winCount = 0, lossCount = 0;
  let maxWin = 0, maxLoss = 0;
  const trades = [];

  for (let i = 1; i < df.length; i++) {
    const row = df[i];
    const ma288 = row.ma288;
    const ma488 = row.ma488;
    const o = row.open, c = row.close;
    const slope = row.ma288Slope;
    const bbw = row.bbWidth;

    let trend;
    if (ma288 < ma488) trend = 'bearish';
    else if (ma288 > ma488) trend = 'bullish';
    else continue;

    // === 过滤条件 ===
    // 1. 倾斜率过滤: 倾斜率绝对值小于阈值则跳过
    if (slopeThreshold > 0 && slope !== null && Math.abs(slope) < slopeThreshold) {
      continue;
    }
    // 2. 布林带带宽过滤: 带宽小于阈值则跳过 (震荡市)
    if (bbWidthThreshold > 0 && bbw !== null && bbw < bbWidthThreshold) {
      continue;
    }

    if (trend === 'bearish') {
      if (o > ma288 && c < ma288) {
        if (position !== 'short') {
          signals.push({ time: row.open_time, type: 'SHORT', price: c, slope, bbw });
          if (position === 'long') {
            // 平多开空
            const pnl = (c - entryPrice) / entryPrice * 100;
            totalPnL += pnl;
            if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
            else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
            trades.push({ entry: entryPrice, exit: c, pnl, type: 'LONG', entryTime, exitTime: row.open_time });
          }
          position = 'short';
          entryPrice = c;
          entryTime = row.open_time;
          entryType = 'SHORT';
        }
      } else if (o < ma288 && c > ma288) {
        if (position === 'short') {
          signals.push({ time: row.open_time, type: 'COVER', price: c, slope, bbw });
          const pnl = (entryPrice - c) / entryPrice * 100;
          totalPnL += pnl;
          if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
          else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
          trades.push({ entry: entryPrice, exit: c, pnl, type: 'SHORT', entryTime, exitTime: row.open_time });
          position = null;
        }
      }
    } else if (trend === 'bullish') {
      if (o < ma288 && c > ma288) {
        if (position !== 'long') {
          signals.push({ time: row.open_time, type: 'LONG', price: c, slope, bbw });
          if (position === 'short') {
            // 平空开多
            const pnl = (entryPrice - c) / entryPrice * 100;
            totalPnL += pnl;
            if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
            else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
            trades.push({ entry: entryPrice, exit: c, pnl, type: 'SHORT', entryTime, exitTime: row.open_time });
          }
          position = 'long';
          entryPrice = c;
          entryTime = row.open_time;
          entryType = 'LONG';
        }
      } else if (o > ma288 && c < ma288) {
        if (position === 'long') {
          signals.push({ time: row.open_time, type: 'STOP', price: c, slope, bbw });
          const pnl = (c - entryPrice) / entryPrice * 100;
          totalPnL += pnl;
          if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
          else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
          trades.push({ entry: entryPrice, exit: c, pnl, type: 'LONG', entryTime, exitTime: row.open_time });
          position = null;
        }
      }
    }
  }

  return {
    label,
    slopeThreshold,
    bbWidthThreshold,
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
// 4. 单变量测试: 倾斜率阈值
// ============================================================
console.log("\n--- 倾斜率阈值测试 (无带宽过滤) ---");
console.log("阈值(bps) | 信号数 | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 最大亏");
console.log("-".repeat(85));

for (const slope of slopeThresholds) {
  const result = runBacktest(df_30m_valid, slope, 0, `slope=${slope}`);
  console.log(
    `${String(slope).padStart(9)} | ${String(result.signalCount).padStart(6)} | ${String(result.tradeCount).padStart(6)} | ` +
    `${result.winRate.toFixed(1).padStart(5)}% | ${result.totalPnL.toFixed(2).padStart(8)}% | ${result.avgPnL.toFixed(3).padStart(8)}% | ` +
    `${result.maxWin.toFixed(2).padStart(7)}% | ${result.maxLoss.toFixed(2).padStart(7)}%`
  );
}

// ============================================================
// 5. 单变量测试: 布林带带宽阈值
// ============================================================
console.log("\n--- 布林带带宽阈值测试 (无倾斜率过滤) ---");
console.log("阈值(%)  | 信号数 | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 最大亏");
console.log("-".repeat(85));

for (const bbw of bbWidthThresholds) {
  const result = runBacktest(df_30m_valid, 0, bbw, `bbw=${bbw}`);
  console.log(
    `${String(bbw).padStart(8)} | ${String(result.signalCount).padStart(6)} | ${String(result.tradeCount).padStart(6)} | ` +
    `${result.winRate.toFixed(1).padStart(5)}% | ${result.totalPnL.toFixed(2).padStart(8)}% | ${result.avgPnL.toFixed(3).padStart(8)}% | ` +
    `${result.maxWin.toFixed(2).padStart(7)}% | ${result.maxLoss.toFixed(2).padStart(7)}%`
  );
}

// ============================================================
// 6. 双变量组合测试: 倾斜率 + 带宽
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【双变量组合测试】倾斜率 + 布林带带宽");
console.log("=".repeat(70));

const combos = [
  [0, 0],    // 基准
  [2, 0],    // 仅倾斜率
  [0, 2.0],  // 仅带宽
  [2, 2.0],  // 双重过滤
  [3, 2.0],  // 倾斜率更严
  [2, 2.5],  // 带宽更严
  [3, 2.5],  // 双重更严
  [5, 2.0],  // 高倾斜率
  [5, 3.0],  // 高倾斜率+高带宽
];

console.log("\n组合                | 信号数 | 交易数 | 胜率   | 总收益   | 平均收益 | 信号减少");
console.log("-".repeat(95));

const baseResult = runBacktest(df_30m_valid, 0, 0, '基准');
const baseSignals = baseResult.signalCount;

for (const [slope, bbw] of combos) {
  const result = runBacktest(df_30m_valid, slope, bbw, `slope=${slope},bbw=${bbw}`);
  const reduction = ((1 - result.signalCount / baseSignals) * 100).toFixed(1);
  const label = `slope=${slope},bbw=${bbw}`.padEnd(20);
  console.log(
    `${label} | ${String(result.signalCount).padStart(6)} | ${String(result.tradeCount).padStart(6)} | ` +
    `${result.winRate.toFixed(1).padStart(5)}% | ${result.totalPnL.toFixed(2).padStart(8)}% | ${result.avgPnL.toFixed(3).padStart(8)}% | ` +
    `-${reduction}%`
  );
}

// ============================================================
// 7. 最优组合详细分析
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【最优组合详细分析】");
console.log("=".repeat(70));

// 选择一个平衡的组合: slope=3, bbw=2.0
const optimal = runBacktest(df_30m_valid, 3, 2.0, 'slope=3,bbw=2.0');

console.log(`\n推荐组合: MA288倾斜率阈值=3bps, 布林带带宽阈值=2.0%`);
console.log(`\n过滤后统计:`);
console.log(`  信号总数: ${optimal.signalCount} (原始: ${baseSignals}, 减少: ${((1-optimal.signalCount/baseSignals)*100).toFixed(1)}%)`);
console.log(`  完成交易: ${optimal.tradeCount} 笔`);
console.log(`  胜率: ${optimal.winRate.toFixed(1)}%`);
console.log(`  总收益: ${optimal.totalPnL.toFixed(2)}%`);
console.log(`  平均收益: ${optimal.avgPnL.toFixed(3)}%`);
console.log(`  最大盈利: ${optimal.maxWin.toFixed(2)}%`);
console.log(`  最大亏损: ${optimal.maxLoss.toFixed(2)}%`);
console.log(`  盈亏比: ${optimal.profitFactor.toFixed(2)}`);

// 与基准对比
console.log(`\n与基准(无过滤)对比:`);
console.log(`  信号减少: ${baseSignals - optimal.signalCount} 个 (${((1-optimal.signalCount/baseSignals)*100).toFixed(1)}%)`);
console.log(`  交易减少: ${baseResult.tradeCount - optimal.tradeCount} 笔`);
console.log(`  胜率变化: ${baseResult.winRate.toFixed(1)}% → ${optimal.winRate.toFixed(1)}% (${(optimal.winRate - baseResult.winRate > 0 ? '+' : '')}${(optimal.winRate - baseResult.winRate).toFixed(1)}%)`);
console.log(`  总收益变化: ${baseResult.totalPnL.toFixed(2)}% → ${optimal.totalPnL.toFixed(2)}% (${(optimal.totalPnL - baseResult.totalPnL > 0 ? '+' : '')}${(optimal.totalPnL - baseResult.totalPnL).toFixed(2)}%)`);

// ============================================================
// 8. 信号间隔分析 (过滤后)
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【过滤后信号间隔分析】");
console.log("=".repeat(70));

if (optimal.signals.length > 1) {
  const intervals = [];
  for (let i = 1; i < optimal.signals.length; i++) {
    intervals.push((optimal.signals[i].time - optimal.signals[i-1].time) / 60000);
  }
  const short30 = intervals.filter(x => x < 30);
  const short15 = intervals.filter(x => x < 15);
  const avg = intervals.reduce((a,b) => a+b, 0) / intervals.length;
  const sorted = [...intervals].sort((a,b) => a-b);
  const median = sorted[Math.floor(sorted.length / 2)];

  console.log(`过滤后信号间隔统计:`);
  console.log(`  总信号数: ${optimal.signals.length}`);
  console.log(`  平均间隔: ${avg.toFixed(1)} 分钟`);
  console.log(`  中位间隔: ${median.toFixed(1)} 分钟`);
  console.log(`  <30min的密集信号: ${short30.length} 次 (${(short30.length/intervals.length*100).toFixed(1)}%)`);
  console.log(`  <15min的密集信号: ${short15.length} 次 (${(short15.length/intervals.length*100).toFixed(1)}%)`);

  // 与第一次分析对比
  console.log(`\n与第一次分析(无过滤)对比:`);
  console.log(`  平均间隔: 172.2分钟 → ${avg.toFixed(1)}分钟`);
  console.log(`  <30min密集信号: 55.7% → ${(short30.length/intervals.length*100).toFixed(1)}%`);
  console.log(`  <15min密集信号: 39.3% → ${(short15.length/intervals.length*100).toFixed(1)}%`);
}

// ============================================================
// 9. 最近交易明细 (过滤后)
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【过滤后最近15笔交易】");
console.log("=".repeat(70));

for (const t of optimal.trades.slice(-15)) {
  const duration = (t.exitTime - t.entryTime) / 3600000;
  const pnlSign = t.pnl >= 0 ? '+' : '';
  console.log(`  ${t.type.padEnd(5)} ${t.entry.toFixed(2)} → ${t.exit.toFixed(2)} | PnL: ${pnlSign}${t.pnl.toFixed(4)}% | 持仓: ${duration.toFixed(1)}h`);
}

// ============================================================
// 10. 过滤器工作原理说明
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【过滤器工作原理】");
console.log("=".repeat(70));

console.log(`
=== MA288倾斜率过滤 ===

原理: MA288的5期变化率(单位: bps)
- 倾斜率 > 0: 均线上升 (多头趋势)
- 倾斜率 < 0: 均线下降 (空头趋势)
- |倾斜率| < 阈值: 均线走平，趋势不明确

作用:
- 当MA288走平时，价格在均线附近反复穿越
- 这种情况下产生的信号大多是噪音
- 过滤掉这些信号可以大幅减少交易频率

推荐阈值: 3 bps
- 太小(1-2): 过滤不够，仍有噪音
- 太大(8-10): 过滤过度，可能错过好信号

=== 布林带带宽过滤 ===

原理: (上轨-下轨)/中轨 * 100%
- 带宽大: 市场波动大，趋势明确
- 带宽小: 市场波动小，震荡盘整

作用:
- 震荡市中价格在窄幅区间波动
- 双均线信号在震荡市中频繁翻转
- 过滤掉低波动期可以避免被反复止损

推荐阈值: 2.0%
- 太小(1.0-1.5): 过滤不够
- 太大(3.0+): 可能错过低波动期的好机会

=== 双重过滤效果 ===

原始信号: ${baseSignals} 个
过滤后信号: ${optimal.signalCount} 个
信号减少: ${((1-optimal.signalCount/baseSignals)*100).toFixed(1)}%

核心改善:
1. 减少了在MA288附近的反复穿越噪音
2. 避免了震荡市中的假突破信号
3. 只在趋势明确+波动充足时交易
`);

console.log("第二次分析完成！");
