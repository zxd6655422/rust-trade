/**
 * ETH 第十六次分析: 纯30m回测 + 波动率阈值过滤 + 矩阵测试
 * (对齐Python策略逻辑: 30m bar迭代, inline入场/出场信号)
 */

const fs = require('fs');

// 加载CSV数据，解析open_time为Date对象，open/high/low/close/volume为数字
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

// 加载30m数据
console.log("加载数据...");
const df = loadCSV('../data_2026-08-13/kline_30m_202608141605_ETH.csv', 'open_time');
console.log(`30m: ${df.length} bars, ${df[0].open_time.toISOString().slice(0,16)} ~ ${df[df.length-1].open_time.toISOString().slice(0,16)}`);

// realized_vol_48: 基于收盘价的48周期波动率 (百分比)
function realizedVol48(closes) {
  const n = closes.length;
  const out = new Array(n).fill(null);
  const rets = new Array(n).fill(0);
  for (let i = 1; i < n; i++) {
    if (closes[i - 1] !== 0) rets[i] = closes[i] / closes[i - 1] - 1;
  }
  for (let i = 48; i < n; i++) {
    let sum = 0, sumSq = 0;
    for (let j = i - 47; j <= i; j++) {
      sum += rets[j];
      sumSq += rets[j] * rets[j];
    }
    const mean = sum / 48;
    const variance = Math.max(0, sumSq / 48 - mean * mean);
    out[i] = Math.sqrt(variance) * 100;
  }
  return out;
}

// 计算30m指标: MA288, MA488, realized_vol_48
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

  const ma288 = calcMA(288);
  const ma488 = calcMA(480);
  const vol48 = realizedVol48(closes);

  for (let i = 0; i < df.length; i++) {
    df[i].ma288 = ma288[i];
    df[i].ma488 = ma488[i];
    df[i].realized_vol_48 = vol48[i];
  }
}

console.log("计算30m指标 (MA288, MA488, realized_vol_48)...");
addIndicators(df);

/**
 * 回测函数 (对齐Python策略逻辑)
 * 遍历30m bar，inline计算入场/出场信号
 * @param {Object} df - 30m数据数组
 * @param {Object} params - 策略参数 {hard_stop_pct, take_profit_mode, trailing_activate_pct, trailing_callback_pct}
 * @param {number} volThreshold - 波动率过滤阈值，0表示不过滤
 */
function runBacktest(df, params, volThreshold) {
  const closes = df.map(r => r.close);
  const fast_ma = df.map(r => r.ma288);
  const slow_ma = df.map(r => r.ma488);
  const SLOW = 480;

  let pos = null;
  let totalPnL = 0, winCount = 0, lossCount = 0, tradeCount = 0;
  let volSkipped = 0;

  for (let i = 0; i < df.length; i++) {
    if (i + 1 < SLOW) continue;
    const close = closes[i];
    const prev_close = closes[i - 1];
    const fma = fast_ma[i];
    const sma = slow_ma[i];
    const prev_fma = fast_ma[i - 1];
    const bar = df[i];

    if (fma === null || sma === null || prev_fma === null) continue;

    // === 持仓中：平仓 ===
    if (pos !== null) {
      const pnl = pos.side === 'long' ? (close - pos.entry) / pos.entry * 100 : (pos.entry - close) / pos.entry * 100;
      pos.maxProfit = Math.max(pos.maxProfit, pnl);
      let exitPrice = null;

      // 硬止损
      if (params.hard_stop_pct > 0) {
        if (pos.side === 'long' && bar.low <= pos.hardStopPrice) exitPrice = pos.hardStopPrice;
        else if (pos.side === 'short' && bar.high >= pos.hardStopPrice) exitPrice = pos.hardStopPrice;
      }

      // MA288穿越止损 (对齐生产 check_exit_conditions step 2)
      if (exitPrice === null) {
        if (pos.side === 'long' && prev_close > prev_fma && close < fma) exitPrice = close;
        else if (pos.side === 'short' && prev_close < prev_fma && close > fma) exitPrice = close;
      }

      // 移动止盈
      if (exitPrice === null && params.take_profit_mode === 'trailing') {
        if (pos.maxProfit >= params.trailing_activate_pct && pos.maxProfit - pnl >= params.trailing_callback_pct) {
          exitPrice = close;
        }
      }

      // 趋势反转出场 (对齐生产 check_exit_conditions step 6)
      if (exitPrice === null) {
        if (pos.side === 'long' && fma < sma) exitPrice = close;
        else if (pos.side === 'short' && fma > sma) exitPrice = close;
      }

      if (exitPrice !== null) {
        const ret = pos.side === 'long' ? (exitPrice - pos.entry) / pos.entry * 100 : (pos.entry - exitPrice) / pos.entry * 100;
        totalPnL += ret; if (ret > 0) winCount++; else lossCount++; tradeCount++;
        pos = null;
        continue;
      }
    }

    // === 无持仓：入场 ===
    if (pos === null) {
      // vol过滤
      if (volThreshold > 0 && bar.realized_vol_48 !== null && bar.realized_vol_48 >= volThreshold) {
        volSkipped++;
        continue;
      }

      if (fma > sma) {
        if (prev_close < prev_fma && close > fma) {
          const hardStop = params.hard_stop_pct > 0 ? close * (1 - params.hard_stop_pct / 100) : fma * 0.98;
          pos = { side: 'long', entry: close, hardStopPrice: hardStop, maxProfit: 0 };
        }
      } else if (fma < sma) {
        if (prev_close > prev_fma && close < fma) {
          const hardStop = params.hard_stop_pct > 0 ? close * (1 + params.hard_stop_pct / 100) : fma * 1.02;
          pos = { side: 'short', entry: close, hardStopPrice: hardStop, maxProfit: 0 };
        }
      }
    }
  }

  // 末尾持仓
  if (pos !== null) {
    const exitPrice = closes[closes.length - 1];
    const ret = pos.side === 'long' ? (exitPrice - pos.entry) / pos.entry * 100 : (pos.entry - exitPrice) / pos.entry * 100;
    totalPnL += ret; if (ret > 0) winCount++; else lossCount++; tradeCount++;
  }

  return { tradeCount, winCount, lossCount, winRate: tradeCount > 0 ? winCount / tradeCount * 100 : 0, totalPnL, volSkipped };
}

// 矩阵测试参数
const hardStops = [1.0, 1.5, 2.0, 2.5]; // ETH
const activates = [2, 3, 4, 5, 6];
const callbacks = [1, 2, 3, 4];
const VOL_THRESHOLD = 0.445; // ETH

// A: 基线 (无vol过滤)
console.log("\n" + "=".repeat(70));
console.log("【A: 纯30m回测, 无vol过滤 (基线)】");
console.log("=".repeat(70));
console.log("hardStop | activate | callback | 交易数 | 胜率   | 总收益   ");
console.log("-".repeat(65));

let bestA = null, countA = 0;
for (const hs of hardStops) {
  for (const act of activates) {
    for (const cb of callbacks) {
      const r = runBacktest(df, { hard_stop_pct: hs, take_profit_mode: 'trailing', trailing_activate_pct: act, trailing_callback_pct: cb }, 0);
      if (!bestA || r.totalPnL > bestA.totalPnL) bestA = { ...r, hs, act, cb };
      if (r.totalPnL > 0) {
        countA++;
        console.log(`  ${hs}%    |    ${String(act).padStart(2)}%    |    ${String(cb).padStart(2)}%    | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ${(r.totalPnL>=0?'+':'')+r.totalPnL.toFixed(2).padStart(7)}%`);
      }
    }
  }
}
console.log(`\n正收益组合: ${countA}个`);
console.log(`基线最优: hs=${bestA.hs}% act=${bestA.act}% cb=${bestA.cb}% → ${bestA.totalPnL.toFixed(2)}%, ${bestA.tradeCount}笔, 胜率${bestA.winRate.toFixed(1)}%`);

// B: vol过滤
console.log("\n" + "=".repeat(70));
console.log(`【B: 纯30m回测 + vol过滤 (realized_vol_48 < ${VOL_THRESHOLD})】`);
console.log("=".repeat(70));
console.log("hardStop | activate | callback | 交易数 | 胜率   | 总收益   | vol跳过");
console.log("-".repeat(75));

let bestB = null, countB = 0;
for (const hs of hardStops) {
  for (const act of activates) {
    for (const cb of callbacks) {
      const r = runBacktest(df, { hard_stop_pct: hs, take_profit_mode: 'trailing', trailing_activate_pct: act, trailing_callback_pct: cb }, VOL_THRESHOLD);
      if (!bestB || r.totalPnL > bestB.totalPnL) bestB = { ...r, hs, act, cb };
      if (r.totalPnL > 0) {
        countB++;
        console.log(`  ${hs}%    |    ${String(act).padStart(2)}%    |    ${String(cb).padStart(2)}%    | ${String(r.tradeCount).padStart(6)} | ${r.winRate.toFixed(1).padStart(5)}% | ${(r.totalPnL>=0?'+':'')+r.totalPnL.toFixed(2).padStart(7)}% | ${r.volSkipped}`);
      }
    }
  }
}
console.log(`\n正收益组合: ${countB}个`);
console.log(`vol过滤最优: hs=${bestB.hs}% act=${bestB.act}% cb=${bestB.cb}% → ${bestB.totalPnL.toFixed(2)}%, ${bestB.tradeCount}笔, 胜率${bestB.winRate.toFixed(1)}%, vol跳过${bestB.volSkipped}次`);

// 汇总
console.log("\n" + "=".repeat(70));
console.log("【汇总对比】");
console.log("=".repeat(70));
console.log(`
策略                              | 参数                       | 收益      | 交易数 | 胜率   | 正收益组合
----------------------------------|----------------------------|-----------|--------|--------|----------
30m纯回测 + 无vol过滤 (基线)       | hs=${bestA.hs}% act=${bestA.act}% cb=${bestA.cb}% | ${(bestA.totalPnL>=0?'+':'')+bestA.totalPnL.toFixed(2)}%   | ${String(bestA.tradeCount).padStart(6)} | ${bestA.winRate.toFixed(1)}% | ${countA}/80
30m纯回测 + vol<${VOL_THRESHOLD}         | hs=${bestB.hs}% act=${bestB.act}% cb=${bestB.cb}% | ${(bestB.totalPnL>=0?'+':'')+bestB.totalPnL.toFixed(2)}%   | ${String(bestB.tradeCount).padStart(6)} | ${bestB.winRate.toFixed(1)}% | ${countB}/80
`);

if (bestA.totalPnL !== 0) {
  const improve = ((bestB.totalPnL - bestA.totalPnL) / Math.abs(bestA.totalPnL) * 100).toFixed(1);
  console.log(`vol过滤改善: ${bestA.totalPnL.toFixed(2)}% → ${bestB.totalPnL.toFixed(2)}% (${improve >= 0 ? '+' : ''}${improve}%)`);
}

console.log("=".repeat(70));
console.log("分析完成！");
console.log("=".repeat(70));
