/**
 * ETH 第十五次分析: 30m穿越入场 + 多维矩阵测试
 *
 * 对比:
 *   A. 5m穿越MA288入场 (原策略)
 *   B. 30m穿越MA288入场 (新策略)
 *
 * 30m穿越: 上一根收盘在MA288一侧, 当前收盘穿越到另一侧
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
      if (header[j] === timeCol) row[header[j]] = new Date(vals[j]);
      else if (['open','high','low','close','volume'].includes(header[j])) row[header[j]] = parseFloat(vals[j]);
      else row[header[j]] = vals[j];
    }
    rows.push(row);
  }
  rows.sort((a, b) => a[timeCol] - b[timeCol]);
  return rows;
}

console.log("加载数据...");
const df_5m = loadCSV('../kline_5m_202608070131_ETHUSDT.csv', 'open_time');
const df_30m = loadCSV('../kline_30m_202608070130_ETHUSDT.csv', 'open_time');
console.log(`5m: ${df_5m.length}, 30m: ${df_30m.length}`);

// ============================================================
// 计算30m指标 + 识别穿越信号
// ============================================================
function add30mIndicators(df) {
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
    if (spread[i] !== null && spread[i - 5] !== null) {
      isExpanding[i] = Math.abs(spread[i]) > Math.abs(spread[i - 5]);
    }
  }

  // 30m穿越信号
  const crossSignal = new Array(df.length).fill(null);
  for (let i = 1; i < df.length; i++) {
    if (ma288[i] === null || ma288[i-1] === null) continue;
    const prevAbove = closes[i-1] > ma288[i-1];
    const currAbove = closes[i] > ma288[i];
    if (!prevAbove && currAbove) crossSignal[i] = 'long';
    else if (prevAbove && !currAbove) crossSignal[i] = 'short';
  }

  for (let i = 0; i < df.length; i++) {
    df[i].ma288_30m = ma288[i];
    df[i].ma488_30m = ma488[i];
    df[i].spread_30m = spread[i];
    df[i].isExpanding_30m = isExpanding[i];
    df[i].crossSignal = crossSignal[i];
  }
}

// ============================================================
// 计算5m指标 (仅扩散)
// ============================================================
function add5mIndicators(df) {
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
    if (spread[i] !== null && spread[i - 5] !== null) {
      isExpanding[i] = Math.abs(spread[i]) > Math.abs(spread[i - 5]);
    }
  }
  for (let i = 0; i < df.length; i++) {
    df[i].isExpanding_5m = isExpanding[i];
  }
}

console.log("计算指标...");
add30mIndicators(df_30m);
add5mIndicators(df_5m);

// ============================================================
// 预构建查找表 (二分)
// ============================================================
const valid30m = df_30m.filter(r => r.ma288_30m !== null);
const ts30m = valid30m.map(r => r.open_time.getTime());

function get30mAtFast(timeMs) {
  let lo = 0, hi = ts30m.length - 1, best = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (ts30m[mid] <= timeMs) { best = mid; lo = mid + 1; }
    else hi = mid - 1;
  }
  return best >= 0 ? valid30m[best] : null;
}

const valid5m = df_5m.filter(r => r.isExpanding_5m !== null);
const ts5m = valid5m.map(r => r.open_time.getTime());
function get5mAtFast(timeMs) {
  let lo = 0, hi = ts5m.length - 1, best = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (ts5m[mid] <= timeMs) { best = mid; lo = mid + 1; }
    else hi = mid - 1;
  }
  return best >= 0 ? valid5m[best] : null;
}

// ============================================================
// 策略A: 5m穿越入场
// ============================================================
function run5mCross(config) {
  const {
    useHardStop = true, hardStopPct = 1.5,
    trailingActivate = 3.0, trailingCallback = 1.0,
    use30mExpanding = true, use5mExpanding = true,
  } = config;

  let position = null, entryPrice = 0, hardStopPrice = 0, maxProfitPct = 0;
  let totalPnL = 0, winCount = 0, lossCount = 0, tradeCount = 0;

  for (let i = 1; i < df_5m.length; i++) {
    const row = df_5m[i];
    const o = row.open, h = row.high, l = row.low, c = row.close;
    const data30m = get30mAtFast(row.open_time.getTime());
    if (!data30m) continue;
    const ma288 = data30m.ma288_30m, ma488 = data30m.ma488_30m;
    const trend = ma288 > ma488 ? 'bullish' : (ma288 < ma488 ? 'bearish' : null);
    if (!trend) continue;
    if (use30mExpanding && data30m.isExpanding_30m === false) continue;
    if (use5mExpanding && row.isExpanding_5m === false) continue;

    if (position !== null) {
      const currentPnl = position === 'long' ? (c - entryPrice) / entryPrice * 100 : (entryPrice - c) / entryPrice * 100;
      maxProfitPct = Math.max(maxProfitPct, currentPnl);
      let shouldStop = false, exitPrice = c;
      if (useHardStop) {
        if (position === 'long' && l <= hardStopPrice) { shouldStop = true; exitPrice = hardStopPrice; }
        else if (position === 'short' && h >= hardStopPrice) { shouldStop = true; exitPrice = hardStopPrice; }
      }
      if (!shouldStop) {
        if (position === 'long' && o > ma288 && c < ma288) shouldStop = true;
        else if (position === 'short' && o < ma288 && c > ma288) shouldStop = true;
      }
      if (shouldStop) {
        const pnl = position === 'long' ? (exitPrice - entryPrice) / entryPrice * 100 : (entryPrice - exitPrice) / entryPrice * 100;
        totalPnL += pnl; if (pnl > 0) winCount++; else lossCount++; tradeCount++;
        position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0;
        continue;
      }
      if (maxProfitPct >= trailingActivate && maxProfitPct - currentPnl >= trailingCallback) {
        totalPnL += currentPnl; if (currentPnl > 0) winCount++; else lossCount++; tradeCount++;
        position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0;
        continue;
      }
    }

    let isEntry = false, entryDir = '';
    if (trend === 'bullish' && o < ma288 && c > ma288) { isEntry = true; entryDir = 'long'; }
    else if (trend === 'bearish' && o > ma288 && c < ma288) { isEntry = true; entryDir = 'short'; }

    if (isEntry) {
      if (position !== null && position !== entryDir) {
        const pnl = position === 'long' ? (c - entryPrice) / entryPrice * 100 : (entryPrice - c) / entryPrice * 100;
        totalPnL += pnl; if (pnl > 0) winCount++; else lossCount++; tradeCount++;
        position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0;
      }
      if (position === null) {
        position = entryDir; entryPrice = c; maxProfitPct = 0;
        hardStopPrice = entryDir === 'long' ? entryPrice * (1 - hardStopPct / 100) : entryPrice * (1 + hardStopPct / 100);
      }
    }
  }
  return { tradeCount, winCount, lossCount, winRate: tradeCount > 0 ? winCount / tradeCount * 100 : 0, totalPnL };
}

// ============================================================
// 策略B: 30m穿越入场
// ============================================================
function run30mCross(config) {
  const {
    useHardStop = true, hardStopPct = 1.5,
    trailingActivate = 3.0, trailingCallback = 1.0,
    use30mExpanding = true, use5mExpanding = true,
  } = config;

  let position = null, entryPrice = 0, hardStopPrice = 0, maxProfitPct = 0;
  let totalPnL = 0, winCount = 0, lossCount = 0, tradeCount = 0;
  let lastCrossTs = -1;

  for (let i = 1; i < df_5m.length; i++) {
    const row = df_5m[i];
    const o = row.open, h = row.high, l = row.low, c = row.close;
    const timeMs = row.open_time.getTime();
    const data30m = get30mAtFast(timeMs);
    if (!data30m) continue;
    const ma288 = data30m.ma288_30m, ma488 = data30m.ma488_30m;
    const trend = ma288 > ma488 ? 'bullish' : (ma288 < ma488 ? 'bearish' : null);
    if (!trend) continue;
    if (use30mExpanding && data30m.isExpanding_30m === false) continue;
    if (use5mExpanding) {
      const data5m = get5mAtFast(timeMs);
      if (data5m && !data5m.isExpanding) continue;
    }

    if (position !== null) {
      const currentPnl = position === 'long' ? (c - entryPrice) / entryPrice * 100 : (entryPrice - c) / entryPrice * 100;
      maxProfitPct = Math.max(maxProfitPct, currentPnl);
      let shouldStop = false, exitPrice = c;
      if (useHardStop) {
        if (position === 'long' && l <= hardStopPrice) { shouldStop = true; exitPrice = hardStopPrice; }
        else if (position === 'short' && h >= hardStopPrice) { shouldStop = true; exitPrice = hardStopPrice; }
      }
      if (!shouldStop) {
        if (position === 'long' && o > ma288 && c < ma288) shouldStop = true;
        else if (position === 'short' && o < ma288 && c > ma288) shouldStop = true;
      }
      if (shouldStop) {
        const pnl = position === 'long' ? (exitPrice - entryPrice) / entryPrice * 100 : (entryPrice - exitPrice) / entryPrice * 100;
        totalPnL += pnl; if (pnl > 0) winCount++; else lossCount++; tradeCount++;
        position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0;
        continue;
      }
      if (maxProfitPct >= trailingActivate && maxProfitPct - currentPnl >= trailingCallback) {
        totalPnL += currentPnl; if (currentPnl > 0) winCount++; else lossCount++; tradeCount++;
        position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0;
        continue;
      }
    }

    // 30m穿越入场
    const crossTs = data30m.open_time.getTime();
    if (crossTs === lastCrossTs) continue;
    const signal = data30m.crossSignal;
    if (!signal) continue;

    let isEntry = false, entryDir = '';
    if (signal === 'long' && trend === 'bullish') { isEntry = true; entryDir = 'long'; }
    else if (signal === 'short' && trend === 'bearish') { isEntry = true; entryDir = 'short'; }

    if (isEntry) {
      lastCrossTs = crossTs;
      if (position !== null && position !== entryDir) {
        const pnl = position === 'long' ? (c - entryPrice) / entryPrice * 100 : (entryPrice - c) / entryPrice * 100;
        totalPnL += pnl; if (pnl > 0) winCount++; else lossCount++; tradeCount++;
        position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0;
      }
      if (position === null) {
        position = entryDir; entryPrice = c; maxProfitPct = 0;
        hardStopPrice = entryDir === 'long' ? entryPrice * (1 - hardStopPct / 100) : entryPrice * (1 + hardStopPct / 100);
      }
    }
  }
  return { tradeCount, winCount, lossCount, winRate: tradeCount > 0 ? winCount / tradeCount * 100 : 0, totalPnL };
}

// ============================================================
// 诊断: 2026-08-03 06:30 附近
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【诊断: 2026-08-03 06:00 ~ 07:30 MA288/MA488 + 穿越信号】");
console.log("=".repeat(70));
const targetDate = new Date('2026-08-03T06:30:00+08:00');
for (let i = 0; i < df_30m.length; i++) {
  const t = df_30m[i].open_time;
  if (t >= new Date('2026-08-03T06:00:00+08:00') && t <= new Date('2026-08-03T07:30:00+08:00')) {
    const r = df_30m[i];
    const trend = r.ma288_30m && r.ma488_30m ? (r.ma288_30m > r.ma488_30m ? 'bullish' : 'bearish') : 'N/A';
    console.log(`${t.toISOString().slice(0,19)} | close=${r.close} | MA288=${r.ma288_30m?.toFixed(2)} | MA488=${r.ma488_30m?.toFixed(2)} | spread=${r.spread_30m?.toFixed(2)} | expand=${r.isExpanding_30m} | cross=${r.crossSignal} | trend=${trend}`);
    // 检查穿越: 前一根close vs MA288
    if (i > 0 && r.ma288_30m !== null && df_30m[i-1].ma288_30m !== null) {
      const prev = df_30m[i-1];
      console.log(`  前根: close=${prev.close} vs MA288=${prev.ma288_30m?.toFixed(2)} → ${prev.close > prev.ma288_30m ? '上方' : '下方'}`);
      console.log(`  当根: close=${r.close} vs MA288=${r.ma288_30m?.toFixed(2)} → ${r.close > r.ma288_30m ? '上方' : '下方'}`);
      if (prev.close > prev.ma288_30m && r.close < r.ma288_30m) console.log(`  → 向下穿越! short信号`);
      else if (prev.close < prev.ma288_30m && r.close > r.ma288_30m) console.log(`  → 向上穿越! long信号`);
      else console.log(`  → 未穿越`);
    }
  }
}

// ============================================================
// 追踪: 数据库参数下 2026-08-03 附近交易
// 参数: hard_stop=1.5%, trailing_activate=5%, trailing_callback=1%, 无扩散
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【追踪: 数据库参数下 2026-08-03 附近交易详情】");
console.log("参数: hardStop=1.5%, activate=5%, callback=1%, 无扩散");
console.log("=".repeat(70));

{
  const config = { useHardStop: true, hardStopPct: 1.5, trailingActivate: 5.0, trailingCallback: 1.0, use30mExpanding: false, use5mExpanding: false };
  let position = null, entryPrice = 0, hardStopPrice = 0, maxProfitPct = 0;
  let lastCrossTs = -1;
  let tradeLog = [];

  for (let i = 1; i < df_5m.length; i++) {
    const row = df_5m[i];
    const o = row.open, h = row.high, l = row.low, c = row.close;
    const timeMs = row.open_time.getTime();
    const data30m = get30mAtFast(timeMs);
    if (!data30m) continue;
    const ma288 = data30m.ma288_30m, ma488 = data30m.ma488_30m;
    const trend = ma288 > ma488 ? 'bullish' : (ma288 < ma488 ? 'bearish' : null);
    if (!trend) continue;

    // 检查平仓
    if (position !== null) {
      const currentPnl = position === 'long' ? (c - entryPrice) / entryPrice * 100 : (entryPrice - c) / entryPrice * 100;
      maxProfitPct = Math.max(maxProfitPct, currentPnl);
      let shouldStop = false, exitPrice = c, exitReason = '';

      // 硬止损
      if (config.useHardStop) {
        if (position === 'long' && l <= hardStopPrice) { shouldStop = true; exitPrice = hardStopPrice; exitReason = '硬止损'; }
        else if (position === 'short' && h >= hardStopPrice) { shouldStop = true; exitPrice = hardStopPrice; exitReason = '硬止损'; }
      }
      // MA288穿越止损
      if (!shouldStop) {
        if (position === 'long' && o > ma288 && c < ma288) { shouldStop = true; exitReason = 'MA288穿越'; }
        else if (position === 'short' && o < ma288 && c > ma288) { shouldStop = true; exitReason = 'MA288穿越'; }
      }
      // 移动止盈
      if (!shouldStop && maxProfitPct >= config.trailingActivate && maxProfitPct - currentPnl >= config.trailingCallback) {
        shouldStop = true; exitReason = `移动止盈(峰值${maxProfitPct.toFixed(2)}%, 回撤至${currentPnl.toFixed(2)}%)`;
      }

      if (shouldStop) {
        const pnl = position === 'long' ? (exitPrice - entryPrice) / entryPrice * 100 : (entryPrice - exitPrice) / entryPrice * 100;
        const entryTime = tradeLog[tradeLog.length - 1]?.entryTime;
        const exitTime = row.open_time.toISOString().slice(0, 19).replace('T', ' ');
        tradeLog[tradeLog.length - 1].exitTime = exitTime;
        tradeLog[tradeLog.length - 1].exitPrice = exitPrice;
        tradeLog[tradeLog.length - 1].pnl = pnl;
        tradeLog[tradeLog.length - 1].exitReason = exitReason;
        tradeLog[tradeLog.length - 1].maxProfit = maxProfitPct;
        position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0;
      }
    }

    // 检查入场
    const crossTs = data30m.open_time.getTime();
    if (crossTs === lastCrossTs) continue;
    const signal = data30m.crossSignal;
    if (!signal) continue;

    let isEntry = false, entryDir = '';
    if (signal === 'long' && trend === 'bullish') { isEntry = true; entryDir = 'long'; }
    else if (signal === 'short' && trend === 'bearish') { isEntry = true; entryDir = 'short'; }

    if (isEntry) {
      lastCrossTs = crossTs;
      if (position !== null && position !== entryDir) {
        const pnl = position === 'long' ? (c - entryPrice) / entryPrice * 100 : (entryPrice - c) / entryPrice * 100;
        tradeLog[tradeLog.length - 1].exitTime = row.open_time.toISOString().slice(0, 19).replace('T', ' ');
        tradeLog[tradeLog.length - 1].exitPrice = c;
        tradeLog[tradeLog.length - 1].pnl = pnl;
        tradeLog[tradeLog.length - 1].exitReason = '反向信号平仓';
        position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0;
      }
      if (position === null) {
        position = entryDir; entryPrice = c; maxProfitPct = 0;
        hardStopPrice = entryDir === 'long' ? entryPrice * (1 - 1.5 / 100) : entryPrice * (1 + 1.5 / 100);
        tradeLog.push({
          entryTime: row.open_time.toISOString().slice(0, 19).replace('T', ' '),
          direction: entryDir,
          entryPrice: c,
          ma288: ma288,
          ma488: ma488,
        });
      }
    }
  }

  // 筛选2026-08-03附近的交易
  const aug3Trades = tradeLog.filter(t => {
    const d = t.entryTime || '';
    return d >= '2026-08-02' && d <= '2026-08-04';
  });

  console.log(`\n2026-08-02 ~ 2026-08-04 交易 (${aug3Trades.length}笔):`);
  console.log("方向 | 入场时间           | 入场价  | MA288   | 出场时间           | 出场价  | 盈亏%   | 出场原因");
  console.log("-".repeat(110));
  for (const t of aug3Trades) {
    const dir = t.direction === 'long' ? '做多' : '做空';
    const pnlStr = t.pnl !== undefined ? `${t.pnl >= 0 ? '+' : ''}${t.pnl.toFixed(2)}%` : '持仓中';
    console.log(`${dir} | ${t.entryTime} | ${t.entryPrice.toFixed(2).padStart(7)} | ${t.ma288?.toFixed(2).padStart(7)} | ${t.exitTime || 'N/A'} | ${(t.exitPrice||0).toFixed(2).padStart(7)} | ${pnlStr.padStart(7)} | ${t.exitReason || ''}`);
  }

  // 汇总
  const totalPnL = tradeLog.reduce((s, t) => s + (t.pnl || 0), 0);
  const wins = tradeLog.filter(t => t.pnl > 0).length;
  console.log(`\n全部: ${tradeLog.length}笔, 胜${wins}笔, 总收益: ${totalPnL >= 0 ? '+' : ''}${totalPnL.toFixed(2)}%`);
}

// ============================================================
// 矩阵测试
// ============================================================
const hardStops = [1.0, 1.5, 2.0, 2.5];
const activates = [2, 3, 4, 5, 6];
const callbacks = [1, 2, 3, 4];

// --- 5m穿越 + 扩散全开 ---
console.log("\n" + "=".repeat(70));
console.log("【A: 5m穿越入场, 30m扩散✅ + 5m扩散✅】");
console.log("=".repeat(70));
console.log("hardStop | activate | callback | 交易数 | 胜率   | 总收益   ");
console.log("-".repeat(65));

let best5m = null;
for (const hs of hardStops) {
  for (const act of activates) {
    for (const cb of callbacks) {
      const r = run5mCross({ useHardStop: true, hardStopPct: hs, trailingActivate: act, trailingCallback: cb, use30mExpanding: true, use5mExpanding: true });
      if (!best5m || r.totalPnL > best5m.totalPnL) best5m = { ...r, hs, act, cb };
      if (r.totalPnL > 0) {
        console.log(`  ${hs}%    |    ${String(act).padStart(2)}%    |    ${String(cb).padStart(2)}%    | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ${(r.totalPnL>=0?'+':'')+r.totalPnL.toFixed(2).padStart(7)}%`);
      }
    }
  }
}
console.log(`\n5m穿越最优: hs=${best5m.hs}% act=${best5m.act}% cb=${best5m.cb}% → ${best5m.totalPnL.toFixed(2)}%, ${best5m.tradeCount}笔, 胜率${best5m.winRate.toFixed(1)}%`);

// --- 30m穿越 + 扩散全开 ---
console.log("\n" + "=".repeat(70));
console.log("【B: 30m穿越入场, 30m扩散✅ + 5m扩散✅】");
console.log("=".repeat(70));
console.log("hardStop | activate | callback | 交易数 | 胜率   | 总收益   ");
console.log("-".repeat(65));

let best30m = null;
for (const hs of hardStops) {
  for (const act of activates) {
    for (const cb of callbacks) {
      const r = run30mCross({ useHardStop: true, hardStopPct: hs, trailingActivate: act, trailingCallback: cb, use30mExpanding: true, use5mExpanding: true });
      if (!best30m || r.totalPnL > best30m.totalPnL) best30m = { ...r, hs, act, cb };
      if (r.totalPnL > 0) {
        console.log(`  ${hs}%    |    ${String(act).padStart(2)}%    |    ${String(cb).padStart(2)}%    | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ${(r.totalPnL>=0?'+':'')+r.totalPnL.toFixed(2).padStart(7)}%`);
      }
    }
  }
}
console.log(`\n30m穿越最优: hs=${best30m.hs}% act=${best30m.act}% cb=${best30m.cb}% → ${best30m.totalPnL.toFixed(2)}%, ${best30m.tradeCount}笔, 胜率${best30m.winRate.toFixed(1)}%`);

// --- 30m穿越 + 无扩散 ---
console.log("\n" + "=".repeat(70));
console.log("【C: 30m穿越入场, 30m扩散❌ + 5m扩散❌】");
console.log("=".repeat(70));
console.log("hardStop | activate | callback | 交易数 | 胜率   | 总收益   ");
console.log("-".repeat(65));

let best30mNoExp = null;
for (const hs of hardStops) {
  for (const act of activates) {
    for (const cb of callbacks) {
      const r = run30mCross({ useHardStop: true, hardStopPct: hs, trailingActivate: act, trailingCallback: cb, use30mExpanding: false, use5mExpanding: false });
      if (!best30mNoExp || r.totalPnL > best30mNoExp.totalPnL) best30mNoExp = { ...r, hs, act, cb };
      if (r.totalPnL > 0) {
        console.log(`  ${hs}%    |    ${String(act).padStart(2)}%    |    ${String(cb).padStart(2)}%    | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ${(r.totalPnL>=0?'+':'')+r.totalPnL.toFixed(2).padStart(7)}%`);
      }
    }
  }
}
console.log(`\n30m穿越(无扩散)最优: hs=${best30mNoExp.hs}% act=${best30mNoExp.act}% cb=${best30mNoExp.cb}% → ${best30mNoExp.totalPnL.toFixed(2)}%, ${best30mNoExp.tradeCount}笔, 胜率${best30mNoExp.winRate.toFixed(1)}%`);

// ============================================================
// 汇总
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【汇总对比】");
console.log("=".repeat(70));
console.log(`
策略              | 参数                       | 收益      | 交易数 | 胜率
------------------|----------------------------|-----------|--------|------
5m穿越 + 扩散全开 | hs=${best5m.hs}% act=${best5m.act}% cb=${best5m.cb}% | ${(best5m.totalPnL>=0?'+':'')+best5m.totalPnL.toFixed(2)}%   | ${String(best5m.tradeCount).padStart(6)} | ${best5m.winRate.toFixed(1)}%
30m穿越 + 扩散全开 | hs=${best30m.hs}% act=${best30m.act}% cb=${best30m.cb}% | ${(best30m.totalPnL>=0?'+':'')+best30m.totalPnL.toFixed(2)}%   | ${String(best30m.tradeCount).padStart(6)} | ${best30m.winRate.toFixed(1)}%
30m穿越 + 无扩散   | hs=${best30mNoExp.hs}% act=${best30mNoExp.act}% cb=${best30mNoExp.cb}% | ${(best30mNoExp.totalPnL>=0?'+':'')+best30mNoExp.totalPnL.toFixed(2)}%   | ${String(best30mNoExp.tradeCount).padStart(6)} | ${best30mNoExp.winRate.toFixed(1)}%
`);

console.log("=".repeat(70));
console.log("分析完成！");
console.log("=".repeat(70));
