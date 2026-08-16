/**
 * 周短策略 10倍杠杆测试
 * 测试三个策略使用10倍合约杠杆的效果
 */

const fs = require('fs');

// ============ 数据加载 ============
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

// ============ 指标计算 ============
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
  const bbPos = new Array(df.length).fill(null);
  for (let i = 99; i < df.length; i++) {
    let sumSq = 0;
    for (let j = i - 99; j <= i; j++) sumSq += (closes[j] - bbMid[i]) ** 2;
    const std = Math.sqrt(sumSq / 100);
    bbUpper[i] = bbMid[i] + 2 * std;
    bbLower[i] = bbMid[i] - 2 * std;
    bbWidth[i] = (bbUpper[i] - bbLower[i]) / bbMid[i] * 100;
    bbPos[i] = (closes[i] - bbLower[i]) / (bbUpper[i] - bbLower[i]) * 100;
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
  const spread = new Array(df.length).fill(null);
  const spreadDelta = new Array(df.length).fill(null);
  const isExpanding = new Array(df.length).fill(null);
  for (let i = 0; i < df.length; i++) {
    if (ma288[i] !== null && ma488[i] !== null) spread[i] = ma288[i] - ma488[i];
  }
  const anglePeriod = 5;
  for (let i = anglePeriod; i < df.length; i++) {
    if (spread[i] !== null && spread[i - anglePeriod] !== null) {
      spreadDelta[i] = spread[i] - spread[i - anglePeriod];
      isExpanding[i] = Math.abs(spread[i]) > Math.abs(spread[i - anglePeriod]);
    }
  }
  for (let i = 0; i < df.length; i++) {
    df[i][`${prefix}ma48`] = ma48[i];
    df[i][`${prefix}ma288`] = ma288[i];
    df[i][`${prefix}ma488`] = ma488[i];
    df[i][`${prefix}bbWidth`] = bbWidth[i];
    df[i][`${prefix}bbPos`] = bbPos[i];
    df[i][`${prefix}ma288Slope`] = ma288Slope[i];
    df[i][`${prefix}volRatio`] = volRatio[i];
    df[i][`${prefix}isExpanding`] = isExpanding[i];
  }
  return df;
}

// ============ 资金管理回测引擎 ============
function runStrategyWithMoney(df, config, moneyConfig) {
  const {
    useHardStop = true, hardStopPct = 2.0,
    tpMode = 'trailing', trailingActivate = 5.0, trailingCallback = 5.0,
    use5mExpanding = true, use30mExpanding = true,
  } = config;

  const {
    initialCapital = 10000,
    positionSizePercent = 0.1,
    leverage = 10,
    commissionRate = 0.0002,  // 合约手续费 0.02%
    slippageRate = 0.0001,
  } = moneyConfig;

  let capital = initialCapital;
  let position = null;
  let entryPrice = 0, entryCapital = 0, hardStopPrice = 0, maxProfitPct = 0;
  const trades = [];
  let totalCommission = 0;
  let liquidated = false;

  for (let i = 1; i < df.length; i++) {
    if (liquidated) break;

    const row = df[i];
    const ma288 = row.m30_ma288, ma488 = row.m30_ma488;
    const o = row.open, h = row.high, l = row.low, c = row.close;
    const slope = row.m30_ma288Slope, bbw = row.m30_bbWidth, volRatio = row.m30_volRatio;

    // 趋势方向判断
    const trend = ma288 > ma488 ? 'bullish' : (ma288 < ma488 ? 'bearish' : null);
    if (!trend) continue;

    // 30m扩散过滤
    if (use30mExpanding && row.m30_isExpanding === false) continue;

    // 持仓中
    if (position !== null) {
      const currentPnl = position === 'long' ? (c - entryPrice) / entryPrice : (entryPrice - c) / entryPrice;
      const unrealizedPnl = currentPnl * entryCapital;
      maxProfitPct = Math.max(maxProfitPct, currentPnl * 100);

      // 检查爆仓
      if (unrealizedPnl < -entryCapital * 0.9) {
        liquidated = true;
        const loss = -entryCapital * 0.95;
        capital += loss;
        trades.push({
          pnl: loss, pnlPercent: loss / entryCapital * 100,
          side: position, exitReason: 'liquidation',
        });
        position = null;
        continue;
      }

      let shouldStop = false, exitPrice = c, exitReason = '';

      // 硬止损
      if (useHardStop) {
        if (position === 'long' && l <= hardStopPrice) {
          shouldStop = true; exitPrice = hardStopPrice; exitReason = 'hard_stop';
        } else if (position === 'short' && h >= hardStopPrice) {
          shouldStop = true; exitPrice = hardStopPrice; exitReason = 'hard_stop';
        }
      }

      // MA288止损
      if (!shouldStop) {
        if (position === 'long' && o > ma288 && c < ma288) {
          shouldStop = true; exitReason = 'ma288_cross';
        } else if (position === 'short' && o < ma288 && c > ma288) {
          shouldStop = true; exitReason = 'ma288_cross';
        }
      }

      // 移动止盈
      if (!shouldStop && tpMode === 'trailing' && maxProfitPct >= trailingActivate) {
        if (maxProfitPct - currentPnl * 100 >= trailingCallback) {
          shouldStop = true; exitReason = 'trailing_stop';
        }
      }

      // 趋势反转退出
      if (!shouldStop) {
        if (position === 'long' && trend === 'bearish' && o > ma288 && c < ma288) {
          shouldStop = true; exitReason = 'trend_reversal';
        } else if (position === 'short' && trend === 'bullish' && o < ma288 && c > ma288) {
          shouldStop = true; exitReason = 'trend_reversal';
        }
      }

      // 执行离场
      if (shouldStop) {
        const pnlPercent = position === 'long'
          ? (exitPrice - entryPrice) / entryPrice * 100
          : (entryPrice - exitPrice) / entryPrice * 100;
        const grossPnl = pnlPercent / 100 * entryCapital;

        // 计算手续费
        const exitCommission = Math.abs(entryCapital) * exitPrice / entryPrice * commissionRate;
        const entryCommission = Math.abs(entryCapital) * commissionRate;
        const totalCost = exitCommission + entryCommission;
        totalCommission += totalCost;

        const netPnl = grossPnl - totalCost;
        capital += netPnl;

        trades.push({
          pnl: netPnl, pnlPercent: netPnl / entryCapital * 100,
          side: position, exitReason,
        });

        position = null;
        entryPrice = 0;
        entryCapital = 0;
        hardStopPrice = 0;
        maxProfitPct = 0;
      }
    }

    // 开仓逻辑
    if (position === null && capital > 0) {
      let isEntry = false, entryDir = '';
      if (trend === 'bullish' && o < ma288 && c > ma288) { isEntry = true; entryDir = 'long'; }
      else if (trend === 'bearish' && o > ma288 && c < ma288) { isEntry = true; entryDir = 'short'; }

      if (isEntry) {
        const availableCapital = capital * positionSizePercent * leverage;
        const entryPriceWithSlippage = entryDir === 'long'
          ? c * (1 + slippageRate)
          : c * (1 - slippageRate);

        position = entryDir;
        entryPrice = entryPriceWithSlippage;
        entryCapital = availableCapital;
        maxProfitPct = 0;
        hardStopPrice = entryDir === 'long'
          ? entryPrice * (1 - hardStopPct / 100)
          : entryPrice * (1 + hardStopPct / 100);
      }
    }
  }

  // 强制平仓
  if (position !== null && !liquidated) {
    const lastRow = df[df.length - 1];
    const c = lastRow.close;
    const pnlPercent = position === 'long'
      ? (c - entryPrice) / entryPrice * 100
      : (entryPrice - c) / entryPrice * 100;
    const grossPnl = pnlPercent / 100 * entryCapital;
    const exitCommission = Math.abs(entryCapital) * c / entryPrice * commissionRate;
    const entryCommission = Math.abs(entryCapital) * commissionRate;
    const totalCost = exitCommission + entryCommission;
    totalCommission += totalCost;

    const netPnl = grossPnl - totalCost;
    capital += netPnl;

    trades.push({
      pnl: netPnl, pnlPercent: netPnl / entryCapital * 100,
      side: position, exitReason: 'force_close',
    });
  }

  // 统计
  const wins = trades.filter(t => t.pnl > 0);
  const losses = trades.filter(t => t.pnl <= 0);
  const longTrades = trades.filter(t => t.side === 'long');
  const shortTrades = trades.filter(t => t.side === 'short');
  const longWins = longTrades.filter(t => t.pnl > 0);
  const shortWins = shortTrades.filter(t => t.pnl > 0);

  // 月度统计
  const monthlyData = {};
  trades.forEach(t => {
    const month = t.exitTime ? t.exitTime.substring(0, 7) : 'unknown';
    if (!monthlyData[month]) monthlyData[month] = 0;
    monthlyData[month] += t.pnl;
  });

  return {
    trades: trades.length,
    winCount: wins.length,
    lossCount: losses.length,
    winRate: trades.length > 0 ? (wins.length / trades.length * 100) : 0,
    totalPnl: capital - initialCapital,
    totalReturn: ((capital / initialCapital) - 1) * 100,
    maxWin: Math.max(...trades.map(t => t.pnl), 0),
    maxLoss: Math.min(...trades.map(t => t.pnl), 0),
    totalCommission,
    liquidated,
    finalCapital: capital,
    longCount: longTrades.length,
    longPnl: longTrades.reduce((s, t) => s + t.pnl, 0),
    longWinRate: longTrades.length > 0 ? (longWins.length / longTrades.length * 100) : 0,
    shortCount: shortTrades.length,
    shortPnl: shortTrades.reduce((s, t) => s + t.pnl, 0),
    shortWinRate: shortTrades.length > 0 ? (shortWins.length / shortTrades.length * 100) : 0,
    monthlyData,
  };
}

// ============ 主程序 ============
function main() {
  console.log('='.repeat(90));
  console.log('周短策略 10倍杠杆测试');
  console.log('='.repeat(90));

  // 加载数据
  const df_5m = loadCSV('../kline_5m_202607232006.csv', 'open_time');
  const df_30m = loadCSV('../kline_30m_202607232006.csv', 'open_time');

  addIndicators(df_5m, 'm5_');
  addIndicators(df_30m, 'm30_');

  const df_30m_valid = df_30m.filter(r => r.m30_ma288 !== null && r.m30_ma488 !== null);

  console.log(`\n数据: ${df_30m_valid.length}条30分钟K线`);
  console.log(`时间范围: ${df_30m_valid[0].open_time.toISOString()} ~ ${df_30m_valid[df_30m_valid.length - 1].open_time.toISOString()}`);

  // 资金配置
  const moneyConfig = {
    initialCapital: 10000,
    positionSizePercent: 0.1,
    leverage: 10,
    commissionRate: 0.0002,
    slippageRate: 0.0001,
  };

  // 测试不同配置
  const testCases = [
    {
      name: '配置1: 硬止损2% + 移动止盈5%/5%',
      config: {
        useHardStop: true, hardStopPct: 2.0,
        tpMode: 'trailing', trailingActivate: 5.0, trailingCallback: 5.0,
        use5mExpanding: true, use30mExpanding: true,
      },
    },
    {
      name: '配置2: 硬止损1.5% + 移动止盈3%/3%',
      config: {
        useHardStop: true, hardStopPct: 1.5,
        tpMode: 'trailing', trailingActivate: 3.0, trailingCallback: 3.0,
        use5mExpanding: true, use30mExpanding: true,
      },
    },
    {
      name: '配置3: 硬止损1.5% + 移动止盈5%/5%',
      config: {
        useHardStop: true, hardStopPct: 1.5,
        tpMode: 'trailing', trailingActivate: 5.0, trailingCallback: 5.0,
        use5mExpanding: true, use30mExpanding: true,
      },
    },
    {
      name: '配置4: 硬止损1% + 移动止盈3%/3%',
      config: {
        useHardStop: true, hardStopPct: 1.0,
        tpMode: 'trailing', trailingActivate: 3.0, trailingCallback: 3.0,
        use5mExpanding: true, use30mExpanding: true,
      },
    },
    {
      name: '配置5: 硬止损1% + 移动止盈2%/2%',
      config: {
        useHardStop: true, hardStopPct: 1.0,
        tpMode: 'trailing', trailingActivate: 2.0, trailingCallback: 2.0,
        use5mExpanding: true, use30mExpanding: true,
      },
    },
  ];

  const results = [];

  console.log('\n' + '测试不同配置...\n');

  testCases.forEach(tc => {
    const result = runStrategyWithMoney(df_30m_valid, tc.config, moneyConfig);
    results.push({ name: tc.name, ...result });
  });

  // 输出对比表格
  console.log('\n' +
    '配置'.padEnd(40) +
    '净利润'.padEnd(12) +
    '收益率'.padEnd(10) +
    '胜率'.padEnd(8) +
    '交易'.padEnd(6) +
    '最大回撤'.padEnd(10) +
    '手续费'.padEnd(10) +
    '爆仓'
  );
  console.log('-'.repeat(100));

  results.forEach(r => {
    const liquidated = r.liquidated ? '💥' : '✅';
    const maxDrawdown = r.maxLoss < 0 ? Math.abs(r.maxLoss / 10000 * 100) : 0;
    console.log(
      r.name.padEnd(40) +
      `${r.totalPnl >= 0 ? '+' : ''}${r.totalPnl.toFixed(2)}`.padEnd(12) +
      `${r.totalReturn >= 0 ? '+' : ''}${r.totalReturn.toFixed(2)}%`.padEnd(10) +
      `${r.winRate.toFixed(1)}%`.padEnd(8) +
      `${r.trades}`.padEnd(6) +
      `${maxDrawdown.toFixed(2)}%`.padEnd(10) +
      `${r.totalCommission.toFixed(2)}`.padEnd(10) +
      liquidated
    );
  });

  // 找出最佳配置
  const validResults = results.filter(r => !r.liquidated);
  if (validResults.length > 0) {
    validResults.sort((a, b) => b.totalPnl - a.totalPnl);
    const best = validResults[0];

    console.log('\n' + '='.repeat(90));
    console.log('🏆 最佳配置');
    console.log('='.repeat(90));
    console.log(`配置: ${best.name}`);
    console.log(`净利润: ${best.totalPnl >= 0 ? '+' : ''}${best.totalPnl.toFixed(2)} USDT`);
    console.log(`收益率: ${best.totalReturn >= 0 ? '+' : ''}${best.totalReturn.toFixed(2)}%`);
    console.log(`胜率: ${best.winRate.toFixed(1)}%`);
    console.log(`交易次数: ${best.trades}`);
    console.log(`手续费: ${best.totalCommission.toFixed(2)} USDT`);

    // 计算年化收益
    const tradingDays = (df_30m_valid[df_30m_valid.length - 1].open_time - df_30m_valid[0].open_time) / (1000 * 60 * 60 * 24);
    const annualizedReturn = (Math.pow(1 + best.totalReturn / 100, 365 / tradingDays) - 1) * 100;
    console.log(`年化收益: ${annualizedReturn >= 0 ? '+' : ''}${annualizedReturn.toFixed(1)}%`);

    // 多空分析
    console.log('\n📊 多空分析:');
    console.log(`  做多: ${best.longCount}笔, 盈亏${best.longPnl >= 0 ? '+' : ''}${best.longPnl.toFixed(2)} USDT, 胜率${best.longWinRate.toFixed(1)}%`);
    console.log(`  做空: ${best.shortCount}笔, 盈亏${best.shortPnl >= 0 ? '+' : ''}${best.shortPnl.toFixed(2)} USDT, 胜率${best.shortWinRate.toFixed(1)}%`);

    // 月度统计
    console.log('\n📅 月度统计:');
    Object.keys(best.monthlyData).sort().forEach(month => {
      const pnl = best.monthlyData[month];
      const emoji = pnl >= 0 ? '✅' : '❌';
      console.log(`  ${emoji} ${month}: ${pnl >= 0 ? '+' : ''}${pnl.toFixed(2)} USDT`);
    });

    // 不同资金规模收益
    console.log('\n💰 不同资金规模收益:');
    [1000, 5000, 10000, 50000, 100000].forEach(capital => {
      const profit = capital * best.totalReturn / 100;
      console.log(`  ${capital.toLocaleString()} USDT → ${profit >= 0 ? '+' : ''}${profit.toFixed(2)} USDT`);
    });
  }

  // 结论
  console.log('\n' + '='.repeat(90));
  console.log('📌 结论');
  console.log('='.repeat(90));

  const liqCount = results.filter(r => r.liquidated).length;
  console.log(`\n  爆仓配置: ${liqCount}/${results.length}`);

  if (validResults.length > 0) {
    const best = validResults[0];
    const tradingDays = (df_30m_valid[df_30m_valid.length - 1].open_time - df_30m_valid[0].open_time) / (1000 * 60 * 60 * 24);
    const annualizedReturn = (Math.pow(1 + best.totalReturn / 100, 365 / tradingDays) - 1) * 100;

    console.log(`\n  推荐: ${best.name}`);
    console.log(`  年化收益: ${annualizedReturn.toFixed(1)}%`);

    if (annualizedReturn > 20) {
      console.log('\n  ✅ 年化收益超过20%，非常优秀！');
    } else if (annualizedReturn > 10) {
      console.log('\n  ✅ 年化收益超过10%，值得考虑！');
    } else if (annualizedReturn > 5) {
      console.log('\n  ⚠️ 年化收益5-10%，需要权衡风险');
    } else {
      console.log('\n  ⚠️ 年化收益较低，需要谨慎考虑');
    }
  }
}

main();
