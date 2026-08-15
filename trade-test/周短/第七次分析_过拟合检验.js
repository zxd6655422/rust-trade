/**
 * 第七次分析: 过拟合检验
 *
 * 方法:
 * 1. 样本内/样本外测试 (前70%训练，后30%测试)
 * 2. 不同时间段稳定性测试
 * 3. 参数敏感性分析
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
console.log("第七次分析: 过拟合检验");
console.log("=".repeat(70));

const df_30m = loadCSV('kline_30m_202607222207.csv', 'open_time');
const df_5m = loadCSV('kline_5m_202607222208.csv', 'open_time');

// ============================================================
// 计算指标
// ============================================================
function addIndicators(df) {
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
  const bbWidth = new Array(df.length).fill(null);
  for (let i = 99; i < df.length; i++) {
    let sumSq = 0;
    for (let j = i - 99; j <= i; j++) sumSq += (closes[j] - bbMid[i]) ** 2;
    const std = Math.sqrt(sumSq / 100);
    const bbUpper = bbMid[i] + 2 * std;
    const bbLower = bbMid[i] - 2 * std;
    bbWidth[i] = (bbUpper - bbLower) / bbMid[i] * 100;
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

  const volMA = new Array(df.length).fill(null);
  const volRatio = new Array(df.length).fill(null);
  for (let i = 19; i < df.length; i++) {
    let sum = 0;
    for (let j = i - 19; j <= i; j++) sum += volumes[j];
    volMA[i] = sum / 20;
    if (volMA[i] > 0) volRatio[i] = volumes[i] / volMA[i];
  }

  for (let i = 0; i < df.length; i++) {
    df[i].ma48 = ma48[i];
    df[i].ma288 = ma288[i];
    df[i].ma488 = ma488[i];
    df[i].bbWidth = bbWidth[i];
    df[i].ma288Slope = ma288Slope[i];
    df[i].priceDevMa488 = priceDevMa488[i];
    df[i].volRatio = volRatio[i];
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
      spread
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
// 策略回测函数
// ============================================================
function runStrategy(df30, config) {
  const {
    slopeThreshold = 5,
    bbWidthThreshold = 2.0,
    strong5mThreshold = 1.0,
    priceDevThreshold = 5.0,
    stopLossPct = 2.0,
    volFilterThreshold = 0.6,
    trailingActivatePct = 3.0,
    trailingCallbackPct = 3.0,
  } = config;

  let position = null;
  let entryPrice = 0, entryTime = null;
  let maxProfitPct = 0;
  let totalPnL = 0, winCount = 0, lossCount = 0;
  let maxWin = 0, maxLoss = 0;
  const trades = [];

  for (let i = 1; i < df30.length; i++) {
    const row = df30[i];
    const {ma288, ma488, open: o, close: c, ma288Slope: slope, bbWidth: bbw, priceDevMa488: dev, volRatio} = row;

    let trend;
    if (ma288 < ma488) trend = 'bearish';
    else if (ma288 > ma488) trend = 'bullish';
    else continue;

    if (slopeThreshold > 0 && slope !== null && Math.abs(slope) < slopeThreshold) continue;
    if (bbWidthThreshold > 0 && bbw !== null && bbw < bbWidthThreshold) continue;

    const data5m = get5mTrendAt(row.open_time);
    const trend5m = data5m ? data5m.trend : null;
    const spread5m = data5m ? data5m.spread : 0;

    if (trend5m !== null && trend5m !== trend) {
      if (dev !== null && Math.abs(dev) > priceDevThreshold) continue;
      if (Math.abs(spread5m) > strong5mThreshold) continue;
    }

    if (dev !== null) {
      if (trend === 'bullish' && dev < -priceDevThreshold && position === 'long') {
        const pnl = (c - entryPrice) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); } else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ pnl, reason: 'FLIP' });
        position = null;
        continue;
      }
      if (trend === 'bearish' && dev > priceDevThreshold && position === 'short') {
        const pnl = (entryPrice - c) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); } else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ pnl, reason: 'FLIP' });
        position = null;
        continue;
      }
    }

    if (position === 'long') {
      const currentPnl = (c - entryPrice) / entryPrice * 100;
      maxProfitPct = Math.max(maxProfitPct, currentPnl);
      if (currentPnl < -stopLossPct) {
        totalPnL += currentPnl;
        lossCount++; maxLoss = Math.min(maxLoss, currentPnl);
        trades.push({ pnl: currentPnl, reason: 'STOP' });
        position = null;
        continue;
      }
      if (maxProfitPct >= trailingActivatePct && (maxProfitPct - currentPnl) >= trailingCallbackPct) {
        totalPnL += currentPnl;
        if (currentPnl > 0) { winCount++; maxWin = Math.max(maxWin, currentPnl); } else { lossCount++; maxLoss = Math.min(maxLoss, currentPnl); }
        trades.push({ pnl: currentPnl, reason: 'TRAILING_TP' });
        position = null;
        continue;
      }
    } else if (position === 'short') {
      const currentPnl = (entryPrice - c) / entryPrice * 100;
      maxProfitPct = Math.max(maxProfitPct, currentPnl);
      if (currentPnl < -stopLossPct) {
        totalPnL += currentPnl;
        lossCount++; maxLoss = Math.min(maxLoss, currentPnl);
        trades.push({ pnl: currentPnl, reason: 'STOP' });
        position = null;
        continue;
      }
      if (maxProfitPct >= trailingActivatePct && (maxProfitPct - currentPnl) >= trailingCallbackPct) {
        totalPnL += currentPnl;
        if (currentPnl > 0) { winCount++; maxWin = Math.max(maxWin, currentPnl); } else { lossCount++; maxLoss = Math.min(maxLoss, currentPnl); }
        trades.push({ pnl: currentPnl, reason: 'TRAILING_TP' });
        position = null;
        continue;
      }
    }

    let isEntrySignal = false;
    let entryType = '';
    if (trend === 'bearish' && o > ma288 && c < ma288) { isEntrySignal = true; entryType = 'SHORT'; }
    else if (trend === 'bullish' && o < ma288 && c > ma288) { isEntrySignal = true; entryType = 'LONG'; }

    if (isEntrySignal) {
      if (volFilterThreshold > 0 && volRatio !== null && volRatio < volFilterThreshold) continue;

      if (position === 'long' && entryType === 'SHORT') {
        const pnl = (c - entryPrice) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); } else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ pnl, reason: 'REVERSE' });
      } else if (position === 'short' && entryType === 'LONG') {
        const pnl = (entryPrice - c) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); } else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ pnl, reason: 'REVERSE' });
      }

      position = entryType === 'LONG' ? 'long' : 'short';
      entryPrice = c;
      entryTime = row.open_time;
      maxProfitPct = 0;
    }

    if (trend === 'bearish' && o < ma288 && c > ma288 && position === 'short') {
      const pnl = (entryPrice - c) / entryPrice * 100;
      totalPnL += pnl;
      if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); } else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
      trades.push({ pnl, reason: 'COVER' });
      position = null;
    } else if (trend === 'bullish' && o > ma288 && c < ma288 && position === 'long') {
      const pnl = (c - entryPrice) / entryPrice * 100;
      totalPnL += pnl;
      if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); } else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
      trades.push({ pnl, reason: 'STOP' });
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
    trades
  };
}

// ============================================================
// 测试1: 样本内/样本外
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试1: 样本内/样本外检验】");
console.log("=".repeat(70));

const splitIndex = Math.floor(df_30m_valid.length * 0.7);
const trainData = df_30m_valid.slice(0, splitIndex);
const testData = df_30m_valid.slice(splitIndex);

console.log(`\n数据划分:`);
console.log(`  训练集: ${trainData.length} 根K线 (${trainData[0].open_time.toISOString().slice(0,10)} ~ ${trainData[trainData.length-1].open_time.toISOString().slice(0,10)})`);
console.log(`  测试集: ${testData.length} 根K线 (${testData[0].open_time.toISOString().slice(0,10)} ~ ${testData[testData.length-1].open_time.toISOString().slice(0,10)})`);

const config = {
  slopeThreshold: 5,
  bbWidthThreshold: 2.0,
  strong5mThreshold: 1.0,
  priceDevThreshold: 5.0,
  stopLossPct: 2.0,
  volFilterThreshold: 0.6,
  trailingActivatePct: 3.0,
  trailingCallbackPct: 3.0,
};

const trainResult = runStrategy(trainData, config);
const testResult = runStrategy(testData, config);
const fullResult = runStrategy(df_30m_valid, config);

console.log(`\n回测结果:`);
console.log(`\n指标          | 训练集(70%) | 测试集(30%) | 全量数据`);
console.log(`-`.repeat(60));
console.log(`交易数        | ${String(trainResult.tradeCount).padStart(11)} | ${String(testResult.tradeCount).padStart(11)} | ${fullResult.tradeCount}`);
console.log(`胜率          | ${(trainResult.winRate).toFixed(1).padStart(10)}% | ${(testResult.winRate).toFixed(1).padStart(10)}% | ${fullResult.winRate.toFixed(1)}%`);
console.log(`总收益        | ${(trainResult.totalPnL >= 0 ? '+' : '') + trainResult.totalPnL.toFixed(2).padStart(9)}% | ${(testResult.totalPnL >= 0 ? '+' : '') + testResult.totalPnL.toFixed(2).padStart(9)}% | ${fullResult.totalPnL.toFixed(2)}%`);
console.log(`平均收益      | ${(trainResult.avgPnL >= 0 ? '+' : '') + trainResult.avgPnL.toFixed(3).padStart(9)}% | ${(testResult.avgPnL >= 0 ? '+' : '') + testResult.avgPnL.toFixed(3).padStart(9)}% | ${fullResult.avgPnL.toFixed(3)}%`);
console.log(`最大盈利      | ${trainResult.maxWin.toFixed(2).padStart(10)}% | ${testResult.maxWin.toFixed(2).padStart(10)}% | ${fullResult.maxWin.toFixed(2)}%`);
console.log(`最大亏损      | ${trainResult.maxLoss.toFixed(2).padStart(10)}% | ${testResult.maxLoss.toFixed(2).padStart(10)}% | ${fullResult.maxLoss.toFixed(2)}%`);

// 计算衰减率
const returnDecay = ((testResult.totalPnL - trainResult.totalPnL) / Math.abs(trainResult.totalPnL) * 100);
const winRateDecay = testResult.winRate - trainResult.winRate;

console.log(`\n过拟合指标:`);
console.log(`  收益衰减: ${returnDecay.toFixed(1)}% ${returnDecay < -50 ? '⚠ 严重衰减' : returnDecay < -30 ? '⚠ 中等衰减' : '✅ 可接受'}`);
console.log(`  胜率变化: ${winRateDecay >= 0 ? '+' : ''}${winRateDecay.toFixed(1)}% ${Math.abs(winRateDecay) > 5 ? '⚠ 变化较大' : '✅ 稳定'}`);

// ============================================================
// 测试2: 不同时间段稳定性
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试2: 不同时间段稳定性】");
console.log("=".repeat(70));

// 将数据分成4段
const quarter = Math.floor(df_30m_valid.length / 4);
const quarters = [
  { label: 'Q1 (最早25%)', data: df_30m_valid.slice(0, quarter) },
  { label: 'Q2 (25-50%)', data: df_30m_valid.slice(quarter, quarter * 2) },
  { label: 'Q3 (50-75%)', data: df_30m_valid.slice(quarter * 2, quarter * 3) },
  { label: 'Q4 (最近25%)', data: df_30m_valid.slice(quarter * 3) },
];

console.log(`\n时间段           | 交易数 | 胜率   | 总收益   | 平均收益`);
console.log(`-`.repeat(70));

const quarterResults = [];
for (const q of quarters) {
  const r = runStrategy(q.data, config);
  quarterResults.push({ label: q.label, ...r });
  console.log(`${q.label.padEnd(16)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}%`);
}

// 计算稳定性
const returns = quarterResults.map(r => r.totalPnL);
const avgReturn = returns.reduce((a, b) => a + b, 0) / returns.length;
const stdReturn = Math.sqrt(returns.reduce((sum, r) => sum + (r - avgReturn) ** 2, 0) / returns.length);
const cv = (stdReturn / Math.abs(avgReturn)) * 100; // 变异系数

console.log(`\n稳定性分析:`);
console.log(`  平均收益: ${avgReturn.toFixed(2)}%`);
console.log(`  标准差: ${stdReturn.toFixed(2)}%`);
console.log(`  变异系数: ${cv.toFixed(1)}% ${cv > 100 ? '⚠ 不稳定' : cv > 50 ? '⚠ 较不稳定' : '✅ 稳定'}`);

// ============================================================
// 测试3: 参数敏感性
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【测试3: 参数敏感性分析】");
console.log("=".repeat(70));

// 测试slope参数
console.log("\n--- MA288斜率阈值敏感性 ---");
console.log(`阈值(bps) | 交易数 | 胜率   | 总收益   | 平均收益`);
console.log(`-`.repeat(60));

for (const slope of [3, 4, 5, 6, 7]) {
  const r = runStrategy(df_30m_valid, { ...config, slopeThreshold: slope });
  console.log(`${String(slope).padStart(9)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}%`);
}

// 测试成交量阈值
console.log("\n--- 成交量阈值敏感性 ---");
console.log(`阈值     | 交易数 | 胜率   | 总收益   | 平均收益`);
console.log(`-`.repeat(60));

for (const vol of [0.4, 0.5, 0.6, 0.7, 0.8]) {
  const r = runStrategy(df_30m_valid, { ...config, volFilterThreshold: vol });
  console.log(`${String('>' + vol).padStart(8)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}%`);
}

// 测试trailing参数
console.log("\n--- 移动止盈参数敏感性 ---");
console.log(`激活+回撤(%) | 交易数 | 胜率   | 总收益   | 平均收益`);
console.log(`-`.repeat(60));

for (const [act, cb] of [[2,2],[2.5,2.5],[3,3],[3.5,3.5],[4,4]]) {
  const r = runStrategy(df_30m_valid, { ...config, trailingActivatePct: act, trailingCallbackPct: cb });
  console.log(`${String(`${act}+${cb}`).padStart(12)} | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ${(r.totalPnL >= 0 ? '+' : '') + r.totalPnL.toFixed(2).padStart(7)}% | ${(r.avgPnL >= 0 ? '+' : '') + r.avgPnL.toFixed(3).padStart(7)}%`);
}

// ============================================================
// 过拟合综合评估
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【过拟合综合评估】");
console.log("=".repeat(70));

console.log(`
=== 过拟合风险评估 ===

1. 样本内/样本外测试:
   训练集收益: ${trainResult.totalPnL.toFixed(2)}%
   测试集收益: ${testResult.totalPnL.toFixed(2)}%
   收益衰减: ${returnDecay.toFixed(1)}%
   评估: ${returnDecay < -50 ? '⚠ 过拟合风险高' : returnDecay < -30 ? '⚠ 过拟合风险中等' : '✅ 过拟合风险较低'}

2. 时间段稳定性:
   变异系数: ${cv.toFixed(1)}%
   评估: ${cv > 100 ? '⚠ 策略不稳定' : cv > 50 ? '⚠ 策略较不稳定' : '✅ 策略较稳定'}

3. 参数敏感性:
   如果参数小幅变化导致收益大幅波动 → 过拟合风险高
   如果参数变化收益相对稳定 → 过拟合风险低

=== 建议 ===

1. 纸上交易: 先用小资金或模拟盘验证1-2周
2. 样本外测试: 用最近1个月数据做样本外验证
3. 多标的测试: 在其他币种(ETH, SOL等)上测试
4. 参数稳健性: 选择参数变化时收益波动小的区间
5. 定期复盘: 每周检查策略表现，及时调整
`);

console.log("第七次分析完成！");
