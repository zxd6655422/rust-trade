/**
 * SOL 第十四次分析: 亏损原因分析 + 多维矩阵测试
 *
 * 1. 分析每笔亏损: 入场即亏 vs 盈利变亏损
 * 2. 矩阵测试: hardStop × trailingActivate × trailingCallback × 扩散过滤
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
const df_5m = loadCSV('../kline_5m_202608010054_SOLUSDT.csv', 'open_time');
const df_30m = loadCSV('../kline_30m_202608010054_SOLUSDT.csv', 'open_time');
console.log(`5m: ${df_5m.length}, 30m: ${df_30m.length}`);

// ============================================================
// 计算30m指标
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
  for (let i = 0; i < df.length; i++) {
    df[i].ma288_30m = ma288[i];
    df[i].ma488_30m = ma488[i];
    df[i].spread_30m = spread[i];
    df[i].isExpanding_30m = isExpanding[i];
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
    df[i].ma288_5m = ma288[i];
    df[i].ma488_5m = ma488[i];
    df[i].isExpanding_5m = isExpanding[i];
  }
}

console.log("计算指标...");
add30mIndicators(df_30m);
add5mIndicators(df_5m);

// ============================================================
// 预构建30m查找数组 (二分查找优化)
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

// 预构建5m查找
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
// 策略回测 (带详细交易记录)
// ============================================================
function runStrategyDetailed(config) {
  const {
    useHardStop = true, hardStopPct = 2.5,
    tpMode = 'trailing', trailingActivate = 6.0, trailingCallback = 2.0,
    use30mExpanding = true, use5mExpanding = true,
  } = config;

  let position = null;
  let entryPrice = 0, hardStopPrice = 0, maxProfitPct = 0;
  let totalPnL = 0, winCount = 0, lossCount = 0;
  let maxWin = 0, maxLoss = 0;
  const trades = [];

  for (let i = 1; i < df_5m.length; i++) {
    const row = df_5m[i];
    const o = row.open, h = row.high, l = row.low, c = row.close;
    const timeMs = row.open_time.getTime();

    const data30m = get30mAtFast(timeMs);
    if (!data30m) continue;

    const ma288 = data30m.ma288_30m;
    const ma488 = data30m.ma488_30m;
    const trend = ma288 > ma488 ? 'bullish' : (ma288 < ma488 ? 'bearish' : null);
    if (!trend) continue;

    if (use30mExpanding && data30m.isExpanding_30m === false) continue;
    if (use5mExpanding && row.isExpanding_5m === false) continue;

    // 持仓管理
    if (position !== null) {
      const currentPnl = position === 'long'
        ? (c - entryPrice) / entryPrice * 100
        : (entryPrice - c) / entryPrice * 100;
      const prevMax = maxProfitPct;
      maxProfitPct = Math.max(maxProfitPct, currentPnl);

      let shouldStop = false, exitPrice = c, exitReason = '';

      // 硬止损
      if (useHardStop) {
        if (position === 'long' && l <= hardStopPrice) {
          shouldStop = true; exitPrice = hardStopPrice; exitReason = 'hardStop';
        } else if (position === 'short' && h >= hardStopPrice) {
          shouldStop = true; exitPrice = hardStopPrice; exitReason = 'hardStop';
        }
      }

      // MA288止损
      if (!shouldStop) {
        if (position === 'long' && o > ma288 && c < ma288) {
          shouldStop = true; exitReason = 'ma288Stop';
        } else if (position === 'short' && o < ma288 && c > ma288) {
          shouldStop = true; exitReason = 'ma288Stop';
        }
      }

      if (shouldStop) {
        const pnl = position === 'long'
          ? (exitPrice - entryPrice) / entryPrice * 100
          : (entryPrice - exitPrice) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
        else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({
          pnl, side: position, exit: exitReason, time: row.open_time,
          maxProfit: maxProfitPct, entryTime: trades[trades.length - 1]?.nextEntryTime,
        });
        position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0;
        continue;
      }

      // 移动止盈
      if (tpMode === 'trailing' && maxProfitPct >= trailingActivate) {
        if (maxProfitPct - currentPnl >= trailingCallback) {
          totalPnL += currentPnl;
          if (currentPnl > 0) { winCount++; maxWin = Math.max(maxWin, currentPnl); }
          else { lossCount++; maxLoss = Math.min(maxLoss, currentPnl); }
          trades.push({
            pnl: currentPnl, side: position, exit: 'trailing', time: row.open_time,
            maxProfit: maxProfitPct,
          });
          position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0;
          continue;
        }
      }
    }

    // 入场
    let isEntry = false, entryDir = '';
    if (trend === 'bullish' && o < ma288 && c > ma288) {
      isEntry = true; entryDir = 'long';
    } else if (trend === 'bearish' && o > ma288 && c < ma288) {
      isEntry = true; entryDir = 'short';
    }

    if (isEntry) {
      if (position !== null && position !== entryDir) {
        const pnl = position === 'long'
          ? (c - entryPrice) / entryPrice * 100
          : (entryPrice - c) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
        else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({
          pnl, side: position, exit: 'reverse', time: row.open_time,
          maxProfit: maxProfitPct,
        });
        position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0;
      }
      if (position === null) {
        position = entryDir;
        entryPrice = c;
        maxProfitPct = 0;
        hardStopPrice = entryDir === 'long'
          ? entryPrice * (1 - hardStopPct / 100)
          : entryPrice * (1 + hardStopPct / 100);
      }
    }
  }

  // 统计
  const longTrades = trades.filter(t => t.side === 'long');
  const shortTrades = trades.filter(t => t.side === 'short');
  const longPnL = longTrades.reduce((sum, t) => sum + t.pnl, 0);
  const shortPnL = shortTrades.reduce((sum, t) => sum + t.pnl, 0);
  const longWins = longTrades.filter(t => t.pnl > 0).length;
  const shortWins = shortTrades.filter(t => t.pnl > 0).length;

  return {
    tradeCount: trades.length, winCount, lossCount,
    winRate: trades.length > 0 ? (winCount / trades.length * 100) : 0,
    totalPnL, avgPnL: trades.length > 0 ? (totalPnL / trades.length) : 0,
    maxWin, maxLoss,
    longCount: longTrades.length, longPnL, longWinRate: longTrades.length > 0 ? (longWins / longTrades.length * 100) : 0,
    shortCount: shortTrades.length, shortPnL, shortWinRate: shortTrades.length > 0 ? (shortWins / shortTrades.length * 100) : 0,
    trades,
  };
}

// ============================================================
// 快速回测 (不记录详细交易, 用于矩阵扫描)
// ============================================================
function runStrategyFast(config) {
  const {
    useHardStop = true, hardStopPct = 2.5,
    tpMode = 'trailing', trailingActivate = 6.0, trailingCallback = 2.0,
    use30mExpanding = true, use5mExpanding = true,
  } = config;

  let position = null;
  let entryPrice = 0, hardStopPrice = 0, maxProfitPct = 0;
  let totalPnL = 0, winCount = 0, lossCount = 0;
  let tradeCount = 0;

  for (let i = 1; i < df_5m.length; i++) {
    const row = df_5m[i];
    const o = row.open, h = row.high, l = row.low, c = row.close;
    const timeMs = row.open_time.getTime();

    const data30m = get30mAtFast(timeMs);
    if (!data30m) continue;

    const ma288 = data30m.ma288_30m;
    const ma488 = data30m.ma488_30m;
    const trend = ma288 > ma488 ? 'bullish' : (ma288 < ma488 ? 'bearish' : null);
    if (!trend) continue;

    if (use30mExpanding && data30m.isExpanding_30m === false) continue;
    if (use5mExpanding && row.isExpanding_5m === false) continue;

    if (position !== null) {
      const currentPnl = position === 'long'
        ? (c - entryPrice) / entryPrice * 100
        : (entryPrice - c) / entryPrice * 100;
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
        totalPnL += pnl;
        if (pnl > 0) winCount++; else lossCount++;
        tradeCount++;
        position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0;
        continue;
      }

      if (tpMode === 'trailing' && maxProfitPct >= trailingActivate) {
        if (maxProfitPct - currentPnl >= trailingCallback) {
          totalPnL += currentPnl;
          if (currentPnl > 0) winCount++; else lossCount++;
          tradeCount++;
          position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0;
          continue;
        }
      }
    }

    let isEntry = false, entryDir = '';
    if (trend === 'bullish' && o < ma288 && c > ma288) { isEntry = true; entryDir = 'long'; }
    else if (trend === 'bearish' && o > ma288 && c < ma288) { isEntry = true; entryDir = 'short'; }

    if (isEntry) {
      if (position !== null && position !== entryDir) {
        const pnl = position === 'long' ? (c - entryPrice) / entryPrice * 100 : (entryPrice - c) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) winCount++; else lossCount++;
        tradeCount++;
        position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0;
      }
      if (position === null) {
        position = entryDir; entryPrice = c; maxProfitPct = 0;
        hardStopPrice = entryDir === 'long' ? entryPrice * (1 - hardStopPct / 100) : entryPrice * (1 + hardStopPct / 100);
      }
    }
  }

  return {
    tradeCount, winCount, lossCount,
    winRate: tradeCount > 0 ? (winCount / tradeCount * 100) : 0,
    totalPnL,
  };
}

// ============================================================
// 1. 亏损原因分析
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【亏损原因分析: 默认参数 hardStop=2.5%, trailing 6+2】");
console.log("=".repeat(70));

const detail = runStrategyDetailed({
  useHardStop: true, hardStopPct: 2.5,
  tpMode: 'trailing', trailingActivate: 6.0, trailingCallback: 2.0,
  use30mExpanding: true, use5mExpanding: true,
});

const lossTrades = detail.trades.filter(t => t.pnl < 0);
const winTrades = detail.trades.filter(t => t.pnl > 0);

// 分类亏损
const lossImmediate = lossTrades.filter(t => t.maxProfit <= 0);           // 入场后从未盈利
const lossSmallProfit = lossTrades.filter(t => t.maxProfit > 0 && t.maxProfit < 1);  // 有小幅盈利但变亏
const lossBigProfit = lossTrades.filter(t => t.maxProfit >= 1);           // 盈利>=1%但变亏

console.log(`\n总交易: ${detail.tradeCount}, 盈利: ${winTrades.length}, 亏损: ${lossTrades.length}`);
console.log(`\n--- 亏损分类 ---`);
console.log(`入场即亏 (从未盈利):     ${lossImmediate.length}笔 (${(lossImmediate.length/lossTrades.length*100).toFixed(1)}%)`);
console.log(`小幅盈利变亏 (max<1%):   ${lossSmallProfit.length}笔 (${(lossSmallProfit.length/lossTrades.length*100).toFixed(1)}%)`);
console.log(`盈利>=1%变亏:            ${lossBigProfit.length}笔 (${(lossBigProfit.length/lossTrades.length*100).toFixed(1)}%)`);

// 按退出原因统计
const exitReasons = {};
for (const t of lossTrades) {
  const key = t.exit || 'unknown';
  if (!exitReasons[key]) exitReasons[key] = { count: 0, totalPnl: 0, immediate: 0, smallProfit: 0, bigProfit: 0 };
  exitReasons[key].count++;
  exitReasons[key].totalPnl += t.pnl;
  if (t.maxProfit <= 0) exitReasons[key].immediate++;
  else if (t.maxProfit < 1) exitReasons[key].smallProfit++;
  else exitReasons[key].bigProfit++;
}

console.log(`\n--- 按退出原因 ---`);
for (const [reason, data] of Object.entries(exitReasons)) {
  console.log(`${reason}: ${data.count}笔, 总亏损=${data.totalPnl.toFixed(2)}%, 入场即亏=${data.immediate}, 小盈变亏=${data.smallProfit}, 大盈变亏=${data.bigProfit}`);
}

// 按方向统计
const longLosses = lossTrades.filter(t => t.side === 'long');
const shortLosses = lossTrades.filter(t => t.side === 'short');
console.log(`\n--- 按方向 ---`);
console.log(`做多亏损: ${longLosses.length}笔, 入场即亏=${longLosses.filter(t=>t.maxProfit<=0).length}, 盈利变亏=${longLosses.filter(t=>t.maxProfit>0).length}`);
console.log(`做空亏损: ${shortLosses.length}笔, 入场即亏=${shortLosses.filter(t=>t.maxProfit<=0).length}, 盈利变亏=${shortLosses.filter(t=>t.maxProfit>0).length}`);

// 盈利变亏的详细分析
if (lossBigProfit.length > 0) {
  console.log(`\n--- 盈利>=1%变亏的交易 (最大浮盈 → 最终亏损) ---`);
  for (const t of lossBigProfit.slice(0, 15)) {
    console.log(`  ${t.time.toISOString().substring(0,16)} | ${t.side.padEnd(5)} | maxProfit=+${t.maxProfit.toFixed(2)}% → PnL=${t.pnl.toFixed(2)}% | ${t.exit}`);
  }
}

// ============================================================
// 2. 多维矩阵测试
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【矩阵测试: hardStop × trailingActivate × trailingCallback】");
console.log("=".repeat(70));

const hardStops = [1.5, 2.0, 2.5, 3.0];
const activates = [2, 3, 4, 5, 6];
const callbacks = [1, 2, 3, 4];

let best = null;

// 先测扩散全开
console.log("\n--- 30m扩散✅ + 5m扩散✅ ---");
console.log("hardStop | activate | callback | 交易数 | 胜率   | 总收益   ");
console.log("-".repeat(65));

for (const hs of hardStops) {
  for (const act of activates) {
    for (const cb of callbacks) {
      const r = runStrategyFast({
        useHardStop: true, hardStopPct: hs,
        tpMode: 'trailing', trailingActivate: act, trailingCallback: cb,
        use30mExpanding: true, use5mExpanding: true,
      });
      const key = `${hs}-${act}-${cb}`;
      if (!best || r.totalPnL > best.totalPnL) best = { ...r, hs, act, cb, expand: 'both' };
      // 只打印有价值的组合
      if (r.totalPnL > 0 || r.winRate > 20) {
        console.log(`  ${hs}%    |    ${String(act).padStart(2)}%    |    ${String(cb).padStart(2)}%    | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ${(r.totalPnL>=0?'+':'')+r.totalPnL.toFixed(2).padStart(7)}%`);
      }
    }
  }
}
console.log(`\n最优(扩散全开): hardStop=${best.hs}%, activate=${best.act}%, callback=${best.cb}% → ${best.totalPnL.toFixed(2)}%, ${best.tradeCount}笔, 胜率${best.winRate.toFixed(1)}%`);

// 测 30m扩散开 + 5m扩散关
console.log("\n--- 30m扩散✅ + 5m扩散❌ ---");
console.log("hardStop | activate | callback | 交易数 | 胜率   | 总收益   ");
console.log("-".repeat(65));

let bestNo5m = null;
for (const hs of hardStops) {
  for (const act of activates) {
    for (const cb of callbacks) {
      const r = runStrategyFast({
        useHardStop: true, hardStopPct: hs,
        tpMode: 'trailing', trailingActivate: act, trailingCallback: cb,
        use30mExpanding: true, use5mExpanding: false,
      });
      if (!bestNo5m || r.totalPnL > bestNo5m.totalPnL) bestNo5m = { ...r, hs, act, cb };
      if (r.totalPnL > 0 || r.winRate > 20) {
        console.log(`  ${hs}%    |    ${String(act).padStart(2)}%    |    ${String(cb).padStart(2)}%    | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ${(r.totalPnL>=0?'+':'')+r.totalPnL.toFixed(2).padStart(7)}%`);
      }
    }
  }
}
console.log(`\n最优(无5m扩散): hardStop=${bestNo5m.hs}%, activate=${bestNo5m.act}%, callback=${bestNo5m.cb}% → ${bestNo5m.totalPnL.toFixed(2)}%, ${bestNo5m.tradeCount}笔, 胜率${bestNo5m.winRate.toFixed(1)}%`);

// 测 扩散全关
console.log("\n--- 30m扩散❌ + 5m扩散❌ ---");
console.log("hardStop | activate | callback | 交易数 | 胜率   | 总收益   ");
console.log("-".repeat(65));

let bestNoExpand = null;
for (const hs of hardStops) {
  for (const act of activates) {
    for (const cb of callbacks) {
      const r = runStrategyFast({
        useHardStop: true, hardStopPct: hs,
        tpMode: 'trailing', trailingActivate: act, trailingCallback: cb,
        use30mExpanding: false, use5mExpanding: false,
      });
      if (!bestNoExpand || r.totalPnL > bestNoExpand.totalPnL) bestNoExpand = { ...r, hs, act, cb };
      if (r.totalPnL > 0 || r.winRate > 20) {
        console.log(`  ${hs}%    |    ${String(act).padStart(2)}%    |    ${String(cb).padStart(2)}%    | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ${(r.totalPnL>=0?'+':'')+r.totalPnL.toFixed(2).padStart(7)}%`);
      }
    }
  }
}
console.log(`\n最优(无扩散): hardStop=${bestNoExpand.hs}%, activate=${bestNoExpand.act}%, callback=${bestNoExpand.cb}% → ${bestNoExpand.totalPnL.toFixed(2)}%, ${bestNoExpand.tradeCount}笔, 胜率${bestNoExpand.winRate.toFixed(1)}%`);

// ============================================================
// 3. 最优组合详细分析
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【最优组合详细分析】");
console.log("=".repeat(70));

// 用最优参数跑详细回测
const optResult = runStrategyDetailed({
  useHardStop: true, hardStopPct: best.hs,
  tpMode: 'trailing', trailingActivate: best.act, trailingCallback: best.cb,
  use30mExpanding: true, use5mExpanding: true,
});

const optLoss = optResult.trades.filter(t => t.pnl < 0);
const optImmediate = optLoss.filter(t => t.maxProfit <= 0);
const optSmallProfit = optLoss.filter(t => t.maxProfit > 0 && t.maxProfit < 1);
const optBigProfit = optLoss.filter(t => t.maxProfit >= 1);

console.log(`\n参数: hardStop=${best.hs}%, activate=${best.act}%, callback=${best.cb}%`);
console.log(`交易: ${optResult.tradeCount}, 胜率: ${optResult.winRate.toFixed(1)}%, 总收益: ${optResult.totalPnL.toFixed(2)}%`);
console.log(`\n亏损分类:`);
console.log(`  入场即亏: ${optImmediate.length}笔`);
console.log(`  小盈变亏: ${optSmallProfit.length}笔`);
console.log(`  大盈变亏: ${optBigProfit.length}笔`);

// 最近20笔交易明细
console.log(`\n最近20笔交易:`);
console.log("时间               | 方向  | PnL     | maxProfit | 退出");
console.log("-".repeat(70));
for (const t of optResult.trades.slice(-20)) {
  console.log(`${t.time.toISOString().substring(0,16)} | ${t.side.padEnd(5)} | ${(t.pnl>=0?'+':'')+t.pnl.toFixed(2).padStart(6)}% | +${t.maxProfit.toFixed(2).padStart(5)}%  | ${t.exit}`);
}

console.log("\n" + "=".repeat(70));
console.log("分析完成！");
console.log("=".repeat(70));
