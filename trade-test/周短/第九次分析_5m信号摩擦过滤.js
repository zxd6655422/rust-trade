/**
 * 第九次分析: 5m策略信号摩擦过滤
 *
 * 给5m策略也加上:
 * 1. MA288斜率过滤
 * 2. 布林带带宽过滤
 * 3. 成交量过滤
 * 4. 30m趋势过滤
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
console.log("第九次分析: 5m策略信号摩擦过滤");
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

// 30m趋势索引
function build30mMap(df30) {
  const map = new Map();
  for (const r of df30) {
    map.set(r.open_time.getTime(), {
      trend: r.m30_ma288 > r.m30_ma488 ? 'bullish' : 'bearish',
      ma288: r.m30_ma288,
      ma488: r.m30_ma488
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
// 策略回测
// ============================================================
function run5mStrategy(df5m, config) {
  const {
    slope5mThreshold = 0,
    bbw5mThreshold = 0,
    vol5mThreshold = 0,
    filter30mEnabled = false,
    trailingActivate = 2.0,
    trailingCallback = 1.5,
    stopLossPct = 2.0,
  } = config;

  let position = null;
  let entryPrice = 0, entryTime = null;
  let maxProfitPct = 0;
  let totalPnL = 0, winCount = 0, lossCount = 0;
  let maxWin = 0, maxLoss = 0;
  const trades = [];

  for (let i = 1; i < df5m.length; i++) {
    const row = df5m[i];
    const ma288 = row.m5_ma288;
    const ma488 = row.m5_ma488;
    const o = row.open, c = row.close;
    const slope = row.m5_ma288Slope;
    const bbw = row.m5_bbWidth;
    const volRatio = row.m5_volRatio;

    let trend5m;
    if (ma288 < ma488) trend5m = 'bearish';
    else if (ma288 > ma488) trend5m = 'bullish';
    else continue;

    // === 信号摩擦过滤 ===
    // 1. MA288斜率过滤
    if (slope5mThreshold > 0 && slope !== null && Math.abs(slope) < slope5mThreshold) continue;

    // 2. 布林带带宽过滤
    if (bbw5mThreshold > 0 && bbw !== null && bbw < bbw5mThreshold) continue;

    // 3. 成交量过滤
    if (vol5mThreshold > 0 && volRatio !== null && volRatio < vol5mThreshold) continue;

    // 4. 30m趋势过滤
    if (filter30mEnabled) {
      const data30m = get30mAt(row.open_time);
      if (data30m && data30m.trend !== trend5m) continue;
    }

    // === 持仓止盈止损 ===
    if (position !== null) {
      const currentPnl = position === 'long'
        ? (c - entryPrice) / entryPrice * 100
        : (entryPrice - c) / entryPrice * 100;
      maxProfitPct = Math.max(maxProfitPct, currentPnl);

      // 止损
      if (currentPnl < -stopLossPct) {
        totalPnL += currentPnl;
        lossCount++; maxLoss = Math.min(maxLoss, currentPnl);
        trades.push({ pnl: currentPnl, reason: 'STOP' });
        position = null;
        continue;
      }

      // 移动止盈
      if (maxProfitPct >= trailingActivate) {
        const drawdown = maxProfitPct - currentPnl;
        if (drawdown >= trailingCallback) {
          totalPnL += currentPnl;
          if (currentPnl > 0) { winCount++; maxWin = Math.max(maxWin, currentPnl); }
          else { lossCount++; maxLoss = Math.min(maxLoss, currentPnl); }
          trades.push({ pnl: currentPnl, reason: 'TRAILING_TP' });
          position = null;
          continue;
        }
      }
    }

    // === 入场信号 ===
    let isEntry = false;
    let entryDir = '';

    if (trend5m === 'bullish' && o < ma288 && c > ma288) {
      isEntry = true;
      entryDir = 'long';
    } else if (trend5m === 'bearish' && o > ma288 && c < ma288) {
      isEntry = true;
      entryDir = 'short';
    }

    if (isEntry) {
      if (position !== null && position !== entryDir) {
        const pnl = position === 'long'
          ? (c - entryPrice) / entryPrice * 100
          : (entryPrice - c) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
        else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ pnl, reason: 'REVERSE' });
      }

      position = entryDir;
      entryPrice = c;
      entryTime = row.open_time;
      maxProfitPct = 0;
    }

    // 趋势反转平仓
    if (position === 'long' && trend5m === 'bearish' && o > ma288 && c < ma288) {
      const pnl = (c - entryPrice) / entryPrice * 100;
      totalPnL += pnl;
      if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
      else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
      trades.push({ pnl, reason: 'TREND_REV' });
      position = null;
    } else if (position === 'short' && trend5m === 'bullish' && o < ma288 && c > ma288) {
      const pnl = (entryPrice - c) / entryPrice * 100;
      totalPnL += pnl;
      if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
      else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
      trades.push({ pnl, reason: 'TREND_REV' });
      position = null;
    }
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
// 测试1: 5m斜率阈值
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试1: 5m MA288斜率阈值】");
console.log("=".repeat(70));

console.log("\n阈值(bps) | 交易数 | 胜率   | 总收益   | 平均收益 | 信号减少");
console.log("-".repeat(75));

const baseResult = run5mStrategy(df_5m_valid, { filter30mEnabled: true });
const slopeResults = [];

for (const slope of [0, 1, 2, 3, 5, 8, 10]) {
  const r = run5mStrategy(df_5m_valid, { slope5mThreshold: slope, filter30mEnabled: true });
  slopeResults.push({ slope, ...r });
  const reduction = ((1 - r.tradeCount / baseResult.tradeCount) * 100).toFixed(1);
  console.log(
    `${String(slope).padStart(9)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}% | ` +
    `-${reduction}%`
  );
}

// ============================================================
// 测试2: 5m布林带带宽
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试2: 5m布林带带宽阈值】");
console.log("=".repeat(70));

console.log("\n阈值(%)  | 交易数 | 胜率   | 总收益   | 平均收益 | 信号减少");
console.log("-".repeat(75));

const bbwResults = [];
for (const bbw of [0, 0.5, 1.0, 1.5, 2.0, 2.5]) {
  const r = run5mStrategy(df_5m_valid, { bbw5mThreshold: bbw, filter30mEnabled: true });
  bbwResults.push({ bbw, ...r });
  const reduction = ((1 - r.tradeCount / baseResult.tradeCount) * 100).toFixed(1);
  console.log(
    `${String(bbw).padStart(8)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}% | ` +
    `-${reduction}%`
  );
}

// ============================================================
// 测试3: 5m成交量
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试3: 5m成交量阈值】");
console.log("=".repeat(70));

console.log("\n阈值     | 交易数 | 胜率   | 总收益   | 平均收益 | 信号减少");
console.log("-".repeat(75));

const volResults = [];
for (const vol of [0, 0.5, 0.8, 1.0, 1.2, 1.5]) {
  const r = run5mStrategy(df_5m_valid, { vol5mThreshold: vol, filter30mEnabled: true });
  volResults.push({ vol, ...r });
  const reduction = ((1 - r.tradeCount / baseResult.tradeCount) * 100).toFixed(1);
  console.log(
    `${String(vol > 0 ? '>' + vol : '无').padStart(8)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}% | ` +
    `-${reduction}%`
  );
}

// ============================================================
// 测试4: 双重过滤组合
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试4: 双重过滤组合】");
console.log("=".repeat(70));

console.log("\n组合                  | 交易数 | 胜率   | 总收益   | 平均收益 | 信号减少");
console.log("-".repeat(80));

const comboConfigs = [
  { label: '基准(仅30m过滤)', slope: 0, bbw: 0, vol: 0 },
  { label: '+slope>2', slope: 2, bbw: 0, vol: 0 },
  { label: '+bbw>1', slope: 0, bbw: 1.0, vol: 0 },
  { label: '+vol>0.5', slope: 0, bbw: 0, vol: 0.5 },
  { label: '+slope+bbw', slope: 2, bbw: 1.0, vol: 0 },
  { label: '+slope+vol', slope: 2, bbw: 0, vol: 0.5 },
  { label: '+bbw+vol', slope: 0, bbw: 1.0, vol: 0.5 },
  { label: '三重过滤', slope: 2, bbw: 1.0, vol: 0.5 },
  { label: '三重(更严)', slope: 3, bbw: 1.5, vol: 0.8 },
];

const comboResults = [];
for (const cfg of comboConfigs) {
  const r = run5mStrategy(df_5m_valid, {
    slope5mThreshold: cfg.slope,
    bbw5mThreshold: cfg.bbw,
    vol5mThreshold: cfg.vol,
    filter30mEnabled: true
  });
  comboResults.push({ label: cfg.label, ...r });
  const reduction = ((1 - r.tradeCount / baseResult.tradeCount) * 100).toFixed(1);
  console.log(
    `${cfg.label.padEnd(21)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}% | ` +
    `-${reduction}%`
  );
}

// ============================================================
// 测试5: 移动止盈参数优化 (带信号摩擦过滤)
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试5: 移动止盈参数优化 (带信号摩擦过滤)】");
console.log("=".repeat(70));

console.log("\n激活+回撤(%) | 交易数 | 胜率   | 总收益   | 平均收益");
console.log("-".repeat(65));

const trailParams = [
  [1.5, 1.0], [1.5, 1.5], [2.0, 1.0], [2.0, 1.5], [2.0, 2.0],
  [2.5, 1.5], [2.5, 2.0], [3.0, 2.0], [3.0, 3.0], [4.0, 3.0],
];

const trailResults = [];
for (const [act, cb] of trailParams) {
  const r = run5mStrategy(df_5m_valid, {
    slope5mThreshold: 2,
    bbw5mThreshold: 1.0,
    vol5mThreshold: 0,
    filter30mEnabled: true,
    trailingActivate: act,
    trailingCallback: cb,
  });
  trailResults.push({ label: `${act}+${cb}`, act, cb, ...r });
  console.log(
    `${String(`${act}+${cb}`).padStart(12)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ` +
    `${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}%`
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

const bestTrail = trailResults.reduce((a, b) => a.totalPnL > b.totalPnL ? a : b);
console.log(`\n最佳止盈: ${bestTrail.label}`);

const bestConfig = {
  slope5mThreshold: 2,
  bbw5mThreshold: 1.0,
  vol5mThreshold: 0,
  filter30mEnabled: true,
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

const decay = trainR.totalPnL !== 0 ? ((testR.totalPnL - trainR.totalPnL) / Math.abs(trainR.totalPnL) * 100) : 0;
console.log(`\n收益衰减: ${decay.toFixed(1)}% ${decay < -50 ? '⚠ 严重' : decay < -30 ? '⚠ 中等' : '✅ 可接受'}`);

// ============================================================
// 最优配置详细分析
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【最优配置详细分析】");
console.log("=".repeat(70));

console.log(`\n配置: 5m(slope>2 + bbw>1) + 30m过滤 + 移动止盈(${bestTrail.label})`);
console.log(`\n统计:`);
console.log(`  交易数: ${fullR.tradeCount}`);
console.log(`  胜率: ${fullR.winRate.toFixed(1)}%`);
console.log(`  总收益: ${fullR.totalPnL.toFixed(2)}%`);
console.log(`  平均收益: ${fullR.avgPnL.toFixed(3)}%`);
console.log(`  最大盈利: ${fullR.maxWin.toFixed(2)}%`);
console.log(`  最大亏损: ${fullR.maxLoss.toFixed(2)}%`);

// 出场类型
console.log("\n--- 出场类型 ---");
const typeCounts = {};
for (const t of fullR.trades) {
  typeCounts[t.reason] = (typeCounts[t.reason] || 0) + 1;
}
for (const [type, count] of Object.entries(typeCounts).sort((a,b) => b[1]-a[1])) {
  const avgPnl = fullR.trades.filter(t => t.reason === type).reduce((s,t) => s+t.pnl, 0) / count;
  console.log(`  ${type.padEnd(15)}: ${count} 次, 平均: ${avgPnl >= 0 ? '+' : ''}${avgPnl.toFixed(3)}%`);
}

// ============================================================
// 最终对比
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【最终对比: 30m主策略 vs 5m主策略(优化后)】");
console.log("=".repeat(70));

console.log(`
维度          | 30m主策略      | 5m主策略(优化后)
--------------|----------------|------------------
交易频率      | 52笔/20个月    | ${fullR.tradeCount}笔/6个月
胜率          | 28.8%          | ${fullR.winRate.toFixed(1)}%
总收益        | +54.39%        | ${fullR.totalPnL >= 0 ? '+' : ''}${fullR.totalPnL.toFixed(2)}%
平均收益      | +1.046%/笔     | ${fullR.avgPnL >= 0 ? '+' : ''}${fullR.avgPnL.toFixed(3)}%/笔
盈亏比        | 3.92           | ${fullR.profitFactor.toFixed(2)}
样本外衰减    | -30.6%         | ${decay.toFixed(1)}%
`);

console.log("第九次分析完成！");
