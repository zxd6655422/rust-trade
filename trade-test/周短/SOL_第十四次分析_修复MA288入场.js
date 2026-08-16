/**
 * SOL 第十四次分析: 修复MA288入场
 *
 * 问题: 策略配置 entry_timeframe="30m"，但系统用5m数据计算MA288
 * 修复: 正确使用30m K线数据计算MA288/MA488，5m K线用于入场判断
 *
 * 策略逻辑:
 * 1. 用30m数据计算MA288/MA488，判断趋势方向
 * 2. 用5m数据判断入场: O > MA288 && C < MA288 (做空)
 * 3. 30m和5m扩散过滤
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
console.log("SOL 第十四次分析: 修复MA288入场");
console.log("=".repeat(70));
console.log("\n修复说明:");
console.log("  - MA288/MA488 使用30m数据计算 (正确)");
console.log("  - 入场判断使用5m数据");
console.log("  - 趋势方向基于30m MA");

const df_5m = loadCSV('../kline_5m_202608010054_SOLUSDT.csv', 'open_time');
const df_30m = loadCSV('../kline_30m_202608010054_SOLUSDT.csv', 'open_time');

console.log(`\n数据加载完成:`);
console.log(`  30m K线: ${df_30m.length} 根`);
console.log(`  5m K线: ${df_5m.length} 根`);

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

  // 扩散
  const spread = new Array(df.length).fill(null);
  const isExpanding = new Array(df.length).fill(null);

  for (let i = 0; i < df.length; i++) {
    if (ma288[i] !== null && ma488[i] !== null) {
      spread[i] = ma288[i] - ma488[i];
    }
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

  return df;
}

// ============================================================
// 计算5m指标 (仅用于扩散判断)
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
    if (ma288[i] !== null && ma488[i] !== null) {
      spread[i] = ma288[i] - ma488[i];
    }
  }

  for (let i = 5; i < df.length; i++) {
    if (spread[i] !== null && spread[i - 5] !== null) {
      isExpanding[i] = Math.abs(spread[i]) > Math.abs(spread[i - 5]);
    }
  }

  for (let i = 0; i < df.length; i++) {
    df[i].ma288_5m = ma288[i];
    df[i].ma488_5m = ma488[i];
    df[i].spread_5m = spread[i];
    df[i].isExpanding_5m = isExpanding[i];
  }

  return df;
}

console.log("\n计算30m指标...");
add30mIndicators(df_30m);
console.log("计算5m指标...");
add5mIndicators(df_5m);

// 构建30m数据查找表 (按时间戳)
const map30m = new Map();
for (const r of df_30m) {
  if (r.ma288_30m !== null) {
    map30m.set(r.open_time.getTime(), r);
  }
}

// 获取某时间对应的30m数据
function get30mAt(time) {
  const t = time.getTime();
  // 找到包含此时间的30m K线 (此时间在K线开始时间之后)
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

// 构建5m数据查找表
const map5m = new Map();
for (const r of df_5m) {
  if (r.isExpanding_5m !== null) {
    map5m.set(r.open_time.getTime(), r);
  }
}

function get5mAt(time) {
  const t = time.getTime();
  let best = null;
  let bestDiff = Infinity;
  for (const [ts, data] of map5m) {
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
function runStrategy(config) {
  const {
    useHardStop = true, hardStopPct = 2.5,
    tpMode = 'trailing', trailingActivate = 6.0, trailingCallback = 2.0,
    use30mExpanding = true, use5mExpanding = true,
  } = config;

  let position = null; // 'long' or 'short'
  let entryPrice = 0, hardStopPrice = 0, maxProfitPct = 0;
  let totalPnL = 0, winCount = 0, lossCount = 0;
  let maxWin = 0, maxLoss = 0;
  const trades = [];

  // 遍历5m K线做入场判断
  for (let i = 1; i < df_5m.length; i++) {
    const row = df_5m[i];
    const o = row.open, h = row.high, l = row.low, c = row.close;

    // 获取对应的30m数据
    const data30m = get30mAt(row.open_time);
    if (!data30m || data30m.ma288_30m === null || data30m.ma488_30m === null) continue;

    const ma288 = data30m.ma288_30m;
    const ma488 = data30m.ma488_30m;

    // 趋势方向判断 (基于30m MA)
    const trend = ma288 > ma488 ? 'bullish' : (ma288 < ma488 ? 'bearish' : null);
    if (!trend) continue;

    // 30m扩散过滤
    if (use30mExpanding && data30m.isExpanding_30m === false) continue;

    // 5m扩散过滤
    if (use5mExpanding && row.isExpanding_5m === false) continue;

    // 持仓管理
    if (position !== null) {
      const currentPnl = position === 'long'
        ? (c - entryPrice) / entryPrice * 100
        : (entryPrice - c) / entryPrice * 100;
      maxProfitPct = Math.max(maxProfitPct, currentPnl);

      let shouldStop = false, exitPrice = c;

      // 硬止损
      if (useHardStop) {
        if (position === 'long' && l <= hardStopPrice) {
          shouldStop = true; exitPrice = hardStopPrice;
        } else if (position === 'short' && h >= hardStopPrice) {
          shouldStop = true; exitPrice = hardStopPrice;
        }
      }

      // MA288止损 (使用30m MA)
      if (!shouldStop) {
        if (position === 'long' && o > ma288 && c < ma288) {
          shouldStop = true;
        } else if (position === 'short' && o < ma288 && c > ma288) {
          shouldStop = true;
        }
      }

      // 趋势反转退出
      if (!shouldStop) {
        if (position === 'long' && trend === 'bearish' && o > ma288 && c < ma288) {
          shouldStop = true;
        } else if (position === 'short' && trend === 'bullish' && o < ma288 && c > ma288) {
          shouldStop = true;
        }
      }

      if (shouldStop) {
        const pnl = position === 'long'
          ? (exitPrice - entryPrice) / entryPrice * 100
          : (entryPrice - exitPrice) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
        else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ pnl, side: position, exit: 'stop', time: row.open_time });
        position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0;
        continue;
      }

      // 移动止盈
      if (tpMode === 'trailing' && maxProfitPct >= trailingActivate) {
        if (maxProfitPct - currentPnl >= trailingCallback) {
          totalPnL += currentPnl;
          if (currentPnl > 0) { winCount++; maxWin = Math.max(maxWin, currentPnl); }
          else { lossCount++; maxLoss = Math.min(maxLoss, currentPnl); }
          trades.push({ pnl: currentPnl, side: position, exit: 'trailing', time: row.open_time });
          position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0;
          continue;
        }
      }
    }

    // 入场判断 (使用30m MA)
    let isEntry = false, entryDir = '';
    if (trend === 'bullish' && o < ma288 && c > ma288) {
      isEntry = true; entryDir = 'long';
    } else if (trend === 'bearish' && o > ma288 && c < ma288) {
      isEntry = true; entryDir = 'short';
    }

    if (isEntry) {
      // 如果有反向持仓，先平仓
      if (position !== null && position !== entryDir) {
        const pnl = position === 'long'
          ? (c - entryPrice) / entryPrice * 100
          : (entryPrice - c) / entryPrice * 100;
        totalPnL += pnl;
        if (pnl > 0) { winCount++; maxWin = Math.max(maxWin, pnl); }
        else { lossCount++; maxLoss = Math.min(maxLoss, pnl); }
        trades.push({ pnl, side: position, exit: 'reverse', time: row.open_time });
        position = null; entryPrice = 0; hardStopPrice = 0; maxProfitPct = 0;
      }

      // 开新仓
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
// 验证特定信号
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【验证: 2026-07-30 14:35 做空信号】");
console.log("=".repeat(70));

const signalTime = new Date('2026-07-30T14:35:13+08:00');
const data30mAtSignal = get30mAt(signalTime);

if (data30mAtSignal) {
  console.log(`\n30m数据 (${data30mAtSignal.open_time.toISOString()}):`);
  console.log(`  MA288 = ${data30mAtSignal.ma288_30m?.toFixed(4)}`);
  console.log(`  MA488 = ${data30mAtSignal.ma488_30m?.toFixed(4)}`);
  console.log(`  趋势: ${data30mAtSignal.ma288_30m > data30mAtSignal.ma488_30m ? 'bullish' : 'bearish'}`);
  console.log(`  30m扩散: ${data30mAtSignal.isExpanding_30m ? '✅' : '❌'}`);
}

// 检查14:30-15:00的5m K线
console.log("\n5m入场检查 (14:30-15:00):");
const check5m = df_5m.filter(r => {
  const t = r.open_time.getTime();
  return t >= new Date('2026-07-30T14:30:00+08:00').getTime() &&
         t < new Date('2026-07-30T15:00:00+08:00').getTime();
});

for (const r of check5m) {
  const ma288 = data30mAtSignal?.ma288_30m;
  if (!ma288) continue;

  const o = r.open, c = r.close;
  const isBearishEntry = data30mAtSignal.ma288_30m < data30mAtSignal.ma488_30m && o > ma288 && c < ma288;
  const isBullishEntry = data30mAtSignal.ma288_30m > data30mAtSignal.ma488_30m && o < ma288 && c > ma288;

  if (isBearishEntry) {
    console.log(`  ✅ ${r.open_time.toISOString().substring(11,16)}: 做空 O=${o}>${ma288.toFixed(2)}, C=${c}<${ma288.toFixed(2)}`);
  } else if (isBullishEntry) {
    console.log(`  ✅ ${r.open_time.toISOString().substring(11,16)}: 做多 O=${o}<${ma288.toFixed(2)}, C=${c}>${ma288.toFixed(2)}`);
  }
}

console.log(`\n结论: 使用正确的30m MA288(${data30mAtSignal?.ma288_30m?.toFixed(4)}), 做空信号不应产生`);

// ============================================================
// 回测: 不同参数组合
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【回测: 默认参数 (hardStop=2.5%, trailing 6+2)】");
console.log("=".repeat(70));

const defaultResult = runStrategy({
  useHardStop: true, hardStopPct: 2.5,
  tpMode: 'trailing', trailingActivate: 6.0, trailingCallback: 2.0,
  use30mExpanding: true, use5mExpanding: true,
});

console.log(`\n交易数: ${defaultResult.tradeCount}`);
console.log(`胜率: ${defaultResult.winRate.toFixed(1)}%`);
console.log(`总收益: ${defaultResult.totalPnL >= 0 ? '+' : ''}${defaultResult.totalPnL.toFixed(2)}%`);
console.log(`最大亏损: ${defaultResult.maxLoss.toFixed(2)}%`);
console.log(`\n做多: ${defaultResult.longCount}笔, 收益=${defaultResult.longPnL >= 0 ? '+' : ''}${defaultResult.longPnL.toFixed(2)}%, 胜率=${defaultResult.longWinRate.toFixed(1)}%`);
console.log(`做空: ${defaultResult.shortCount}笔, 收益=${defaultResult.shortPnL >= 0 ? '+' : ''}${defaultResult.shortPnL.toFixed(2)}%, 胜率=${defaultResult.shortWinRate.toFixed(1)}%`);

// 显示最近几笔交易
if (defaultResult.trades.length > 0) {
  console.log("\n最近10笔交易:");
  const recent = defaultResult.trades.slice(-10);
  for (const t of recent) {
    console.log(`  ${t.time.toISOString().substring(0,16)} | ${t.side.padEnd(5)} | PnL=${t.pnl >= 0 ? '+' : ''}${t.pnl.toFixed(2)}% | ${t.exit}`);
  }
}

// ============================================================
// 参数优化
// ============================================================
console.log("\n" + "=".repeat(70));
console.log("【参数优化: trailingActivate × trailingCallback】");
console.log("=".repeat(70));

const activates = [2, 3, 4, 5, 6, 8, 10];
const callbacks = [1, 2, 3, 4, 5];

let header = "activate\\cb |";
for (const cb of callbacks) header += `  ${cb}%    |`;
console.log("\n" + header);
console.log("-".repeat(header.length));

let bestResult = null;
let bestConfig = null;

for (const act of activates) {
  let row = `    ${String(act).padStart(2)}%     |`;
  for (const cb of callbacks) {
    const r = runStrategy({
      useHardStop: true, hardStopPct: 2.5,
      tpMode: 'trailing', trailingActivate: act, trailingCallback: cb,
      use30mExpanding: true, use5mExpanding: true,
    });
    row += `${r.totalPnL >= 0 ? '+' : ''}${r.totalPnL.toFixed(1).padStart(5)}% |`;

    if (!bestResult || r.totalPnL > bestResult.totalPnL) {
      bestResult = r;
      bestConfig = { activate: act, callback: cb };
    }
  }
  console.log(row);
}

console.log(`\n最优配置: activate=${bestConfig.activate}%, callback=${bestConfig.callback}%`);
console.log(`交易数: ${bestResult.tradeCount}`);
console.log(`胜率: ${bestResult.winRate.toFixed(1)}%`);
console.log(`总收益: ${bestResult.totalPnL >= 0 ? '+' : ''}${bestResult.totalPnL.toFixed(2)}%`);
console.log(`做多: ${bestResult.longCount}笔, ${bestResult.longPnL >= 0 ? '+' : ''}${bestResult.longPnL.toFixed(2)}%`);
console.log(`做空: ${bestResult.shortCount}笔, ${bestResult.shortPnL >= 0 ? '+' : ''}${bestResult.shortPnL.toFixed(2)}%`);

console.log("\n" + "=".repeat(70));
console.log("分析完成！");
console.log("=".repeat(70));
