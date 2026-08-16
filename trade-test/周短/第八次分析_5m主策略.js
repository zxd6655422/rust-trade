/**
 * 第八次分析: 5m双均线主策略 + 30m止盈优化
 *
 * 思路:
 * - 5m MA288/MA488 作为主趋势判断和入场信号
 * - 30m MA/布林带 作为止盈参考
 * - 测试不同的30m止盈方式
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
console.log("第八次分析: 5m双均线主策略 + 30m止盈优化");
console.log("=".repeat(70));

const df_5m = loadCSV('kline_5m_202607222208.csv', 'open_time');
const df_30m = loadCSV('kline_30m_202607222207.csv', 'open_time');

// ============================================================
// 计算指标
// ============================================================
function addIndicators(df, prefix = '') {
  const closes = df.map(r => r.close);
  const volumes = df.map(r => r.volume);

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

  // 布林带
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

  // MA288斜率
  const ma288Slope = new Array(df.length).fill(null);
  for (let i = 5; i < df.length; i++) {
    if (ma288[i] !== null && ma288[i-5] !== null && ma288[i-5] !== 0) {
      ma288Slope[i] = (ma288[i] - ma288[i-5]) / ma288[i-5] * 10000;
    }
  }

  // 成交量MA
  const volMA = new Array(df.length).fill(null);
  const volRatio = new Array(df.length).fill(null);
  for (let i = 19; i < df.length; i++) {
    let sum = 0;
    for (let j = i - 19; j <= i; j++) sum += volumes[j];
    volMA[i] = sum / 20;
    if (volMA[i] > 0) volRatio[i] = volumes[i] / volMA[i];
  }

  for (let i = 0; i < df.length; i++) {
    df[i][`${prefix}ma48`] = ma48[i];
    df[i][`${prefix}ma288`] = ma288[i];
    df[i][`${prefix}ma488`] = ma488[i];
    df[i][`${prefix}bbMid`] = bbMid[i];
    df[i][`${prefix}bbUpper`] = bbUpper[i];
    df[i][`${prefix}bbLower`] = bbLower[i];
    df[i][`${prefix}bbWidth`] = bbWidth[i];
    df[i][`${prefix}ma288Slope`] = ma288Slope[i];
    df[i][`${prefix}volRatio`] = volRatio[i];
  }
  return df;
}

console.log("\n计算技术指标...");
addIndicators(df_5m, 'm5_');
addIndicators(df_30m, 'm30_');

const df_5m_valid = df_5m.filter(r => r.m5_ma288 !== null && r.m5_ma488 !== null);
const df_30m_valid = df_30m.filter(r => r.m30_ma288 !== null && r.m30_ma488 !== null);

console.log(`5m有效数据: ${df_5m_valid.length} 根`);
console.log(`30m有效数据: ${df_30m_valid.length} 根`);

// ============================================================
// 30m数据索引 (用于查找对应时间的30m指标)
// ============================================================
function build30mMap(df30) {
  const map = new Map();
  for (const r of df30) {
    map.set(r.open_time.getTime(), {
      ma48: r.m30_ma48,
      ma288: r.m30_ma288,
      ma488: r.m30_ma488,
      bbMid: r.m30_bbMid,
      bbUpper: r.m30_bbUpper,
      bbLower: r.m30_bbLower,
      bbWidth: r.m30_bbWidth,
      trend: r.m30_ma288 > r.m30_ma488 ? 'bullish' : 'bearish'
    });
  }
  return map;
}

const map30m = build30mMap(df_30m_valid);

function get30mAt(time) {
  const t = time.getTime();
  let best = null;
  let bestDiff = Infinity;
  for (const [ts, data] of map30m) {
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
// 5m策略统计
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【5m双均线基础统计】");
console.log("=".repeat(70));

let bullBars = 0, bearBars = 0;
for (const r of df_5m_valid) {
  if (r.m5_ma288 > r.m5_ma488) bullBars++;
  else bearBars++;
}
console.log(`\n5m趋势分布:`);
console.log(`  多头: ${bullBars} 根 (${(bullBars/df_5m_valid.length*100).toFixed(1)}%)`);
console.log(`  空头: ${bearBars} 根 (${(bearBars/df_5m_valid.length*100).toFixed(1)}%)`);

// 5m信号统计
let signalCount = 0;
for (let i = 1; i < df_5m_valid.length; i++) {
  const prev = df_5m_valid[i-1];
  const curr = df_5m_valid[i];
  if ((prev.m5_ma288 > prev.m5_ma488) !== (curr.m5_ma288 > curr.m5_ma488)) {
    signalCount++;
  }
}
console.log(`  趋势转换次数: ${signalCount} 次`);
console.log(`  平均每次趋势持续: ${(df_5m_valid.length / signalCount).toFixed(0)} 根K线 (${(df_5m_valid.length / signalCount * 5 / 60).toFixed(1)} 小时)`);

// ============================================================
// 策略回测函数
// ============================================================
function run5mStrategy(df5m, config) {
  const {
    // 5m入场参数
    slope5mThreshold = 0,      // 5m MA288斜率阈值
    vol5mThreshold = 0,        // 5m成交量阈值
    // 30m过滤
    filter30mEnabled = false,  // 是否用30m过滤
    filter30mMode = 'same',    // 'same' = 同方向才交易
    // 止盈模式
    tpMode = 'none',           // none, trailing, bb30m, ma30m, combo
    // 移动止盈
    trailingActivate = 3.0,
    trailingCallback = 3.0,
    // 30m布林带止盈
    bbTpPct = 90,              // 价格达到布林带X%位置止盈
    // 30m MA48止盈
    ma48TpEnabled = false,
    ma48TpBars = 2,            // 连续N根K线收在MA48另一侧
    // 止损
    stopLossPct = 2.0,
  } = config;

  let position = null;
  let entryPrice = 0, entryTime = null;
  let maxProfitPct = 0;
  let ma48CrossCount = 0;
  let totalPnL = 0, winCount = 0, lossCount = 0;
  let maxWin = 0, maxLoss = 0;
  const trades = [];

  function closePosition(price, reason) {
    const pnl = position === 'long'
      ? (price - entryPrice) / entryPrice * 100
      : (entryPrice - price) / entryPrice * 100;
    totalPnL += pnl;
    if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
    else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
    trades.push({
      entry: entryPrice, exit: price, pnl, type: position,
      entryTime, exitTime: null, reason
    });
    position = null;
    maxProfitPct = 0;
    ma48CrossCount = 0;
    return pnl;
  }

  for (let i = 1; i < df5m.length; i++) {
    const row = df5m[i];
    const ma288 = row.m5_ma288;
    const ma488 = row.m5_ma488;
    const ma48 = row.m5_ma48;
    const o = row.open, c = row.close;
    const slope = row.m5_ma288Slope;
    const volRatio = row.m5_volRatio;

    // 5m趋势
    let trend5m;
    if (ma288 < ma488) trend5m = 'bearish';
    else if (ma288 > ma488) trend5m = 'bullish';
    else continue;

    // 5m斜率过滤
    if (slope5mThreshold > 0 && slope !== null && Math.abs(slope) < slope5mThreshold) continue;

    // 5m成交量过滤
    if (vol5mThreshold > 0 && volRatio !== null && volRatio < vol5mThreshold) continue;

    // 30m过滤
    if (filter30mEnabled) {
      const data30m = get30mAt(row.open_time);
      if (data30m && data30m.trend !== trend5m) continue;
    }

    // === 持仓中的止盈止损 ===
    if (position !== null) {
      const currentPnl = position === 'long'
        ? (c - entryPrice) / entryPrice * 100
        : (entryPrice - c) / entryPrice * 100;
      maxProfitPct = Math.max(maxProfitPct, currentPnl);

      // 止损
      if (currentPnl < -stopLossPct) {
        closePosition(c, 'STOP');
        continue;
      }

      // 移动止盈
      if (tpMode === 'trailing' || tpMode === 'combo') {
        if (maxProfitPct >= trailingActivate) {
          const drawdown = maxProfitPct - currentPnl;
          if (drawdown >= trailingCallback) {
            closePosition(c, 'TRAILING_TP');
            continue;
          }
        }
      }

      // 30m布林带止盈
      if (tpMode === 'bb30m' || tpMode === 'combo') {
        const data30m = get30mAt(row.open_time);
        if (data30m && data30m.bbUpper && data30m.bbLower) {
          const bbRange = data30m.bbUpper - data30m.bbLower;
          if (position === 'long') {
            const pricePos = (c - data30m.bbLower) / bbRange * 100;
            if (pricePos >= bbTpPct) {
              closePosition(c, 'BB_TP');
              continue;
            }
          } else {
            const pricePos = (c - data30m.bbLower) / bbRange * 100;
            if (pricePos <= (100 - bbTpPct)) {
              closePosition(c, 'BB_TP');
              continue;
            }
          }
        }
      }

      // 30m MA48止盈
      if (ma48TpEnabled || tpMode === 'combo') {
        const data30m = get30mAt(row.open_time);
        if (data30m && data30m.ma48) {
          if (position === 'long' && c < data30m.ma48) {
            ma48CrossCount++;
            if (ma48CrossCount >= ma48TpBars) {
              closePosition(c, 'MA48_TP');
              continue;
            }
          } else if (position === 'short' && c > data30m.ma48) {
            ma48CrossCount++;
            if (ma48CrossCount >= ma48TpBars) {
              closePosition(c, 'MA48_TP');
              continue;
            }
          } else {
            ma48CrossCount = 0;
          }
        }
      }
    }

    // === 入场信号 ===
    let isEntry = false;
    let entryDir = '';

    // 5m双均线交叉入场
    if (trend5m === 'bullish') {
      // 开盘低于MA288，收盘高于MA288 → 做多
      if (o < ma288 && c > ma288) {
        isEntry = true;
        entryDir = 'long';
      }
    } else {
      // 开盘高于MA288，收盘低于MA288 → 做空
      if (o > ma288 && c < ma288) {
        isEntry = true;
        entryDir = 'short';
      }
    }

    if (isEntry) {
      // 平掉反向持仓
      if (position !== null && position !== entryDir) {
        closePosition(c, 'REVERSE');
      }

      // 开新仓
      if (position === null) {
        position = entryDir;
        entryPrice = c;
        entryTime = row.open_time;
        maxProfitPct = 0;
        ma48CrossCount = 0;
      }
    }

    // 5m趋势反转平仓
    if (position === 'long' && trend5m === 'bearish') {
      if (o > ma288 && c < ma288) {
        closePosition(c, 'TREND_REV');
      }
    } else if (position === 'short' && trend5m === 'bullish') {
      if (o < ma288 && c > ma288) {
        closePosition(c, 'TREND_REV');
      }
    }
  }

  // 更新exitTime
  for (const t of trades) {
    if (!t.exitTime) t.exitTime = df5m[df5m.length-1].open_time;
  }

  return {
    tradeCount: trades.length,
    winCount, lossCount,
    winRate: trades.length > 0 ? (winCount / trades.length * 100) : 0,
    totalPnL,
    avgPnL: trades.length > 0 ? (totalPnL / trades.length) : 0,
    maxWin, maxLoss,
    profitFactor: maxLoss !== 0 ? (maxWin / Math.abs(maxLoss)) : 0,
    trades
  };
}

// ============================================================
// 测试1: 5m基础策略 (无30m优化)
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试1: 5m基础策略】");
console.log("=".repeat(70));

console.log("\n配置              | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 最大亏");
console.log("-".repeat(90));

const basicConfigs = [
  { label: '5m基础(无过滤)', slope: 0, vol: 0, filter30m: false, tp: 'none' },
  { label: '+slope>2', slope: 2, vol: 0, filter30m: false, tp: 'none' },
  { label: '+slope>5', slope: 5, vol: 0, filter30m: false, tp: 'none' },
  { label: '+vol>0.5', slope: 0, vol: 0.5, filter30m: false, tp: 'none' },
  { label: '+vol>0.8', slope: 0, vol: 0.8, filter30m: false, tp: 'none' },
  { label: '+30m过滤', slope: 0, vol: 0, filter30m: true, tp: 'none' },
  { label: '+slope+vol+30m', slope: 3, vol: 0.6, filter30m: true, tp: 'none' },
];

const basicResults = [];
for (const cfg of basicConfigs) {
  const r = run5mStrategy(df_5m_valid, {
    slope5mThreshold: cfg.slope,
    vol5mThreshold: cfg.vol,
    filter30mEnabled: cfg.filter30m,
    tpMode: cfg.tp,
    stopLossPct: 2.0,
  });
  basicResults.push({ label: cfg.label, ...r });
  console.log(
    `${cfg.label.padEnd(18)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}% | ` +
    `${r.maxWin.toFixed(2).padStart(7)}% | ${r.maxLoss.toFixed(2).padStart(7)}%`
  );
}

// ============================================================
// 测试2: 30m止盈优化
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试2: 30m止盈优化】");
console.log("=".repeat(70));

console.log("\n止盈模式           | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 最大亏");
console.log("-".repeat(90));

const tpConfigs = [
  { label: '无止盈', tp: 'none' },
  { label: '移动(3%+3%)', tp: 'trailing', act: 3, cb: 3 },
  { label: '移动(2%+2%)', tp: 'trailing', act: 2, cb: 2 },
  { label: '移动(5%+3%)', tp: 'trailing', act: 5, cb: 3 },
  { label: '30m布林带(90%)', tp: 'bb30m', bbPct: 90 },
  { label: '30m布林带(95%)', tp: 'bb30m', bbPct: 95 },
  { label: '30m MA48(2根)', tp: 'ma30m', ma48Bars: 2 },
  { label: '30m MA48(3根)', tp: 'ma30m', ma48Bars: 3 },
  { label: '组合(移动+BB)', tp: 'combo', act: 3, cb: 3, bbPct: 90 },
  { label: '组合(移动+MA48)', tp: 'combo', act: 3, cb: 3, ma48Bars: 2 },
];

const tpResults = [];
for (const cfg of tpConfigs) {
  const r = run5mStrategy(df_5m_valid, {
    slope5mThreshold: 0,
    vol5mThreshold: 0,
    filter30mEnabled: false,
    tpMode: cfg.tp,
    trailingActivate: cfg.act || 3,
    trailingCallback: cfg.cb || 3,
    bbTpPct: cfg.bbPct || 90,
    ma48TpBars: cfg.ma48Bars || 2,
    stopLossPct: 2.0,
  });
  tpResults.push({ label: cfg.label, ...r });
  console.log(
    `${cfg.label.padEnd(18)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}% | ` +
    `${r.maxWin.toFixed(2).padStart(7)}% | ${r.maxLoss.toFixed(2).padStart(7)}%`
  );
}

// ============================================================
// 测试3: 移动止盈参数优化
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试3: 移动止盈参数优化】");
console.log("=".repeat(70));

console.log("\n激活+回撤(%) | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 最大亏");
console.log("-".repeat(85));

const trailParams = [
  [1.5, 1.0], [1.5, 1.5], [2.0, 1.0], [2.0, 1.5], [2.0, 2.0],
  [2.5, 1.5], [2.5, 2.0], [2.5, 2.5], [3.0, 2.0], [3.0, 3.0],
  [4.0, 2.0], [4.0, 3.0], [5.0, 3.0], [5.0, 5.0],
];

const trailResults = [];
for (const [act, cb] of trailParams) {
  const r = run5mStrategy(df_5m_valid, {
    slope5mThreshold: 0,
    vol5mThreshold: 0,
    filter30mEnabled: false,
    tpMode: 'trailing',
    trailingActivate: act,
    trailingCallback: cb,
    stopLossPct: 2.0,
  });
  trailResults.push({ label: `${act}+${cb}`, act, cb, ...r });
  console.log(
    `${String(`${act}+${cb}`).padEnd(12)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}% | ` +
    `${r.maxWin.toFixed(2).padStart(7)}% | ${r.maxLoss.toFixed(2).padStart(7)}%`
  );
}

// ============================================================
// 测试4: 最优组合 + 30m过滤 + 成交量
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试4: 最优组合测试】");
console.log("=".repeat(70));

// 找出最佳止盈参数
const bestTrail = trailResults.reduce((a, b) => a.totalPnL > b.totalPnL ? a : b);
console.log(`\n最佳移动止盈: ${bestTrail.label} (${bestTrail.totalPnL.toFixed(2)}%)`);

console.log("\n组合                           | 交易数 | 胜率   | 总收益   | 平均收益 | 最大盈  | 最大亏");
console.log("-".repeat(95));

const comboConfigs = [
  { label: '基准(5m+移动)', slope: 0, vol: 0, f30m: false, act: bestTrail.act, cb: bestTrail.cb },
  { label: '+slope>2', slope: 2, vol: 0, f30m: false, act: bestTrail.act, cb: bestTrail.cb },
  { label: '+vol>0.5', slope: 0, vol: 0.5, f30m: false, act: bestTrail.act, cb: bestTrail.cb },
  { label: '+30m过滤', slope: 0, vol: 0, f30m: true, act: bestTrail.act, cb: bestTrail.cb },
  { label: '+slope+vol', slope: 2, vol: 0.5, f30m: false, act: bestTrail.act, cb: bestTrail.cb },
  { label: '+slope+vol+30m', slope: 2, vol: 0.5, f30m: true, act: bestTrail.act, cb: bestTrail.cb },
  { label: '全配置', slope: 3, vol: 0.6, f30m: true, act: bestTrail.act, cb: bestTrail.cb },
];

const comboResults = [];
for (const cfg of comboConfigs) {
  const r = run5mStrategy(df_5m_valid, {
    slope5mThreshold: cfg.slope,
    vol5mThreshold: cfg.vol,
    filter30mEnabled: cfg.f30m,
    tpMode: 'trailing',
    trailingActivate: cfg.act,
    trailingCallback: cfg.cb,
    stopLossPct: 2.0,
  });
  comboResults.push({ label: cfg.label, ...r });
  console.log(
    `${cfg.label.padEnd(29)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}% | ` +
    `${r.maxWin.toFixed(2).padStart(7)}% | ${r.maxLoss.toFixed(2).padStart(7)}%`
  );
}

// ============================================================
// 样本内/样本外检验
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【样本内/样本外检验】");
console.log("=".repeat(70));

const splitIdx = Math.floor(df_5m_valid.length * 0.7);
const train5m = df_5m_valid.slice(0, splitIdx);
const test5m = df_5m_valid.slice(splitIdx);

console.log(`\n训练集: ${train5m.length} 根 (${train5m[0].open_time.toISOString().slice(0,10)} ~ ${train5m[train5m.length-1].open_time.toISOString().slice(0,10)})`);
console.log(`测试集: ${test5m.length} 根 (${test5m[0].open_time.toISOString().slice(0,10)} ~ ${test5m[test5m.length-1].open_time.toISOString().slice(0,10)})`);

const bestConfig = {
  slope5mThreshold: 0,
  vol5mThreshold: 0,
  filter30mEnabled: false,
  tpMode: 'trailing',
  trailingActivate: bestTrail.act,
  trailingCallback: bestTrail.cb,
  stopLossPct: 2.0,
};

const trainR = run5mStrategy(train5m, bestConfig);
const testR = run5mStrategy(test5m, bestConfig);
const fullR = run5mStrategy(df_5m_valid, bestConfig);

console.log(`\n指标          | 训练集(70%) | 测试集(30%) | 全量数据`);
console.log(`-`.repeat(60));
console.log(`交易数        | ${String(trainR.tradeCount).padStart(11)} | ${String(testR.tradeCount).padStart(11)} | ${fullR.tradeCount}`);
console.log(`胜率          | ${trainR.winRate.toFixed(1).padStart(10)}% | ${testR.winRate.toFixed(1).padStart(10)}% | ${fullR.winRate.toFixed(1)}%`);
console.log(`总收益        | ${(trainR.totalPnL >= 0 ? '+' : '') + trainR.totalPnL.toFixed(2).padStart(9)}% | ${(testR.totalPnL >= 0 ? '+' : '') + testR.totalPnL.toFixed(2).padStart(9)}% | ${fullR.totalPnL.toFixed(2)}%`);
console.log(`平均收益      | ${(trainR.avgPnL >= 0 ? '+' : '') + trainR.avgPnL.toFixed(3).padStart(9)}% | ${(testR.avgPnL >= 0 ? '+' : '') + testR.avgPnL.toFixed(3).padStart(9)}% | ${fullR.avgPnL.toFixed(3)}%`);

const decay = ((testR.totalPnL - trainR.totalPnL) / Math.abs(trainR.totalPnL) * 100);
console.log(`\n收益衰减: ${decay.toFixed(1)}% ${decay < -50 ? '⚠ 严重' : decay < -30 ? '⚠ 中等' : '✅ 可接受'}`);

// ============================================================
// 最优配置详细分析
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【最优配置详细分析】");
console.log("=".repeat(70));

const optimal = comboResults[0]; // 基准(5m+移动)

console.log(`\n配置: 5m双均线 + 移动止盈(${bestTrail.label})`);
console.log(`\n统计:`);
console.log(`  交易数: ${optimal.tradeCount}`);
console.log(`  胜率: ${optimal.winRate.toFixed(1)}%`);
console.log(`  总收益: ${optimal.totalPnL.toFixed(2)}%`);
console.log(`  平均收益: ${optimal.avgPnL.toFixed(3)}%`);
console.log(`  最大盈利: ${optimal.maxWin.toFixed(2)}%`);
console.log(`  最大亏损: ${optimal.maxLoss.toFixed(2)}%`);
console.log(`  盈亏比: ${optimal.profitFactor.toFixed(2)}`);

// 出场类型统计
console.log("\n--- 出场类型统计 ---");
const typeCounts = {};
for (const t of optimal.trades) {
  typeCounts[t.reason] = (typeCounts[t.reason] || 0) + 1;
}
for (const [type, count] of Object.entries(typeCounts).sort((a,b) => b[1]-a[1])) {
  const avgPnl = optimal.trades.filter(t => t.reason === type).reduce((s,t) => s+t.pnl, 0) / count;
  console.log(`  ${type.padEnd(15)}: ${count} 次, 平均: ${avgPnl >= 0 ? '+' : ''}${avgPnl.toFixed(3)}%`);
}

// 最近交易
console.log("\n--- 最近15笔交易 ---");
for (const t of optimal.trades.slice(-15)) {
  const duration = (t.exitTime - t.entryTime) / 3600000;
  const pnlSign = t.pnl >= 0 ? '+' : '';
  console.log(`  ${t.type.padEnd(5)} ${t.entry.toFixed(2)} → ${t.exit.toFixed(2)} | ${pnlSign}${t.pnl.toFixed(4)}% | ${t.reason} | ${duration.toFixed(1)}h`);
}

console.log("\n第八次分析完成！");
