/**
 * 多周期双均线+布林带量化策略可行性分析
 * - 30m: MA(288)/MA(488) 判断主趋势方向
 * - 5m: MA(288)/MA(488) 辅助判断，防踏空
 * - 布林带(100,2): 止盈减仓参考
 * - MA(48): 短周期信号辅助
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
console.log("加载数据...");

const df_1m = loadCSV('kline_1m_202607222211.csv', 'timestamp');
const df_5m = loadCSV('kline_5m_202607222208.csv', 'open_time');
const df_30m = loadCSV('kline_30m_202607222207.csv', 'open_time');
const df_4h = loadCSV('kline_4h_202607222213.csv', 'open_time');

for (const [name, df, tc] of [['1m', df_1m, 'timestamp'], ['5m', df_5m, 'open_time'],
                                ['30m', df_30m, 'open_time'], ['4h', df_4h, 'open_time']]) {
  console.log(`  ${name}: ${df.length} bars, ${df[0][tc].toISOString()} ~ ${df[df.length-1][tc].toISOString()}`);
}

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
  for (let i = 99; i < df.length; i++) {
    let sumSq = 0;
    for (let j = i - 99; j <= i; j++) {
      sumSq += (closes[j] - bbMid[i]) ** 2;
    }
    const std = Math.sqrt(sumSq / 100);
    bbUpper[i] = bbMid[i] + 2 * std;
    bbLower[i] = bbMid[i] - 2 * std;
  }

  // MA288 斜率 (5期变化率, bps)
  const ma288Slope = new Array(df.length).fill(null);
  for (let i = 5; i < df.length; i++) {
    if (ma288[i] !== null && ma288[i-5] !== null && ma288[i-5] !== 0) {
      ma288Slope[i] = (ma288[i] - ma288[i-5]) / ma288[i-5] * 10000;
    }
  }

  for (let i = 0; i < df.length; i++) {
    df[i].ma48 = ma48[i];
    df[i].ma288 = ma288[i];
    df[i].ma488 = ma488[i];
    df[i].bbMid = bbMid[i];
    df[i].bbUpper = bbUpper[i];
    df[i].bbLower = bbLower[i];
    df[i].ma288Slope = ma288Slope[i];
  }
  return df;
}

console.log("\n" + "=".repeat(70));
console.log("计算技术指标...");

addIndicators(df_30m);
addIndicators(df_5m);
addIndicators(df_4h);

// ============================================================
// 3. 30m 主趋势分析
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【30分钟K线】主趋势分析");
console.log("=".repeat(70));

const df_30m_valid = df_30m.filter(r => r.ma288 !== null && r.ma488 !== null);
console.log(`有效数据: ${df_30m_valid.length} bars (需要至少488根K线预热)`);

if (df_30m_valid.length > 0) {
  const latest = df_30m_valid[df_30m_valid.length - 1];
  console.log(`\n最新30m K线: ${latest.open_time.toISOString()}`);
  console.log(`  Open: ${latest.open.toFixed(2)}  Close: ${latest.close.toFixed(2)}`);
  console.log(`  MA48:  ${latest.ma48.toFixed(2)}`);
  console.log(`  MA288: ${latest.ma288.toFixed(2)}`);
  console.log(`  MA488: ${latest.ma488.toFixed(2)}`);
  console.log(`  BB Mid:   ${latest.bbMid.toFixed(2)}`);
  console.log(`  BB Upper: ${latest.bbUpper.toFixed(2)}`);
  console.log(`  BB Lower: ${latest.bbLower.toFixed(2)}`);
  console.log(`  MA288 倾斜率: ${latest.ma288Slope ? latest.ma288Slope.toFixed(2) + ' bps' : 'N/A'}`);

  const maDiff = latest.ma288 - latest.ma488;
  const maDiffPct = maDiff / latest.ma488 * 100;
  console.log(`\n  MA288 - MA488 = ${maDiff.toFixed(2)} (${maDiffPct.toFixed(3)}%)`);

  if (maDiff > 0) {
    console.log(`  → MA288在MA488之上 → 多头趋势 (以涨为主)`);
  } else {
    console.log(`  → MA288在MA488之下 → 空头趋势 (以跌为主)`);
  }

  // 均线扩散/收敛判断
  if (df_30m_valid.length >= 20) {
    const recent20 = df_30m_valid.slice(-20);
    const diffStart = recent20[0].ma288 - recent20[0].ma488;
    const diffEnd = recent20[19].ma288 - recent20[19].ma488;
    const diffChange = diffEnd - diffStart;
    const diffChangePct = diffStart !== 0 ? diffChange / Math.abs(diffStart) * 100 : 0;
    console.log(`\n  近20根K线 MA288-MA488 差值变化: ${diffChange.toFixed(2)} (${diffChangePct.toFixed(1)}%)`);
    if (Math.abs(diffChangePct) > 5) {
      console.log(diffChange > 0 ? `  → 均线扩散中 (多头加速)` : `  → 均线扩散中 (空头加速)`);
    } else if (Math.abs(diffChangePct) < 2) {
      console.log(`  → 均线收敛/横盘`);
    } else {
      console.log(`  → 均线轻微变化`);
    }
  }
}

// ============================================================
// 4. 30m 信号回测 (策略1+2)
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【30分钟K线】策略1+2 信号回测");
console.log("=".repeat(70));

const signals_30m = [];
let position = null;

for (let i = 1; i < df_30m_valid.length; i++) {
  const row = df_30m_valid[i];
  const ma288 = row.ma288;
  const ma488 = row.ma488;
  const o = row.open, c = row.close;

  let trend;
  if (ma288 < ma488) trend = 'bearish';
  else if (ma288 > ma488) trend = 'bullish';
  else continue;

  if (trend === 'bearish') {
    if (o > ma288 && c < ma288) {
      if (position !== 'short') {
        signals_30m.push({
          time: row.open_time, type: 'SHORT', price: c,
          reason: '空头趋势反弹受阻MA288', trend
        });
        position = 'short';
      }
    } else if (o < ma288 && c > ma288) {
      if (position === 'short') {
        signals_30m.push({
          time: row.open_time, type: 'COVER', price: c,
          reason: '空头止损：收盘站上MA288', trend
        });
        position = null;
      }
    }
  } else if (trend === 'bullish') {
    if (o < ma288 && c > ma288) {
      if (position !== 'long') {
        signals_30m.push({
          time: row.open_time, type: 'LONG', price: c,
          reason: '多头趋势回落获撑MA288', trend
        });
        position = 'long';
      }
    } else if (o > ma288 && c < ma288) {
      if (position === 'long') {
        signals_30m.push({
          time: row.open_time, type: 'STOP', price: c,
          reason: '多头止损：收盘跌破MA288', trend
        });
        position = null;
      }
    }
  }
}

console.log(`共产生 ${signals_30m.length} 个信号`);
console.log(`\n最近20个信号:`);
for (const s of signals_30m.slice(-20)) {
  console.log(`  ${s.time.toISOString()} | ${s.type.padEnd(6)} @ ${s.price.toFixed(2)} | ${s.reason}`);
}

// ============================================================
// 5. 5m 双均线分析 (策略3: 防踏空)
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【5分钟K线】MA288/MA488 趋势分析 (防踏空)");
console.log("=".repeat(70));

const df_5m_valid = df_5m.filter(r => r.ma288 !== null && r.ma488 !== null);
if (df_5m_valid.length > 0) {
  const latest5 = df_5m_valid[df_5m_valid.length - 1];
  console.log(`最新5m K线: ${latest5.open_time.toISOString()}`);
  console.log(`  Open: ${latest5.open.toFixed(2)}  Close: ${latest5.close.toFixed(2)}`);
  console.log(`  MA48:  ${latest5.ma48.toFixed(2)}`);
  console.log(`  MA288: ${latest5.ma288.toFixed(2)}`);
  console.log(`  MA488: ${latest5.ma488.toFixed(2)}`);
  console.log(`  MA288 倾斜率: ${latest5.ma288Slope ? latest5.ma288Slope.toFixed(2) + ' bps' : 'N/A'}`);

  const ma5Diff = latest5.ma288 - latest5.ma488;
  const ma5DiffPct = ma5Diff / latest5.ma488 * 100;
  console.log(`  MA288 - MA488 = ${ma5Diff.toFixed(2)} (${ma5DiffPct.toFixed(3)}%)`);
  console.log(ma5Diff > 0 ? `  → 5m级别: 多头趋势` : `  → 5m级别: 空头趋势`);

  if (df_30m_valid.length > 0) {
    const trend30m = df_30m_valid[df_30m_valid.length - 1].ma288 > df_30m_valid[df_30m_valid.length - 1].ma488 ? 'bullish' : 'bearish';
    const trend5m = ma5Diff > 0 ? 'bullish' : 'bearish';
    console.log(`\n  30m趋势: ${trend30m}  |  5m趋势: ${trend5m}`);
    if (trend30m === trend5m) {
      console.log(`  → 多周期一致，信号可信度高`);
    } else {
      console.log(`  → ⚠ 多周期背离！30m和5m方向不同，需谨慎`);
    }
  }
}

// 5m信号统计
const signals_5m = [];
for (let i = 1; i < df_5m_valid.length; i++) {
  const row = df_5m_valid[i];
  const o = row.open, c = row.close;
  const ma288 = row.ma288, ma488 = row.ma488;

  if (ma288 < ma488) {
    if (o > ma288 && c < ma288) signals_5m.push({ time: row.open_time, type: 'SHORT', price: c });
    else if (o < ma288 && c > ma288) signals_5m.push({ time: row.open_time, type: 'LONG', price: c });
  } else if (ma288 > ma488) {
    if (o < ma288 && c > ma288) signals_5m.push({ time: row.open_time, type: 'LONG', price: c });
    else if (o > ma288 && c < ma288) signals_5m.push({ time: row.open_time, type: 'SHORT', price: c });
  }
}
console.log(`\n5m级别总信号数: ${signals_5m.length}`);

// ============================================================
// 6. 信号摩擦分析 (策略5)
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【5分钟K线】信号摩擦分析");
console.log("=".repeat(70));

if (signals_5m.length > 1) {
  const intervals = [];
  for (let i = 1; i < signals_5m.length; i++) {
    intervals.push((signals_5m[i].time - signals_5m[i-1].time) / 60000);
  }
  const short30 = intervals.filter(x => x < 30);
  const short15 = intervals.filter(x => x < 15);
  const avg = intervals.reduce((a,b) => a+b, 0) / intervals.length;
  const sorted = [...intervals].sort((a,b) => a-b);
  const median = sorted[Math.floor(sorted.length / 2)];

  console.log(`5m信号间隔统计:`);
  console.log(`  总信号数: ${signals_5m.length}`);
  console.log(`  平均间隔: ${avg.toFixed(1)} 分钟`);
  console.log(`  中位间隔: ${median.toFixed(1)} 分钟`);
  console.log(`  <30min的密集信号: ${short30.length} 次 (${(short30.length/intervals.length*100).toFixed(1)}%)`);
  console.log(`  <15min的密集信号: ${short15.length} 次 (${(short15.length/intervals.length*100).toFixed(1)}%)`);

  let frictionCount = 0;
  for (let i = 2; i < signals_5m.length; i++) {
    if (signals_5m[i].type !== signals_5m[i-1].type && signals_5m[i].type !== signals_5m[i-2].type) {
      frictionCount++;
    }
  }
  console.log(`\n  反复穿越(3次信号内方向反转): ${frictionCount} 次`);
  console.log(`  → 这些是典型的「信号摩擦」，会频繁触发开平仓`);
}

// ============================================================
// 7. 布林带止盈分析 (策略4)
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【30分钟K线】布林带止盈分析");
console.log("=".repeat(70));

const df_30m_bb = df_30m.filter(r => r.bbUpper !== null && r.bbLower !== null && r.bbMid !== null);
if (df_30m_bb.length > 0) {
  const latest_bb = df_30m_bb[df_30m_bb.length - 1];
  const price = latest_bb.close;
  const bbMid = latest_bb.bbMid;
  const bbUpper = latest_bb.bbUpper;
  const bbLower = latest_bb.bbLower;
  const bbWidth = (bbUpper - bbLower) / bbMid * 100;

  console.log(`最新布林带状态:`);
  console.log(`  价格: ${price.toFixed(2)}`);
  console.log(`  中轨: ${bbMid.toFixed(2)}`);
  console.log(`  上轨: ${bbUpper.toFixed(2)}`);
  console.log(`  下轨: ${bbLower.toFixed(2)}`);
  console.log(`  带宽: ${bbWidth.toFixed(2)}%`);

  const bbPos = (price - bbLower) / (bbUpper - bbLower) * 100;
  console.log(`  价格位置: ${bbPos.toFixed(1)}% (0=下轨, 100=上轨)`);

  if (bbPos > 80) console.log(`  → 价格接近上轨，考虑止盈减仓`);
  else if (bbPos < 20) console.log(`  → 价格接近下轨，空头可考虑止盈`);
  else console.log(`  → 价格在布林带中部，持仓观望`);

  let touchUpper = 0, touchLower = 0;
  for (const r of df_30m_bb) {
    if (r.high >= r.bbUpper) touchUpper++;
    if (r.low <= r.bbLower) touchLower++;
  }
  console.log(`\n  历史触及上轨次数: ${touchUpper}`);
  console.log(`  历史触及下轨次数: ${touchLower}`);
}

// ============================================================
// 8. 30m vs 5m 信号冲突分析 (策略3)
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【多周期信号冲突分析】策略3可行性");
console.log("=".repeat(70));

let conflictCount = 0, agreeCount = 0;
const recent30Signals = signals_30m.slice(-100);
for (const s30 of recent30Signals) {
  const t = s30.time;
  for (let j = signals_5m.length - 1; j >= 0; j--) {
    if (signals_5m[j].time <= t) {
      if ((s30.type === 'SHORT' || s30.type === 'COVER') && signals_5m[j].type === 'SHORT') agreeCount++;
      else if ((s30.type === 'LONG' || s30.type === 'STOP') && signals_5m[j].type === 'LONG') agreeCount++;
      else conflictCount++;
      break;
    }
  }
}
console.log(`最近${recent30Signals.length}个30m信号中:`);
console.log(`  30m/5m信号一致: ${agreeCount}`);
console.log(`  30m/5m信号冲突: ${conflictCount}`);
const total = agreeCount + conflictCount;
if (total > 0) console.log(`  一致率: ${(agreeCount/total*100).toFixed(1)}%`);

// ============================================================
// 9. 4h 超级趋势确认
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【4小时K线】大趋势确认");
console.log("=".repeat(70));

const df_4h_valid = df_4h.filter(r => r.ma288 !== null && r.ma488 !== null);
if (df_4h_valid.length > 0) {
  const latest4h = df_4h_valid[df_4h_valid.length - 1];
  console.log(`最新4h K线: ${latest4h.open_time.toISOString()}`);
  console.log(`  MA48:  ${latest4h.ma48.toFixed(2)}`);
  console.log(`  MA288: ${latest4h.ma288.toFixed(2)}`);
  console.log(`  MA488: ${latest4h.ma488.toFixed(2)}`);

  const ma4hDiff = latest4h.ma288 - latest4h.ma488;
  const trend4h = ma4hDiff > 0 ? 'bullish' : 'bearish';
  console.log(`  4h趋势: ${trend4h}`);
  console.log(`  MA288-MA488: ${ma4hDiff.toFixed(2)} (${(ma4hDiff/latest4h.ma488*100).toFixed(3)}%)`);
}

// ============================================================
// 10. 策略回测 - 模拟盈亏
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【策略回测】模拟盈亏统计");
console.log("=".repeat(70));

let totalPnL = 0;
let winCount = 0, lossCount = 0;
let maxWin = 0, maxLoss = 0;
let trades = [];
let entryPrice = 0, entryTime = null, entryType = null;

for (const s of signals_30m) {
  if (s.type === 'SHORT' && entryType !== 'SHORT') {
    entryPrice = s.price;
    entryTime = s.time;
    entryType = 'SHORT';
  } else if (s.type === 'LONG' && entryType !== 'LONG') {
    entryPrice = s.price;
    entryTime = s.time;
    entryType = 'LONG';
  } else if ((s.type === 'COVER' && entryType === 'SHORT') || (s.type === 'STOP' && entryType === 'LONG')) {
    const pnl = entryType === 'SHORT' ? (entryPrice - s.price) / entryPrice * 100 : (s.price - entryPrice) / entryPrice * 100;
    totalPnL += pnl;
    if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
    else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
    trades.push({
      entry: entryPrice, exit: s.price, pnl, type: entryType,
      entryTime, exitTime: s.time
    });
    entryType = null;
  }
}

if (trades.length > 0) {
  console.log(`\n交易统计:`);
  console.log(`  总交易次数: ${trades.length}`);
  console.log(`  盈利次数: ${winCount}  亏损次数: ${lossCount}`);
  console.log(`  胜率: ${(winCount/trades.length*100).toFixed(1)}%`);
  console.log(`  总收益: ${totalPnL.toFixed(4)}%`);
  console.log(`  平均收益: ${(totalPnL/trades.length).toFixed(4)}%`);
  console.log(`  最大单笔盈利: ${maxWin.toFixed(4)}%`);
  console.log(`  最大单笔亏损: ${maxLoss.toFixed(4)}%`);
  console.log(`  盈亏比: ${maxWin !== 0 && maxLoss !== 0 ? (maxWin/Math.abs(maxLoss)).toFixed(2) : 'N/A'}`);

  console.log(`\n最近10笔交易:`);
  for (const t of trades.slice(-10)) {
    const duration = (t.exitTime - t.entryTime) / 3600000;
    console.log(`  ${t.type} ${t.entry.toFixed(2)} → ${t.exit.toFixed(2)} | PnL: ${t.pnl.toFixed(4)}% | 持仓: ${duration.toFixed(1)}h`);
  }
}

// ============================================================
// 11. 综合可行性评估
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【综合可行性评估】");
console.log("=".repeat(70));

const recent30m = df_30m.slice(-48);
const priceHigh = Math.max(...recent30m.map(r => r.high));
const priceLow = Math.min(...recent30m.map(r => r.low));
const priceRange = priceHigh - priceLow;
const priceVolatility = priceRange / (recent30m.reduce((a,b) => a+b.close, 0) / recent30m.length) * 100;

console.log(`\n近24小时行情:`);
console.log(`  最高: ${priceHigh.toFixed(2)}`);
console.log(`  最低: ${priceLow.toFixed(2)}`);
console.log(`  波动: ${priceRange.toFixed(2)} (${priceVolatility.toFixed(2)}%)`);

const latest30 = df_30m_valid[df_30m_valid.length - 1];
const trendDir = latest30.ma288 > latest30.ma488 ? '多头' : '空头';

console.log(`
=== 策略可行性总结 ===

1. 【30m双均线趋势判断】✅ 可行
   - MA(288) = 144小时 ≈ 6天, MA(488) = 244小时 ≈ 10天
   - 这两个周期适合判断1-2周的中期趋势
   - 当前30m趋势: ${trendDir}

2. 【信号触发逻辑】⚠ 需要优化
   - "开盘价高于MA288且收盘价低于MA288"这个条件在趋势中会频繁出现
   - 建议增加过滤条件:
     a) MA288斜率过滤: 只在MA288明显倾斜时开仓
     b) 成交量确认: 信号K线成交量需大于均值
     c) 距离过滤: 价格需距离MA288一定比例才触发

3. 【5m防踏空】⚠ 需要谨慎
   - 5m MA(288) = 24小时, MA(488) ≈ 40小时
   - 5m信号会非常频繁，容易产生大量噪音
   - 建议: 5m只用于确认30m信号，不单独开仓

4. 【布林带止盈】✅ 可行
   - 布林带(100,2)适合判断价格是否过度延伸
   - 价格触及上/下轨时减仓是合理的止盈策略
   - 可结合MA(48)作为动态止盈线

5. 【信号摩擦问题】⚠ 核心痛点
   - 5m级别的MA288附近反复穿越会产生大量假信号
   - 解决方案:
     a) MA288倾斜率过滤: 倾斜率<阈值时不交易
     b) 布林带带宽过滤: 带宽收窄时不交易(震荡市)
     c) ATR过滤: 波动率过低时不交易
     d) 价格需连续N根K线站稳MA288才确认信号
`);

console.log("分析完成！");
