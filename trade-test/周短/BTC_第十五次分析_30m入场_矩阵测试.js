/**
 * BTC 第十五次分析: 30m穿越入场 + 多维矩阵测试
 *
 * 对比:
 *   A. 5m穿越MA288入场 (原策略)
 *   B. 30m穿越MA288入场 (新策略)
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
const df_5m = loadCSV('../kline_5m_202608070217_BTCUSDT.csv', 'open_time');
const df_30m = loadCSV('../kline_30m_202608070216_BTCUSDT.csv', 'open_time');
console.log(`5m: ${df_5m.length}, 30m: ${df_30m.length}`);

// ============================================================
// 计算30m指标 + 30m穿越信号
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
  const ma288Slope = new Array(df.length).fill(null);
  const bbMid = calcMA(100);
  const bbWidth = new Array(df.length).fill(null);
  const volRatio = new Array(df.length).fill(null);
  const volumes = df.map(r => r.volume);

  for (let i = 0; i < df.length; i++) {
    if (ma288[i] !== null && ma488[i] !== null) spread[i] = ma288[i] - ma488[i];
  }
  for (let i = 5; i < df.length; i++) {
    if (spread[i] !== null && spread[i - 5] !== null) {
      isExpanding[i] = Math.abs(spread[i]) > Math.abs(spread[i - 5]);
    }
    if (ma288[i] !== null && ma288[i-5] !== null && ma288[i-5] !== 0) {
      ma288Slope[i] = (ma288[i] - ma288[i-5]) / ma288[i-5] * 10000;
    }
  }
  for (let i = 99; i < df.length; i++) {
    let sumSq = 0;
    for (let j = i - 99; j <= i; j++) sumSq += (closes[j] - bbMid[i]) ** 2;
    const std = Math.sqrt(sumSq / 100);
    const upper = bbMid[i] + 2 * std;
    const lower = bbMid[i] - 2 * std;
    bbWidth[i] = (upper - lower) / bbMid[i] * 100;
  }
  for (let i = 19; i < df.length; i++) {
    let sum = 0;
    for (let j = i - 19; j <= i; j++) sum += volumes[j];
    const volMA = sum / 20;
    if (volMA > 0) volRatio[i] = volumes[i] / volMA;
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
    df[i].ma288 = ma288[i];
    df[i].ma488 = ma488[i];
    df[i].spread = spread[i];
    df[i].isExpanding = isExpanding[i];
    df[i].ma288Slope = ma288Slope[i];
    df[i].bbWidth = bbWidth[i];
    df[i].volRatio = volRatio[i];
    df[i].crossSignal = crossSignal[i];
  }
}

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
const valid30m = df_30m.filter(r => r.ma288 !== null);
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
    useHardStop = true, hardStopPct = 1.0,
    trailingActivate = 3.0, trailingCallback = 1.0,
    use30mExpanding = false, use5mExpanding = false,
    slopeThreshold = 0, bbwThreshold = 0, volThreshold = 0,
  } = config;

  let position = null, entryPrice = 0, hardStopPrice = 0, maxProfitPct = 0;
  let totalPnL = 0, winCount = 0, lossCount = 0, tradeCount = 0;

  for (let i = 1; i < df_5m.length; i++) {
    const row = df_5m[i];
    const o = row.open, h = row.high, l = row.low, c = row.close;
    const data30m = get30mAtFast(row.open_time.getTime());
    if (!data30m) continue;
    const ma288 = data30m.ma288, ma488 = data30m.ma488;
    const trend = ma288 > ma488 ? 'bullish' : (ma288 < ma488 ? 'bearish' : null);
    if (!trend) continue;
    if (slopeThreshold > 0 && data30m.ma288Slope !== null && Math.abs(data30m.ma288Slope) < slopeThreshold) continue;
    if (bbwThreshold > 0 && data30m.bbWidth !== null && data30m.bbWidth < bbwThreshold) continue;
    if (volThreshold > 0 && data30m.volRatio !== null && data30m.volRatio < volThreshold) continue;
    if (use30mExpanding && data30m.isExpanding === false) continue;
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
    useHardStop = true, hardStopPct = 1.0,
    trailingActivate = 3.0, trailingCallback = 1.0,
    use30mExpanding = false, use5mExpanding = false,
    slopeThreshold = 0, bbwThreshold = 0, volThreshold = 0,
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
    const ma288 = data30m.ma288, ma488 = data30m.ma488;
    const trend = ma288 > ma488 ? 'bullish' : (ma288 < ma488 ? 'bearish' : null);
    if (!trend) continue;
    if (slopeThreshold > 0 && data30m.ma288Slope !== null && Math.abs(data30m.ma288Slope) < slopeThreshold) continue;
    if (bbwThreshold > 0 && data30m.bbWidth !== null && data30m.bbWidth < bbwThreshold) continue;
    if (volThreshold > 0 && data30m.volRatio !== null && data30m.volRatio < volThreshold) continue;
    if (use30mExpanding && data30m.isExpanding === false) continue;
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
// 追踪: 30m穿越无过滤 2026年7-8月交易详情
// 参数: hardStop=1.0%, activate=5%, callback=1%
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【追踪: 30m穿越无过滤 2026年7-8月交易详情】");
console.log("参数: hardStop=1.0%, activate=5%, callback=1%");
console.log("=".repeat(70));

{
  const config = { useHardStop: true, hardStopPct: 1.0, trailingActivate: 5.0, trailingCallback: 1.0, use30mExpanding: false, use5mExpanding: false, slopeThreshold: 0, bbwThreshold: 0, volThreshold: 0 };
  let position = null, entryPrice = 0, hardStopPrice = 0, maxProfitPct = 0;
  let lastCrossTs = -1;
  let tradeLog = [];

  for (let i = 1; i < df_5m.length; i++) {
    const row = df_5m[i];
    const o = row.open, h = row.high, l = row.low, c = row.close;
    const timeMs = row.open_time.getTime();
    const data30m = get30mAtFast(timeMs);
    if (!data30m) continue;
    const ma288 = data30m.ma288, ma488 = data30m.ma488;
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
        const exitTime = row.open_time.toISOString().slice(0, 19).replace('T', ' ');
        tradeLog[tradeLog.length - 1].exitTime = exitTime;
        tradeLog[tradeLog.length - 1].exitPrice = exitPrice;
        tradeLog[tradeLog.length - 1].pnl = pnl;
        tradeLog[tradeLog.length - 1].exitReason = exitReason;
        tradeLog[tradeLog.length - 1].maxProfit = maxProfitPct;
        tradeLog[tradeLog.length - 1].bars = i - tradeLog[tradeLog.length - 1].entryBarIdx;
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
        hardStopPrice = entryDir === 'long' ? entryPrice * (1 - 1.0 / 100) : entryPrice * (1 + 1.0 / 100);
        tradeLog.push({
          entryTime: row.open_time.toISOString().slice(0, 19).replace('T', ' '),
          direction: entryDir,
          entryPrice: c,
          ma288: ma288,
          ma488: ma488,
          entryBarIdx: i,
        });
      }
    }
  }

  // 筛选2026年7-8月的交易
  const julAugTrades = tradeLog.filter(t => {
    const d = t.entryTime || '';
    return d >= '2026-07-01' && d < '2026-09-01';
  });

  console.log(`\n2026年7-8月交易 (${julAugTrades.length}笔):`);
  console.log(" # | 方向 | 入场时间           | 入场价      | MA288     | 出场时间           | 出场价      | 盈亏%    | 出场原因");
  console.log("-".repeat(130));
  let julPnL = 0, augPnL = 0, julCount = 0, augCount = 0;
  for (let idx = 0; idx < julAugTrades.length; idx++) {
    const t = julAugTrades[idx];
    const dir = t.direction === 'long' ? '做多' : '做空';
    const pnlStr = t.pnl !== undefined ? `${t.pnl >= 0 ? '+' : ''}${t.pnl.toFixed(2)}%` : '持仓中';
    const month = t.entryTime.slice(5, 7);
    if (month === '07') { julPnL += (t.pnl || 0); julCount++; }
    if (month === '08') { augPnL += (t.pnl || 0); augCount++; }
    console.log(`${String(idx+1).padStart(2)} | ${dir} | ${t.entryTime} | ${t.entryPrice.toFixed(2).padStart(11)} | ${t.ma288?.toFixed(2).padStart(9)} | ${t.exitTime || 'N/A'} | ${(t.exitPrice||0).toFixed(2).padStart(11)} | ${pnlStr.padStart(8)} | ${t.exitReason || ''}`);
  }

  console.log(`\n--- 月度汇总 ---`);
  console.log(`7月: ${julCount}笔, 收益 ${julPnL >= 0 ? '+' : ''}${julPnL.toFixed(2)}%`);
  console.log(`8月: ${augCount}笔, 收益 ${augPnL >= 0 ? '+' : ''}${augPnL.toFixed(2)}%`);
  console.log(`合计: ${julAugTrades.length}笔, 收益 ${(julPnL+augPnL) >= 0 ? '+' : ''}${(julPnL+augPnL).toFixed(2)}%`);

  // 全局统计
  const totalPnL = tradeLog.reduce((s, t) => s + (t.pnl || 0), 0);
  const wins = tradeLog.filter(t => t.pnl > 0).length;
  console.log(`\n全部历史: ${tradeLog.length}笔, 胜${wins}笔, 总收益: ${totalPnL >= 0 ? '+' : ''}${totalPnL.toFixed(2)}%`);
}

// ============================================================
// 矩阵测试
// ============================================================
const hardStops = [1.0, 1.5, 2.0, 2.5];
const activates = [2, 3, 4, 5, 6];
const callbacks = [1, 2, 3, 4];

// --- 5m穿越 + 无过滤 (BTC基准) ---
console.log("\n" + "=".repeat(70));
console.log("【A: 5m穿越入场, 无过滤 (BTC基准)】");
console.log("=".repeat(70));
console.log("hardStop | activate | callback | 交易数 | 胜率   | 总收益   ");
console.log("-".repeat(65));

let best5m = null;
for (const hs of hardStops) {
  for (const act of activates) {
    for (const cb of callbacks) {
      const r = run5mCross({ useHardStop: true, hardStopPct: hs, trailingActivate: act, trailingCallback: cb, slopeThreshold: 0, bbwThreshold: 0, volThreshold: 0 });
      if (!best5m || r.totalPnL > best5m.totalPnL) best5m = { ...r, hs, act, cb };
      if (r.totalPnL > 0) {
        console.log(`  ${hs}%    |    ${String(act).padStart(2)}%    |    ${String(cb).padStart(2)}%    | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ${(r.totalPnL>=0?'+':'')+r.totalPnL.toFixed(2).padStart(7)}%`);
      }
    }
  }
}
console.log(`\n5m穿越最优: hs=${best5m.hs}% act=${best5m.act}% cb=${best5m.cb}% → ${best5m.totalPnL.toFixed(2)}%, ${best5m.tradeCount}笔, 胜率${best5m.winRate.toFixed(1)}%`);

// --- 5m穿越 + BTC原有过滤 (slope+BBW+vol) ---
console.log("\n" + "=".repeat(70));
console.log("【A2: 5m穿越入场, slope≥5 + BBW≥2 + vol≥0.6】");
console.log("=".repeat(70));
console.log("hardStop | activate | callback | 交易数 | 胜率   | 总收益   ");
console.log("-".repeat(65));

let best5mFilter = null;
for (const hs of hardStops) {
  for (const act of activates) {
    for (const cb of callbacks) {
      const r = run5mCross({ useHardStop: true, hardStopPct: hs, trailingActivate: act, trailingCallback: cb, slopeThreshold: 5, bbwThreshold: 2.0, volThreshold: 0.6 });
      if (!best5mFilter || r.totalPnL > best5mFilter.totalPnL) best5mFilter = { ...r, hs, act, cb };
      if (r.totalPnL > 0) {
        console.log(`  ${hs}%    |    ${String(act).padStart(2)}%    |    ${String(cb).padStart(2)}%    | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ${(r.totalPnL>=0?'+':'')+r.totalPnL.toFixed(2).padStart(7)}%`);
      }
    }
  }
}
console.log(`\n5m穿越+过滤最优: hs=${best5mFilter.hs}% act=${best5mFilter.act}% cb=${best5mFilter.cb}% → ${best5mFilter.totalPnL.toFixed(2)}%, ${best5mFilter.tradeCount}笔, 胜率${best5mFilter.winRate.toFixed(1)}%`);

// --- 30m穿越 + 无扩散 ---
console.log("\n" + "=".repeat(70));
console.log("【B: 30m穿越入场, 无扩散】");
console.log("=".repeat(70));
console.log("hardStop | activate | callback | 交易数 | 胜率   | 总收益   ");
console.log("-".repeat(65));

let best30m = null;
for (const hs of hardStops) {
  for (const act of activates) {
    for (const cb of callbacks) {
      const r = run30mCross({ useHardStop: true, hardStopPct: hs, trailingActivate: act, trailingCallback: cb, slopeThreshold: 0, bbwThreshold: 0, volThreshold: 0 });
      if (!best30m || r.totalPnL > best30m.totalPnL) best30m = { ...r, hs, act, cb };
      if (r.totalPnL > 0) {
        console.log(`  ${hs}%    |    ${String(act).padStart(2)}%    |    ${String(cb).padStart(2)}%    | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ${(r.totalPnL>=0?'+':'')+r.totalPnL.toFixed(2).padStart(7)}%`);
      }
    }
  }
}
console.log(`\n30m穿越最优: hs=${best30m.hs}% act=${best30m.act}% cb=${best30m.cb}% → ${best30m.totalPnL.toFixed(2)}%, ${best30m.tradeCount}笔, 胜率${best30m.winRate.toFixed(1)}%`);

// --- 30m穿越 + BTC过滤 ---
console.log("\n" + "=".repeat(70));
console.log("【B2: 30m穿越入场, slope≥5 + BBW≥2 + vol≥0.6】");
console.log("=".repeat(70));
console.log("hardStop | activate | callback | 交易数 | 胜率   | 总收益   ");
console.log("-".repeat(65));

let best30mFilter = null;
for (const hs of hardStops) {
  for (const act of activates) {
    for (const cb of callbacks) {
      const r = run30mCross({ useHardStop: true, hardStopPct: hs, trailingActivate: act, trailingCallback: cb, slopeThreshold: 5, bbwThreshold: 2.0, volThreshold: 0.6 });
      if (!best30mFilter || r.totalPnL > best30mFilter.totalPnL) best30mFilter = { ...r, hs, act, cb };
      if (r.totalPnL > 0) {
        console.log(`  ${hs}%    |    ${String(act).padStart(2)}%    |    ${String(cb).padStart(2)}%    | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ${(r.totalPnL>=0?'+':'')+r.totalPnL.toFixed(2).padStart(7)}%`);
      }
    }
  }
}
console.log(`\n30m穿越+过滤最优: hs=${best30mFilter.hs}% act=${best30mFilter.act}% cb=${best30mFilter.cb}% → ${best30mFilter.totalPnL.toFixed(2)}%, ${best30mFilter.tradeCount}笔, 胜率${best30mFilter.winRate.toFixed(1)}%`);

// ============================================================
// 汇总
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【汇总对比】");
console.log("=".repeat(70));
console.log(`
策略                    | 参数                       | 收益      | 交易数 | 胜率
------------------------|----------------------------|-----------|--------|------
5m穿越 + 无过滤          | hs=${best5m.hs}% act=${best5m.act}% cb=${best5m.cb}% | ${(best5m.totalPnL>=0?'+':'')+best5m.totalPnL.toFixed(2)}%   | ${String(best5m.tradeCount).padStart(6)} | ${best5m.winRate.toFixed(1)}%
5m穿越 + slope+BBW+vol   | hs=${best5mFilter.hs}% act=${best5mFilter.act}% cb=${best5mFilter.cb}% | ${(best5mFilter.totalPnL>=0?'+':'')+best5mFilter.totalPnL.toFixed(2)}%   | ${String(best5mFilter.tradeCount).padStart(6)} | ${best5mFilter.winRate.toFixed(1)}%
30m穿越 + 无过滤          | hs=${best30m.hs}% act=${best30m.act}% cb=${best30m.cb}% | ${(best30m.totalPnL>=0?'+':'')+best30m.totalPnL.toFixed(2)}%   | ${String(best30m.tradeCount).padStart(6)} | ${best30m.winRate.toFixed(1)}%
30m穿越 + slope+BBW+vol   | hs=${best30mFilter.hs}% act=${best30mFilter.act}% cb=${best30mFilter.cb}% | ${(best30mFilter.totalPnL>=0?'+':'')+best30mFilter.totalPnL.toFixed(2)}%   | ${String(best30mFilter.tradeCount).padStart(6)} | ${best30mFilter.winRate.toFixed(1)}%
`);

console.log("=".repeat(70));
console.log("分析完成！");
console.log("=".repeat(70));
