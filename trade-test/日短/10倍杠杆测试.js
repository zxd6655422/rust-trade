/**
 * 10倍杠杆测试
 * 测试使用10倍合约杠杆的收益和风险
 */

const fs = require('fs');
const path = require('path');

// ============ 配置 ============
const CONFIG = {
    ma48_period: 48,
    ma288_period: 288,
    ma488_period: 488,
    bollinger_period: 100,
    bollinger_std_mult: 2.0,
    trend_confirm_bars: 3,
    initial_capital: 10000,
    position_size_percent: 0.1,
    commission_rate: 0.0004,  // 现货手续费
    slippage_rate: 0.0001,
};

// 合约配置
const CONTRACT_CONFIG = {
    ...CONFIG,
    commission_rate: 0.0002,  // 合约手续费更低 (0.02%)
    leverage: 10,
};

// ============ 数据加载 ============
function loadCSV(filePath) {
    const content = fs.readFileSync(filePath, 'utf-8');
    const lines = content.trim().split('\n');
    const data = [];

    for (let i = 1; i < lines.length; i++) {
        const values = lines[i].split(',');
        if (values.length < 7) continue;

        data.push({
            symbol: values[0].trim(),
            open_time: values[1].trim(),
            open: parseFloat(values[2]),
            high: parseFloat(values[3]),
            low: parseFloat(values[4]),
            close: parseFloat(values[5]),
            volume: parseFloat(values[6]),
        });
    }

    data.sort((a, b) => new Date(a.open_time) - new Date(b.open_time));
    return data;
}

// ============ 技术指标 ============
function calcSMA(data, period) {
    const result = new Array(data.length).fill(null);
    for (let i = period - 1; i < data.length; i++) {
        let sum = 0;
        for (let j = 0; j < period; j++) sum += data[i - j].close;
        result[i] = sum / period;
    }
    return result;
}

function calcBollinger(data, period, stdMult) {
    const middle = new Array(data.length).fill(null);
    const upper = new Array(data.length).fill(null);
    const lower = new Array(data.length).fill(null);

    for (let i = period - 1; i < data.length; i++) {
        let sum = 0;
        for (let j = 0; j < period; j++) sum += data[i - j].close;
        const ma = sum / period;
        middle[i] = ma;

        let sqSum = 0;
        for (let j = 0; j < period; j++) sqSum += Math.pow(data[i - j].close - ma, 2);
        const std = Math.sqrt(sqSum / period);
        upper[i] = ma + stdMult * std;
        lower[i] = ma - stdMult * std;
    }
    return { middle, upper, lower };
}

// ============ 趋势判断 ============
function getTrendIntent(ma48, ma288, bollMiddle, index, confirmBars) {
    if (index < confirmBars) return 'neutral';

    let bullCount = 0, bearCount = 0;

    for (let i = 0; i < confirmBars; i++) {
        const idx = index - i;
        const prevIdx = idx - 1;

        if (ma48[idx] === null || ma288[idx] === null || bollMiddle[idx] === null) continue;
        if (ma48[prevIdx] === null || bollMiddle[prevIdx] === null) continue;

        const ma48Above288 = ma48[idx] > ma288[idx];
        const ma48Rising = ma48[idx] > ma48[prevIdx];
        const bollRising = bollMiddle[idx] > bollMiddle[prevIdx];

        if (ma48Above288 && ma48Rising && bollRising) bullCount++;
        if (!ma48Above288 && !ma48Rising && !bollRising) bearCount++;
    }

    if (bullCount >= confirmBars) return 'bull';
    if (bearCount >= confirmBars) return 'bear';
    return 'neutral';
}

// ============ 回测引擎 ============
function backtest(data, config, options = {}) {
    const n = data.length;
    const ma48 = calcSMA(data, config.ma48_period);
    const ma288 = calcSMA(data, config.ma288_period);
    const boll = calcBollinger(data, config.bollinger_period, config.bollinger_std_mult);

    let capital = config.initial_capital;
    let position = 0;
    let entryPrice = 0, entryTime = '', entryIdx = 0, entryCapital = 0;
    let touchedBollBand = false;
    let highestPnl = 0, lowestPnl = 0;
    let crossBelowCount = 0, crossAboveCount = 0;

    const trades = [];
    let totalCommission = 0;
    let filteredSignals = 0;
    let liquidated = false; // 是否爆仓

    const startIdx = Math.max(config.ma488_period, config.bollinger_period) + config.trend_confirm_bars;

    for (let i = startIdx; i < n; i++) {
        if (liquidated) break; // 已爆仓，停止交易

        const bar = data[i];
        const currentMa48 = ma48[i];
        const currentMa288 = ma288[i];
        const currentBollMid = boll.middle[i];
        const currentBollUpper = boll.upper[i];
        const currentBollLower = boll.lower[i];

        if (currentMa48 === null || currentMa288 === null || currentBollMid === null) continue;

        const trend = getTrendIntent(ma48, ma288, boll.middle, i, config.trend_confirm_bars);
        const openAboveMid = bar.open > currentBollMid;
        const closeAboveMid = bar.close > currentBollMid;

        // 持仓中
        if (position !== 0) {
            const currentPrice = bar.close;
            const priceDiff = position > 0
                ? currentPrice - entryPrice
                : entryPrice - currentPrice;
            const unrealizedPnl = priceDiff * Math.abs(position);
            const currentPnlPercent = unrealizedPnl / entryCapital * 100;

            // 检查爆仓（亏损超过保证金的90%）
            if (unrealizedPnl < -entryCapital * 0.9) {
                liquidated = true;
                const loss = -entryCapital * 0.95; // 爆仓损失95%保证金
                capital += loss;
                trades.push({
                    type: position > 0 ? 'LONG' : 'SHORT',
                    entryTime, entryPrice, exitTime: bar.open_time, exitPrice: bar.close,
                    grossPnl: loss, netPnl: loss, totalCost: 0,
                    pnlPercent: loss / entryCapital * 100,
                    holdBars: i - entryIdx, exitReason: 'liquidation',
                });
                position = 0;
                continue;
            }

            if (position > 0) {
                if (unrealizedPnl > highestPnl) highestPnl = unrealizedPnl;
            } else {
                if (unrealizedPnl > lowestPnl) lowestPnl = unrealizedPnl;
            }

            if (position > 0 && bar.high >= currentBollUpper) touchedBollBand = true;
            if (position < 0 && bar.low <= currentBollLower) touchedBollBand = true;

            // 更新连续穿越中轨计数
            if (options.consecutiveCross) {
                if (position > 0 && !closeAboveMid) {
                    crossBelowCount++;
                } else {
                    crossBelowCount = 0;
                }
                if (position < 0 && closeAboveMid) {
                    crossAboveCount++;
                } else {
                    crossAboveCount = 0;
                }
            }

            let shouldExit = false;
            let exitReason = '';

            // 布林轨反弹离场
            if (touchedBollBand && unrealizedPnl > 0) {
                const distToMid = Math.abs(bar.close - currentBollMid);
                const bollWidth = currentBollUpper - currentBollLower;
                const distPercent = distToMid / bollWidth;

                if (distPercent < 0.3 && currentPnlPercent > 1) {
                    shouldExit = true;
                    exitReason = 'boll_rebound_profit';
                }
            }

            // 移动止盈
            const peakPnl = position > 0 ? highestPnl : lowestPnl;
            const peakPnlPercent = peakPnl / entryCapital * 100;
            if (peakPnlPercent > 2) {
                const drawdown = peakPnl - unrealizedPnl;
                const drawdownPercent = drawdown / entryCapital * 100;
                if (drawdownPercent > 0.5) {
                    shouldExit = true;
                    exitReason = 'trailing_stop';
                }
            }

            // 穿越中轨离场
            if (!shouldExit) {
                if (options.consecutiveCross) {
                    if (position > 0 && crossBelowCount >= 2) {
                        shouldExit = true;
                        exitReason = 'bollinger_cross_consecutive';
                    } else if (position < 0 && crossAboveCount >= 2) {
                        shouldExit = true;
                        exitReason = 'bollinger_cross_consecutive';
                    }
                } else {
                    if (position > 0 && openAboveMid && !closeAboveMid) {
                        shouldExit = true;
                        exitReason = 'bollinger_cross';
                    } else if (position < 0 && !openAboveMid && closeAboveMid) {
                        shouldExit = true;
                        exitReason = 'bollinger_cross';
                    }
                }
            }

            // 执行离场
            if (shouldExit) {
                const exitPrice = bar.close;
                const priceDiff = position > 0
                    ? exitPrice - entryPrice
                    : entryPrice - exitPrice;
                const grossPnl = priceDiff * Math.abs(position);

                const exitCommission = Math.abs(position) * exitPrice * config.commission_rate;
                const entryCommission = Math.abs(position) * entryPrice * config.commission_rate;
                const slippage = Math.abs(position) * exitPrice * config.slippage_rate;
                const totalCost = exitCommission + entryCommission + slippage;
                totalCommission += totalCost;

                const netPnl = grossPnl - totalCost;
                capital += netPnl;

                trades.push({
                    type: position > 0 ? 'LONG' : 'SHORT',
                    entryTime, entryPrice, exitTime: bar.open_time, exitPrice,
                    grossPnl, netPnl, totalCost,
                    pnlPercent: netPnl / entryCapital * 100,
                    holdBars: i - entryIdx, exitReason,
                });

                position = 0;
                touchedBollBand = false;
                highestPnl = 0;
                lowestPnl = 0;
                crossBelowCount = 0;
                crossAboveCount = 0;
            }
        }

        // 开仓逻辑
        if (position === 0 && capital > 0 && !liquidated) {
            const longSignal = trend === 'bull' && !openAboveMid && closeAboveMid;
            const shortSignal = trend === 'bear' && openAboveMid && !closeAboveMid;

            if (longSignal || shortSignal) {
                // 过滤假信号
                if (options.filterFakeSignals) {
                    const distToMid = Math.abs(bar.close - currentBollMid);
                    const distPercent = distToMid / bar.close * 100;

                    const isBullish = bar.close > bar.open;
                    const isBearish = bar.close < bar.open;
                    const directionMatch = (longSignal && isBullish) || (shortSignal && isBearish);

                    if (distPercent < 0.1 || !directionMatch) {
                        filteredSignals++;
                        continue;
                    }
                }

                // 开仓
                const leverage = config.leverage || 1;
                const availableCapital = capital * config.position_size_percent * leverage;

                if (longSignal) {
                    const entryPriceWithSlippage = bar.close * (1 + config.slippage_rate);
                    position = availableCapital / entryPriceWithSlippage;
                    entryPrice = entryPriceWithSlippage;
                } else {
                    const entryPriceWithSlippage = bar.close * (1 - config.slippage_rate);
                    position = -availableCapital / entryPriceWithSlippage;
                    entryPrice = entryPriceWithSlippage;
                }
                entryTime = bar.open_time;
                entryIdx = i;
                entryCapital = availableCapital;
                touchedBollBand = false;
                highestPnl = 0;
                lowestPnl = 0;
                crossBelowCount = 0;
                crossAboveCount = 0;
            }
        }
    }

    // 强制平仓
    if (position !== 0 && !liquidated) {
        const lastBar = data[n - 1];
        const exitPrice = lastBar.close;
        const priceDiff = position > 0
            ? exitPrice - entryPrice
            : entryPrice - exitPrice;
        const grossPnl = priceDiff * Math.abs(position);
        const exitCommission = Math.abs(position) * exitPrice * config.commission_rate;
        const entryCommission = Math.abs(position) * entryPrice * config.commission_rate;
        const slippage = Math.abs(position) * exitPrice * config.slippage_rate;
        const totalCost = exitCommission + entryCommission + slippage;
        totalCommission += totalCost;

        const netPnl = grossPnl - totalCost;
        capital += netPnl;

        trades.push({
            type: position > 0 ? 'LONG' : 'SHORT',
            entryTime, entryPrice, exitTime: lastBar.open_time, exitPrice,
            grossPnl, netPnl, totalCost,
            pnlPercent: netPnl / entryCapital * 100,
            holdBars: n - 1 - entryIdx, exitReason: 'force_close',
        });
    }

    return { trades, finalCapital: capital, totalCommission, filteredSignals, liquidated };
}

// ============ 统计函数 ============
function analyze(result, label) {
    const trades = result.trades;
    if (trades.length === 0) return { label, trades: 0, netProfit: 0 };

    const wins = trades.filter(t => t.netPnl > 0);
    const losses = trades.filter(t => t.netPnl <= 0);
    const avgWin = wins.length > 0 ? wins.reduce((s, t) => s + t.netPnl, 0) / wins.length : 0;
    const avgLoss = losses.length > 0 ? losses.reduce((s, t) => s + t.netPnl, 0) / losses.length : 0;
    const profitFactor = avgLoss !== 0 ? Math.abs(avgWin / avgLoss) : Infinity;

    const netProfit = result.finalCapital - 10000;
    const totalReturn = (netProfit / 10000) * 100;

    let capital = 10000, peak = capital, maxDrawdown = 0;
    trades.forEach(t => {
        capital += t.netPnl;
        if (capital > peak) peak = capital;
        const dd = (peak - capital) / peak * 100;
        if (dd > maxDrawdown) maxDrawdown = dd;
    });

    const monthlyData = {};
    trades.forEach(t => {
        const month = t.exitTime.substring(0, 7);
        if (!monthlyData[month]) monthlyData[month] = 0;
        monthlyData[month] += t.netPnl;
    });
    const profitableMonths = Object.values(monthlyData).filter(p => p >= 0).length;

    // 计算年化收益
    const tradingDays = 202; // 7个月约202天
    const annualizedReturn = result.liquidated ? -100 : (Math.pow(1 + totalReturn / 100, 365 / tradingDays) - 1) * 100;

    return {
        label,
        trades: trades.length,
        winRate: wins.length / trades.length,
        profitFactor,
        netProfit,
        totalReturn,
        annualizedReturn,
        maxDrawdown,
        totalCommission: result.totalCommission,
        filteredSignals: result.filteredSignals,
        monthlyProfitRate: profitableMonths / Object.keys(monthlyData).length,
        liquidated: result.liquidated,
        finalCapital: result.finalCapital,
        monthlyData,
    };
}

// ============ 主程序 ============
function main() {
    const csvPath = path.resolve('F:/rust_projects/trade/src/kline_5m_202607232011.csv');
    const data = loadCSV(csvPath);

    console.log('='.repeat(90));
    console.log('10倍杠杆测试');
    console.log('='.repeat(90));
    console.log(`数据: ${data.length}条K线, ${data[0].open_time} ~ ${data[data.length - 1].open_time}\n`);

    // 测试不同杠杆
    const testCases = [
        {
            name: '1倍杠杆(现货)',
            config: { ...CONFIG, leverage: 1 },
            options: { filterFakeSignals: true, consecutiveCross: false },
        },
        {
            name: '2倍杠杆',
            config: { ...CONTRACT_CONFIG, leverage: 2 },
            options: { filterFakeSignals: true, consecutiveCross: false },
        },
        {
            name: '3倍杠杆',
            config: { ...CONTRACT_CONFIG, leverage: 3 },
            options: { filterFakeSignals: true, consecutiveCross: false },
        },
        {
            name: '5倍杠杆',
            config: { ...CONTRACT_CONFIG, leverage: 5 },
            options: { filterFakeSignals: true, consecutiveCross: false },
        },
        {
            name: '10倍杠杆',
            config: { ...CONTRACT_CONFIG, leverage: 10 },
            options: { filterFakeSignals: true, consecutiveCross: false },
        },
        {
            name: '20倍杠杆',
            config: { ...CONTRACT_CONFIG, leverage: 20 },
            options: { filterFakeSignals: true, consecutiveCross: false },
        },
    ];

    const results = [];

    testCases.forEach(tc => {
        const result = backtest(data, tc.config, tc.options);
        const stats = analyze(result, tc.name);
        results.push(stats);
    });

    // 输出对比表格
    console.log('\n' +
        '策略'.padEnd(18) +
        '净利润'.padEnd(12) +
        '收益率'.padEnd(10) +
        '年化收益'.padEnd(10) +
        '最大回撤'.padEnd(10) +
        '胜率'.padEnd(8) +
        '盈亏比'.padEnd(8) +
        '手续费'.padEnd(10) +
        '爆仓'
    );
    console.log('-'.repeat(95));

    results.forEach(r => {
        if (r) {
            const liquidated = r.liquidated ? '💥' : '✅';
            console.log(
                r.label.padEnd(18) +
                `${r.netProfit >= 0 ? '+' : ''}${r.netProfit.toFixed(2)}`.padEnd(12) +
                `${r.totalReturn >= 0 ? '+' : ''}${r.totalReturn.toFixed(2)}%`.padEnd(10) +
                `${r.annualizedReturn >= 0 ? '+' : ''}${r.annualizedReturn.toFixed(1)}%`.padEnd(10) +
                `${r.maxDrawdown.toFixed(2)}%`.padEnd(10) +
                `${(r.winRate * 100).toFixed(1)}%`.padEnd(8) +
                `${r.profitFactor.toFixed(2)}`.padEnd(8) +
                `${r.totalCommission.toFixed(2)}`.padEnd(10) +
                liquidated
            );
        }
    });

    // 找出最佳策略（排除爆仓的）
    const validResults = results.filter(r => !r.liquidated);
    if (validResults.length > 0) {
        validResults.sort((a, b) => b.netProfit - a.netProfit);
        const best = validResults[0];

        console.log('\n' + '='.repeat(90));
        console.log('🏆 最佳杠杆配置');
        console.log('='.repeat(90));
        console.log(`策略: ${best.label}`);
        console.log(`净利润: ${best.netProfit >= 0 ? '+' : ''}${best.netProfit.toFixed(2)} USDT`);
        console.log(`收益率: ${best.totalReturn >= 0 ? '+' : ''}${best.totalReturn.toFixed(2)}%`);
        console.log(`年化收益: ${best.annualizedReturn >= 0 ? '+' : ''}${best.annualizedReturn.toFixed(1)}%`);
        console.log(`最大回撤: ${best.maxDrawdown.toFixed(2)}%`);
        console.log(`胜率: ${(best.winRate * 100).toFixed(1)}%`);
        console.log(`盈亏比: ${best.profitFactor.toFixed(2)}`);
        console.log(`手续费: ${best.totalCommission.toFixed(2)} USDT`);

        // 月度统计
        console.log('\n📅 月度统计:');
        Object.keys(best.monthlyData).sort().forEach(month => {
            const pnl = best.monthlyData[month];
            const emoji = pnl >= 0 ? '✅' : '❌';
            console.log(`  ${emoji} ${month}: ${pnl >= 0 ? '+' : ''}${pnl.toFixed(2)} USDT`);
        });

        // 不同资金规模的收益
        console.log('\n💰 不同资金规模的收益:');
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
    console.log(`\n  爆仓策略: ${liqCount}/${results.length}`);

    if (validResults.length > 0) {
        const best = validResults[0];
        console.log(`\n  推荐使用: ${best.label}`);
        console.log(`  年化收益: ${best.annualizedReturn.toFixed(1)}%`);
        console.log(`  最大回撤: ${best.maxDrawdown.toFixed(2)}%`);

        if (best.annualizedReturn > 10) {
            console.log('\n  ✅ 年化收益超过10%，值得考虑！');
        } else if (best.annualizedReturn > 5) {
            console.log('\n  ⚠️ 年化收益5-10%，需要权衡风险');
        } else {
            console.log('\n  ⚠️ 年化收益较低，需要谨慎考虑');
        }
    }
}

main();
