/**
 * 验证 SOL 2026-07-30 14:35 做空信号
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
console.log("验证 SOL 2026-07-30 14:35 做空信号");
console.log("=".repeat(70));

const df_5m = loadCSV('../kline_5m_202608010054_SOLUSDT.csv', 'open_time');
const df_30m = loadCSV('../kline_30m_202608010054_SOLUSDT.csv', 'open_time');

console.log(`\n30m K线总数: ${df_30m.length}`);
console.log(`5m K线总数: ${df_5m.length}`);
console.log(`30m 时间范围: ${df_30m[0].open_time.toISOString()} ~ ${df_30m[df_30m.length-1].open_time.toISOString()}`);
console.log(`5m 时间范围: ${df_5m[0].open_time.toISOString()} ~ ${df_5m[df_5m.length-1].open_time.toISOString()}`);

// 信号参数
const signalTime = new Date('2026-07-30T14:35:13+08:00');
const signalPrice = 73.58;
const ma288Val = 73.59684027777783;
const ma488Val = 73.65122950819668;

console.log("\n" + "=".repeat(70));
console.log("信号信息");
console.log("=".repeat(70));
console.log(`信号时间: ${signalTime.toISOString()}`);
console.log(`入场价: ${signalPrice}`);
console.log(`MA288: ${ma288Val}`);
console.log(`MA488: ${ma488Val}`);
console.log(`趋势: MA288(${ma288Val}) < MA488(${ma488Val}) → bearish ✓`);

// 入场条件: trend === 'bearish' && o > ma288 && c < ma288
console.log("\n" + "=".repeat(70));
console.log("入场条件检查 (bearish: O > MA288 && C < MA288)");
console.log("=".repeat(70));

// 检查14:00-14:30的30m K线
console.log("\n--- 30m K线 ---");
const relevant30m = df_30m.filter(r => {
  const t = r.open_time.getTime();
  return t >= new Date('2026-07-30T13:00:00+08:00').getTime() &&
         t <= new Date('2026-07-30T15:30:00+08:00').getTime();
});

for (const r of relevant30m) {
  const o = r.open, c = r.close;
  const cond1 = o > ma288Val;
  const cond2 = c < ma288Val;
  const isEntry = cond1 && cond2;
  const marker = isEntry ? '✅ 入场!' : '';
  console.log(`${r.open_time.toISOString()}: O=${o.toFixed(2)}, C=${c.toFixed(2)} | O>MA288=${cond1}, C<MA288=${cond2} ${marker}`);
}

// 检查5m K线
console.log("\n--- 5m K线 ---");
const relevant5m = df_5m.filter(r => {
  const t = r.open_time.getTime();
  return t >= new Date('2026-07-30T14:00:00+08:00').getTime() &&
         t <= new Date('2026-07-30T15:00:00+08:00').getTime();
});

for (const r of relevant5m) {
  const o = r.open, c = r.close;
  const cond1 = o > ma288Val;
  const cond2 = c < ma288Val;
  const isEntry = cond1 && cond2;
  const marker = isEntry ? '✅ 入场!' : '';
  console.log(`${r.open_time.toISOString()}: O=${o.toFixed(2)}, C=${c.toFixed(2)} | O>MA288=${cond1}, C<MA288=${cond2} ${marker}`);
}

// 30m扩散检查
console.log("\n" + "=".repeat(70));
console.log("30m扩散检查");
console.log("=".repeat(70));

// 计算扩散指标
function addSpread(df) {
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
  const ma288 = calcMA(288);
  const ma488 = calcMA(488);
  const spread = new Array(df.length).fill(null);
  const isExpanding = new Array(df.length).fill(null);
  for (let i = 0; i < df.length; i++) {
    if (ma288[i] !== null && ma488[i] !== null) spread[i] = ma288[i] - ma488[i];
  }
  for (let i = 5; i < df.length; i++) {
    if (spread[i] !== null && spread[i-5] !== null) {
      isExpanding[i] = Math.abs(spread[i]) > Math.abs(spread[i-5]);
    }
  }
  for (let i = 0; i < df.length; i++) {
    df[i].ma288 = ma288[i];
    df[i].ma488 = ma488[i];
    df[i].spread = spread[i];
    df[i].isExpanding = isExpanding[i];
  }
  return df;
}

addSpread(df_30m);

// 找到信号时间点的30m数据
const signalTimeMs = signalTime.getTime();
let bestIdx = -1;
let bestDiff = Infinity;
for (let i = 0; i < df_30m.length; i++) {
  const diff = signalTimeMs - df_30m[i].open_time.getTime();
  if (diff >= 0 && diff < bestDiff) {
    bestDiff = diff;
    bestIdx = i;
  }
}

if (bestIdx >= 0) {
  const row = df_30m[bestIdx];
  console.log(`\n信号时间最近的30m K线: ${row.open_time.toISOString()}`);
  console.log(`  MA288=${row.ma288?.toFixed(4)}, MA488=${row.ma488?.toFixed(4)}`);
  console.log(`  Spread=${row.spread?.toFixed(6)}`);
  console.log(`  30m扩散: ${row.isExpanding ? '✅ 正在扩散' : '❌ 未扩散'}`);

  // 检查前一根
  if (bestIdx > 0) {
    const prev = df_30m[bestIdx - 1];
    console.log(`\n前一根30m K线: ${prev.open_time.toISOString()}`);
    console.log(`  Spread=${prev.spread?.toFixed(6)}`);
    console.log(`  30m扩散: ${prev.isExpanding ? '✅ 正在扩散' : '❌ 未扩散'}`);
  }
}

// 结论
console.log("\n" + "=".repeat(70));
console.log("结论");
console.log("=".repeat(70));

// 检查14:00 K线
const candle1400 = df_30m.find(r => r.open_time.getTime() === new Date('2026-07-30T14:00:00+08:00').getTime());
const candle1430 = df_30m.find(r => r.open_time.getTime() === new Date('2026-07-30T14:30:00+08:00').getTime());

console.log(`\n1. 趋势方向: ✅ MA288(${ma288Val}) < MA488(${ma488Val}) → bearish`);

if (candle1400) {
  const ok = candle1400.open > ma288Val && candle1400.close < ma288Val;
  console.log(`2. 14:00 30m K线入场: O=${candle1400.open.toFixed(2)}>MA288=${candle1400.open > ma288Val}, C=${candle1400.close.toFixed(2)}<MA288=${candle1400.close < ma288Val} → ${ok ? '✅ 满足' : '❌ 不满足'}`);
}

if (candle1430) {
  const ok = candle1430.open > ma288Val && candle1430.close < ma288Val;
  console.log(`3. 14:30 30m K线入场: O=${candle1430.open.toFixed(2)}>MA288=${candle1430.open > ma288Val}, C=${candle1430.close.toFixed(2)}<MA288=${candle1430.close < ma288Val} → ${ok ? '✅ 满足' : '❌ 不满足'}`);
}

if (bestIdx >= 0) {
  const row = df_30m[bestIdx];
  console.log(`4. 30m扩散: ${row.isExpanding ? '✅' : '❌'}`);
}

console.log("\n5m入场检查:");
let any5mEntry = false;
for (const r of relevant5m) {
  if (r.open > ma288Val && r.close < ma288Val) {
    console.log(`   ✅ ${r.open_time.toISOString()}: O=${r.open.toFixed(2)}, C=${r.close.toFixed(2)}`);
    any5mEntry = true;
  }
}
if (!any5mEntry) {
  console.log(`   ❌ 没有5m K线满足入场条件`);
}

console.log("\n" + "=".repeat(70));
console.log("最终判断");
console.log("=".repeat(70));

const trendOK = ma288Val < ma488Val;
const candleOK = candle1400 && (candle1400.open > ma288Val && candle1400.close < ma288Val);
const expanding30 = bestIdx >= 0 && df_30m[bestIdx].isExpanding;

console.log(`趋势 bearish: ${trendOK ? '✅' : '❌'}`);
console.log(`30m K线穿越MA288: ${candleOK ? '✅' : '❌'}`);
console.log(`30m扩散: ${expanding30 ? '✅' : '❌'}`);

if (trendOK && candleOK && expanding30) {
  console.log("\n✅ 信号正确! 做空信号符合策略规则。");
} else {
  console.log("\n❌ 信号存在问题!");
  if (!trendOK) console.log("   - 趋势方向不对");
  if (!candleOK) console.log("   - 30m K线未穿越MA288");
  if (!expanding30) console.log("   - 30m未扩散");
}

console.log("\n验证完成！");
