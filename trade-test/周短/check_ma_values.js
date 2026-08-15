/**
 * 检查 MA288 计算值
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
console.log("检查 MA288 计算值");
console.log("=".repeat(70));

const df_5m = loadCSV('../kline_5m_202608010054_SOLUSDT.csv', 'open_time');
const df_30m = loadCSV('../kline_30m_202608010054_SOLUSDT.csv', 'open_time');

function calcMA(closes, period) {
  if (closes.length < period) return null;
  let sum = 0;
  for (let i = closes.length - period; i < closes.length; i++) {
    sum += closes[i];
  }
  return sum / period;
}

// 找到 2026-07-30 14:30 对应的索引
const targetTime30m = new Date('2026-07-30T14:30:00+08:00').getTime();
const idx30m = df_30m.findIndex(r => r.open_time.getTime() === targetTime30m);

const targetTime5m = new Date('2026-07-30T14:30:00+08:00').getTime();
const idx5m = df_5m.findIndex(r => r.open_time.getTime() === targetTime5m);

console.log("\n--- 30m K线数据 ---");
console.log(`找到 14:30 K线索引: ${idx30m}`);
if (idx30m >= 287) {
  const closes30m = df_30m.slice(idx30m - 287, idx30m + 1).map(r => r.close);
  const ma288_30m = calcMA(closes30m, 288);
  const closes488_30m = df_30m.slice(idx30m - 487, idx30m + 1).map(r => r.close);
  const ma488_30m = calcMA(closes488_30m, 488);

  console.log(`30m MA288 = ${ma288_30m?.toFixed(4)}`);
  console.log(`30m MA488 = ${ma488_30m?.toFixed(4)}`);
  console.log(`30m Close at 14:30 = ${df_30m[idx30m].close}`);

  // 显示最近几根K线的close
  console.log("\n最近5根30m K线:");
  for (let i = Math.max(0, idx30m - 4); i <= idx30m; i++) {
    console.log(`  ${df_30m[i].open_time.toISOString()}: Close=${df_30m[i].close}`);
  }
}

console.log("\n--- 5m K线数据 ---");
console.log(`找到 14:30 K线索引: ${idx5m}`);
if (idx5m >= 287) {
  const closes5m = df_5m.slice(idx5m - 287, idx5m + 1).map(r => r.close);
  const ma288_5m = calcMA(closes5m, 288);
  const closes488_5m = df_5m.slice(idx5m - 487, idx5m + 1).map(r => r.close);
  const ma488_5m = calcMA(closes488_5m, 488);

  console.log(`5m MA288 = ${ma288_5m?.toFixed(4)}`);
  console.log(`5m MA488 = ${ma488_5m?.toFixed(4)}`);
  console.log(`5m Close at 14:30 = ${df_5m[idx5m].close}`);
}

console.log("\n--- 数据库中的值 ---");
console.log(`market_context.fast_ma (MA288) = 73.59684027777783`);
console.log(`market_context.slow_ma (MA488) = 73.65122950819668`);

console.log("\n--- 对比 ---");
if (idx30m >= 287 && idx5m >= 287) {
  const closes30m = df_30m.slice(idx30m - 287, idx30m + 1).map(r => r.close);
  const ma288_30m = calcMA(closes30m, 288);
  const closes5m = df_5m.slice(idx5m - 287, idx5m + 1).map(r => r.close);
  const ma288_5m = calcMA(closes5m, 288);

  console.log(`30m MA288 = ${ma288_30m?.toFixed(4)}`);
  console.log(`5m MA288  = ${ma288_5m?.toFixed(4)}`);
  console.log(`DB MA288  = 73.5968`);
  console.log(`\n结论: DB中的MA288更接近 5m MA288 还是 30m MA288 ?`);

  const diff30m = Math.abs(73.5968 - ma288_30m);
  const diff5m = Math.abs(73.5968 - ma288_5m);
  console.log(`与30m MA288差异: ${diff30m.toFixed(4)}`);
  console.log(`与5m MA288差异: ${diff5m.toFixed(4)}`);
  console.log(`→ DB使用的是: ${diff5m < diff30m ? '5m MA288' : '30m MA288'}`);
}

console.log("\n" + "=".repeat(70));
console.log("问题分析");
console.log("=".repeat(70));
console.log(`
策略配置:
  - entry_timeframe: "30m"
  - fast_ma_period: 288
  - slow_ma_period: 488

如果 entry_timeframe 是 30m，那么:
  - MA288 应该基于 30m K线计算 → 约 74.49
  - 但 DB 中显示 73.5968 → 更接近 5m MA288

可能的原因:
  1. 系统错误地使用了 5m 数据计算 MA288
  2. 策略配置有问题
  3. 代码实现有 bug
`);

console.log("检查完成！");
