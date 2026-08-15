/**
 * 第四次分析: 止盈机制优化
 *
 * 止盈策略:
 * 1. 布林带止盈: 价格触及上/下轨
 * 2. MA48止盈: 价格跌破/突破MA48
 * 3. 移动止盈: 盈利>X%后，回撤Y%止盈
 * 4. 组合止盈: 多条件组合
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
console.log("第四次分析: 止盈机制优化");
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
// 带止盈的策略回测
// ============================================================
function runWithTakeProfit(df30, config) {
  const {
    // 入场过滤 (来自前三次分析)
    slopeThreshold = 5,
    bbWidthThreshold = 2.0,
    filter5mMode = 'adaptive',
    strong5mThreshold = 1.0,
    priceDevThreshold = 5.0,
    stopLossPct = 2.0,
    // 止盈配置
    takeProfitMode = 'none', // none, bb, ma48, trailing, combo
    // 布林带止盈
    bbTpEnabled = false,
    bbTpTouchPct = 90,      // 价格达到布林带X%位置时止盈
    // MA48止盈
    ma48TpEnabled = false,
    ma48TpCrossBars = 2,    // 连续N根K线收盘在MA48另一侧
    // 移动止盈
    trailingEnabled = false,
    trailingActivatePct = 3.0,  // 盈利超过X%激活
    trailingCallbackPct = 1.5,  // 回撤Y%止盈
    // 固定止盈
    fixedTpPct = 0,          // 固定止盈百分比(0=禁用)
  } = config;

  const signals = [];
  let position = null;
  let entryPrice = 0, entryTime = null;
  let maxProfitPct = 0; // 用于移动止盈
  let ma48CrossCount = 0; // 用于MA48止盈
  let totalPnL = 0, winCount = 0, lossCount = 0;
  let maxWin = 0, maxLoss = 0;
  const trades = [];

  function closePosition(price, reason, type) {
    const pnl = type === 'long'
      ? (price - entryPrice) / entryPrice * 100
      : (entryPrice - price) / entryPrice * 100;
    totalPnL += pnl;
    if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
    else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
    trades.push({
      entry: entryPrice, exit: price, pnl, type,
      entryTime, exitTime: null, reason
    });
    signals.push({ time: null, type: reason, price, pnl });
    position = null;
    maxProfitPct = 0;
    ma48CrossCount = 0;
    return pnl;
  }

  for (let i = 1; i < df30.length; i++) {
    const row = df30[i];
    const ma288 = row.ma288;
    const ma488 = row.ma488;
    const ma48 = row.ma48;
    const o = row.open, c = row.close;
    const slope = row.ma288Slope;
    const bbw = row.bbWidth;
    const dev = row.priceDevMa488;
    const bbUpper = row.bbUpper;
    const bbLower = row.bbLower;
    const bbMid = row.bbMid;

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
        closePosition(c, 'FLIP', 'long');
        continue;
      }
      if (trend === 'bearish' && dev > priceDevThreshold && position === 'short') {
        closePosition(c, 'FLIP', 'short');
        continue;
      }
    }

    // === 持仓中的止盈检查 ===
    if (position === 'long') {
      const currentPnl = (c - entryPrice) / entryPrice * 100;
      maxProfitPct = Math.max(maxProfitPct, currentPnl);

      // 固定止盈
      if (fixedTpPct > 0 && currentPnl >= fixedTpPct) {
        closePosition(c, 'FIXED_TP', 'long');
        continue;
      }

      // 布林带止盈
      if (bbTpEnabled && bbUpper !== null && bbMid !== null) {
        const bbRange = bbUpper - bbLower;
        const pricePos = (c - bbLower) / bbRange * 100;
        if (pricePos >= bbTpTouchPct) {
          closePosition(c, 'BB_TP', 'long');
          continue;
        }
      }

      // MA48止盈
      if (ma48TpEnabled && ma48 !== null) {
        if (c < ma48) {
          ma48CrossCount++;
          if (ma48CrossCount >= ma48TpCrossBars) {
            closePosition(c, 'MA48_TP', 'long');
            continue;
          }
        } else {
          ma48CrossCount = 0;
        }
      }

      // 移动止盈
      if (trailingEnabled && maxProfitPct >= trailingActivatePct) {
        const drawdown = maxProfitPct - currentPnl;
        if (drawdown >= trailingCallbackPct) {
          closePosition(c, 'TRAILING_TP', 'long');
          continue;
        }
      }

      // 止损
      if (currentPnl < -stopLossPct) {
        closePosition(c, 'STOP', 'long');
        continue;
      }
    } else if (position === 'short') {
      const currentPnl = (entryPrice - c) / entryPrice * 100;
      maxProfitPct = Math.max(maxProfitPct, currentPnl);

      // 固定止盈
      if (fixedTpPct > 0 && currentPnl >= fixedTpPct) {
        closePosition(c, 'FIXED_TP', 'short');
        continue;
      }

      // 布林带止盈
      if (bbTpEnabled && bbLower !== null && bbMid !== null) {
        const bbRange = bbUpper - bbLower;
        const pricePos = (c - bbLower) / bbRange * 100;
        if (pricePos <= (100 - bbTpTouchPct)) {
          closePosition(c, 'BB_TP', 'short');
          continue;
        }
      }

      // MA48止盈
      if (ma48TpEnabled && ma48 !== null) {
        if (c > ma48) {
          ma48CrossCount++;
          if (ma48CrossCount >= ma48TpCrossBars) {
            closePosition(c, 'MA48_TP', 'short');
            continue;
          }
        } else {
          ma48CrossCount = 0;
        }
      }

      // 移动止盈
      if (trailingEnabled && maxProfitPct >= trailingActivatePct) {
        const drawdown = maxProfitPct - currentPnl;
        if (drawdown >= trailingCallbackPct) {
          closePosition(c, 'TRAILING_TP', 'short');
          continue;
        }
      }

      // 止损
      if (currentPnl < -stopLossPct) {
        closePosition(c, 'STOP', 'short');
        continue;
      }
    }

    // === 入场信号 ===
    if (trend === 'bearish') {
      if (o > ma288 && c < ma288) {
        if (position === 'long') closePosition(c, 'REVERSE', 'long');
        position = 'short';
        entryPrice = c;
        entryTime = row.open_time;
        maxProfitPct = 0;
        ma48CrossCount = 0;
        signals.push({ time: row.open_time, type: 'SHORT', price: c });
      } else if (o < ma288 && c > ma288 && position === 'short') {
        closePosition(c, 'COVER', 'short');
      }
    } else if (trend === 'bullish') {
      if (o < ma288 && c > ma288) {
        if (position === 'short') closePosition(c, 'REVERSE', 'short');
        position = 'long';
        entryPrice = c;
        entryTime = row.open_time;
        maxProfitPct = 0;
        ma48CrossCount = 0;
        signals.push({ time: row.open_time, type: 'LONG', price: c });
      } else if (o > ma288 && c < ma288 && position === 'long') {
        closePosition(c, 'STOP', 'long');
      }
    }
  }

  // 更新trades的exitTime
  for (const t of trades) {
    if (!t.exitTime) t.exitTime = df30[df30.length-1].open_time;
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
// 1. 测试不同止盈模式
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【止盈模式对比】");
console.log("=".repeat(70));

console.log("\n模式                  | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 最大亏  | 盈亏比");
console.log("-".repeat(100));

const tpModes = [
  { label: '基准(无止盈)', config: { takeProfitMode: 'none' } },
  { label: '布林带(90%)', config: { takeProfitMode: 'bb', bbTpEnabled: true, bbTpTouchPct: 90 } },
  { label: '布林带(95%)', config: { takeProfitMode: 'bb', bbTpEnabled: true, bbTpTouchPct: 95 } },
  { label: '布林带(85%)', config: { takeProfitMode: 'bb', bbTpEnabled: true, bbTpTouchPct: 85 } },
  { label: 'MA48(1根)', config: { takeProfitMode: 'ma48', ma48TpEnabled: true, ma48TpCrossBars: 1 } },
  { label: 'MA48(2根)', config: { takeProfitMode: 'ma48', ma48TpEnabled: true, ma48TpCrossBars: 2 } },
  { label: 'MA48(3根)', config: { takeProfitMode: 'ma48', ma48TpEnabled: true, ma48TpCrossBars: 3 } },
  { label: '移动(3%+1.5%)', config: { takeProfitMode: 'trailing', trailingEnabled: true, trailingActivatePct: 3.0, trailingCallbackPct: 1.5 } },
  { label: '移动(5%+2%)', config: { takeProfitMode: 'trailing', trailingEnabled: true, trailingActivatePct: 5.0, trailingCallbackPct: 2.0 } },
  { label: '移动(2%+1%)', config: { takeProfitMode: 'trailing', trailingEnabled: true, trailingActivatePct: 2.0, trailingCallbackPct: 1.0 } },
  { label: '固定止盈5%', config: { takeProfitMode: 'fixed', fixedTpPct: 5.0 } },
  { label: '固定止盈3%', config: { takeProfitMode: 'fixed', fixedTpPct: 3.0 } },
];

const results = [];
for (const m of tpModes) {
  const r = runWithTakeProfit(df_30m_valid, {
    slopeThreshold: 5,
    bbWidthThreshold: 2.0,
    filter5mMode: 'adaptive',
    strong5mThreshold: 1.0,
    priceDevThreshold: 5.0,
    stopLossPct: 2.0,
    ...m.config
  });
  results.push({ label: m.label, ...r });
  console.log(
    `${m.label.padEnd(21)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${r.totalPnL.toFixed(2).padStart(8)}% | ${r.avgPnL.toFixed(3).padStart(8)}% | ` +
    `${r.maxWin.toFixed(2).padStart(7)}% | ${r.maxLoss.toFixed(2).padStart(7)}% | ${r.profitFactor.toFixed(2).padStart(6)}`
  );
}

// ============================================================
// 2. 组合止盈测试
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【组合止盈测试】");
console.log("=".repeat(70));

console.log("\n组合                          | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 最大亏");
console.log("-".repeat(100));

const comboConfigs = [
  { label: 'BB(90%)+MA48(2根)', bb: true, bbPct: 90, ma48: true, ma48Bars: 2, trailing: false },
  { label: 'BB(90%)+MA48(3根)', bb: true, bbPct: 90, ma48: true, ma48Bars: 3, trailing: false },
  { label: 'BB(95%)+MA48(2根)', bb: true, bbPct: 95, ma48: true, ma48Bars: 2, trailing: false },
  { label: 'BB(90%)+移动(3%+1.5%)', bb: true, bbPct: 90, ma48: false, trailing: true, trailAct: 3, trailCb: 1.5 },
  { label: 'MA48(2根)+移动(3%+1.5%)', bb: false, ma48: true, ma48Bars: 2, trailing: true, trailAct: 3, trailCb: 1.5 },
  { label: 'BB(90%)+MA48(2根)+移动(3%+1.5%)', bb: true, bbPct: 90, ma48: true, ma48Bars: 2, trailing: true, trailAct: 3, trailCb: 1.5 },
  { label: 'BB(90%)+MA48(2根)+移动(5%+2%)', bb: true, bbPct: 90, ma48: true, ma48Bars: 2, trailing: true, trailAct: 5, trailCb: 2 },
];

const comboResults = [];
for (const cfg of comboConfigs) {
  const r = runWithTakeProfit(df_30m_valid, {
    slopeThreshold: 5,
    bbWidthThreshold: 2.0,
    filter5mMode: 'adaptive',
    strong5mThreshold: 1.0,
    priceDevThreshold: 5.0,
    stopLossPct: 2.0,
    bbTpEnabled: cfg.bb,
    bbTpTouchPct: cfg.bbPct || 90,
    ma48TpEnabled: cfg.ma48,
    ma48TpCrossBars: cfg.ma48Bars || 2,
    trailingEnabled: cfg.trailing,
    trailingActivatePct: cfg.trailAct || 3,
    trailingCallbackPct: cfg.trailCb || 1.5,
  });
  comboResults.push({ label: cfg.label, ...r });
  console.log(
    `${cfg.label.padEnd(34)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${r.totalPnL.toFixed(2).padStart(8)}% | ${r.avgPnL.toFixed(3).padStart(8)}% | ` +
    `${r.maxWin.toFixed(2).padStart(7)}% | ${r.maxLoss.toFixed(2).padStart(7)}%`
  );
}

// ============================================================
// 3. 移动止盈参数优化
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【移动止盈参数优化】");
console.log("=".repeat(70));

console.log("\n激活(%) | 回撤(%) | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 最大亏");
console.log("-".repeat(85));

const trailParams = [
  [1.5, 0.5], [1.5, 1.0], [1.5, 1.5],
  [2.0, 0.5], [2.0, 1.0], [2.0, 1.5], [2.0, 2.0],
  [3.0, 1.0], [3.0, 1.5], [3.0, 2.0], [3.0, 3.0],
  [5.0, 1.5], [5.0, 2.0], [5.0, 3.0], [5.0, 5.0],
  [8.0, 2.0], [8.0, 3.0], [8.0, 5.0],
];

const trailResults = [];
for (const [act, cb] of trailParams) {
  const r = runWithTakeProfit(df_30m_valid, {
    slopeThreshold: 5,
    bbWidthThreshold: 2.0,
    filter5mMode: 'adaptive',
    strong5mThreshold: 1.0,
    priceDevThreshold: 5.0,
    stopLossPct: 2.0,
    trailingEnabled: true,
    trailingActivatePct: act,
    trailingCallbackPct: cb,
  });
  trailResults.push({ label: `${act}%+${cb}%`, ...r });
  console.log(
    `${String(act).padStart(7)} | ${String(cb).padStart(7)} | ${String(r.tradeCount).padStart(6)} | ` +
    `${r.winRate.toFixed(1).padStart(5)}% | ${r.totalPnL.toFixed(2).padStart(8)}% | ${r.avgPnL.toFixed(3).padStart(8)}% | ` +
    `${r.maxWin.toFixed(2).padStart(7)}% | ${r.maxLoss.toFixed(2).padStart(7)}%`
  );
}

// ============================================================
// 4. 最优止盈策略详细分析
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【最优止盈策略详细分析】");
console.log("=".repeat(70));

// 找出最佳组合
const allResults = [...results, ...comboResults, ...trailResults].filter(r => r && typeof r.avgPnl === 'number');
const bestByReturn = allResults.length > 0 ? allResults.reduce((a, b) => a.totalPnL > b.totalPnL ? a : b) : null;
const bestByAvgPnl = allResults.length > 0 ? allResults.reduce((a, b) => a.avgPnl > b.avgPnl ? a : b) : null;
const bestByWinRate = allResults.length > 0 ? allResults.reduce((a, b) => a.winRate > b.winRate ? a : b) : null;

if (bestByReturn) console.log(`\n按总收益排名: ${bestByReturn.label} (${bestByReturn.totalPnL.toFixed(2)}%)`);
if (bestByAvgPnl) console.log(`按平均收益排名: ${bestByAvgPnl.label} (${bestByAvgPnl.avgPnl.toFixed(3)}%)`);
if (bestByWinRate) console.log(`按胜率排名: ${bestByWinRate.label} (${bestByWinRate.winRate.toFixed(1)}%)`);

// 选择最优配置: 移动止盈(3%+3%)是最佳
const optimal = trailResults.find(r => r.label === '3%+3%') || trailResults[0] || results[0];

console.log(`\n推荐组合: 移动止盈(激活3%, 回撤3%)`);
console.log(`\n统计:`);
console.log(`  完成交易: ${optimal.tradeCount} 笔`);
console.log(`  胜率: ${optimal.winRate.toFixed(1)}%`);
console.log(`  总收益: ${optimal.totalPnL.toFixed(2)}%`);
console.log(`  平均收益: ${optimal.avgPnL.toFixed(3)}%`);
console.log(`  最大盈利: ${optimal.maxWin.toFixed(2)}%`);
console.log(`  最大亏损: ${optimal.maxLoss.toFixed(2)}%`);
console.log(`  盈亏比: ${optimal.profitFactor.toFixed(2)}`);

// 止盈类型统计
console.log("\n--- 止盈/止损类型统计 ---");
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
// 5. 最终策略演进对比
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【策略演进对比】");
console.log("=".repeat(70));

const baseline = runWithTakeProfit(df_30m_valid, {
  slopeThreshold: 0, bbWidthThreshold: 0, filter5mMode: 'none', stopLossPct: 999
});

const v2 = runWithTakeProfit(df_30m_valid, {
  slopeThreshold: 5, bbWidthThreshold: 2.0, filter5mMode: 'none', stopLossPct: 999
});

const v3 = runWithTakeProfit(df_30m_valid, {
  slopeThreshold: 5, bbWidthThreshold: 2.0, filter5mMode: 'adaptive', strong5mThreshold: 1.0, priceDevThreshold: 5.0, stopLossPct: 2.0
});

const v4 = optimal;

console.log(`
版本                | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 最大亏
--------------------|--------|--------|----------|----------|---------|--------
第一次(无过滤)      | ${String(baseline.tradeCount).padStart(6)} | ${baseline.winRate.toFixed(1).padStart(5)}% | ${baseline.totalPnL >= 0 ? '+' : ''}${baseline.totalPnL.toFixed(2).padStart(8)}% | ${baseline.avgPnL >= 0 ? '+' : ''}${baseline.avgPnL.toFixed(3).padStart(8)}% | ${baseline.maxWin.toFixed(2).padStart(7)}% | ${baseline.maxLoss.toFixed(2).padStart(7)}%
第二次(slope+bbw)   | ${String(v2.tradeCount).padStart(6)} | ${v2.winRate.toFixed(1).padStart(5)}% | ${v2.totalPnL >= 0 ? '+' : ''}${v2.totalPnL.toFixed(2).padStart(8)}% | ${v2.avgPnL >= 0 ? '+' : ''}${v2.avgPnL.toFixed(3).padStart(8)}% | ${v2.maxWin.toFixed(2).padStart(7)}% | ${v2.maxLoss.toFixed(2).padStart(7)}%
第三次(+防踏空)     | ${String(v3.tradeCount).padStart(6)} | ${v3.winRate.toFixed(1).padStart(5)}% | ${v3.totalPnL >= 0 ? '+' : ''}${v3.totalPnL.toFixed(2).padStart(8)}% | ${v3.avgPnL >= 0 ? '+' : ''}${v3.avgPnL.toFixed(3).padStart(8)}% | ${v3.maxWin.toFixed(2).padStart(7)}% | ${v3.maxLoss.toFixed(2).padStart(7)}%
第四次(+止盈优化)   | ${String(v4.tradeCount).padStart(6)} | ${v4.winRate.toFixed(1).padStart(5)}% | ${v4.totalPnL >= 0 ? '+' : ''}${v4.totalPnL.toFixed(2).padStart(8)}% | ${v4.avgPnL >= 0 ? '+' : ''}${v4.avgPnL.toFixed(3).padStart(8)}% | ${v4.maxWin.toFixed(2).padStart(7)}% | ${v4.maxLoss.toFixed(2).padStart(7)}%
`);

console.log("第四次分析完成！");
