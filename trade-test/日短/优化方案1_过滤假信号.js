/**
 * 优化方案1: 过滤假信号
 * 增加入场确认条件：收盘价与中轨距离 > 0.1%
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
    leverage: 1,
    commission_rate: 0.0004,
    slippage_rate: 0.0001,
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

    const trades = [];
    let totalCommission = 0;
    let filteredSignals = 0;

    const startIdx = Math.max(config.ma488_period, config.bollinger_period) + config.trend_confirm_bars;

    for (let i = startIdx; i < n; i++) {
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

            if (position > 0) {
                if (unrealizedPnl > highestPnl) highestPnl = unrealizedPnl;
            } else {
                if (unrealizedPnl > lowestPnl) lowestPnl = unrealizedPnl;
            }

            if (position > 0 && bar.high >= currentBollUpper) touchedBollBand = true;
            if (position < 0 && bar.low <= currentBollLower) touchedBollBand = true;

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
                if (position > 0 && openAboveMid && !closeAboveMid) {
                    shouldExit = true;
                    exitReason = 'bollinger_cross';
                } else if (position < 0 && !openAboveMid && closeAboveMid) {
                    shouldExit = true;
                    exitReason = 'bollinger_cross';
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
            }
        }

        // 开仓逻辑
        if (position === 0 && capital > 0) {
            const longSignal = trend === 'bull' && !openAboveMid && closeAboveMid;
            const shortSignal = trend === 'bear' && openAboveMid && !closeAboveMid;

            if (longSignal || shortSignal) {
                // 优化方案1: 过滤假信号
                if (options.filterFakeSignals) {
                    // 条件1: 收盘价与中轨距离 > 0.1%
                    const distToMid = Math.abs(bar.close - currentBollMid);
                    const distPercent = distToMid / bar.close * 100;

                    // 条件2: 收盘价与开盘价方向一致（阳线做多，阴线做空）
                    const isBullish = bar.close > bar.open;
                    const isBearish = bar.close < bar.open;
                    const directionMatch = (longSignal && isBullish) || (shortSignal && isBearish);

                    if (distPercent < 0.1 || !directionMatch) {
                        filteredSignals++;
                        continue; // 过滤掉这个信号
                    }
                }

                // 开仓
                if (longSignal) {
                    const availableCapital = capital * config.position_size_percent * config.leverage;
                    const entryPriceWithSlippage = bar.close * (1 + config.slippage_rate);
                    position = availableCapital / entryPriceWithSlippage;
                    entryPrice = entryPriceWithSlippage;
                    entryTime = bar.open_time;
                    entryIdx = i;
                    entryCapital = availableCapital;
                } else {
                    const availableCapital = capital * config.position_size_percent * config.leverage;
                    const entryPriceWithSlippage = bar.close * (1 - config.slippage_rate);
                    position = -availableCapital / entryPriceWithSlippage;
                    entryPrice = entryPriceWithSlippage;
                    entryTime = bar.open_time;
                    entryIdx = i;
                    entryCapital = availableCapital;
                }
                touchedBollBand = false;
                highestPnl = 0;
                lowestPnl = 0;
            }
        }
    }

    // 强制平仓
    if (position !== 0) {
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

    return { trades, finalCapital: capital, totalCommission, filteredSignals };
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

    // 计算最大回撤
    let capital = 10000, peak = capital, maxDrawdown = 0;
    trades.forEach(t => {
        capital += t.netPnl;
        if (capital > peak) peak = capital;
        const dd = (peak - capital) / peak * 100;
        if (dd > maxDrawdown) maxDrawdown = dd;
    });

    // 月度统计
    const monthlyData = {};
    trades.forEach(t => {
        const month = t.exitTime.substring(0, 7);
        if (!monthlyData[month]) monthlyData[month] = 0;
        monthlyData[month] += t.netPnl;
    });
    const profitableMonths = Object.values(monthlyData).filter(p => p >= 0).length;

    return {
        label,
        trades: trades.length,
        winRate: wins.length / trades.length,
        profitFactor,
        netProfit,
        totalReturn,
        maxDrawdown,
        totalCommission: result.totalCommission,
        filteredSignals: result.filteredSignals,
        monthlyProfitRate: profitableMonths / Object.keys(monthlyData).length,
    };
}

// ============ 主程序 ============
function main() {
    const csvPath = path.resolve('F:/rust_projects/trade/src/kline_5m_202607232011.csv');
    const data = loadCSV(csvPath);

    console.log('='.repeat(80));
    console.log('优化方案1: 过滤假信号测试');
    console.log('='.repeat(80));
    console.log(`数据: ${data.length}条K线\n`);

    // 测试原版
    const original = backtest(data, CONFIG, { filterFakeSignals: false });
    const originalStats = analyze(original, '原版策略');

    // 测试优化方案1
    const optimized = backtest(data, CONFIG, { filterFakeSignals: true });
    const optimizedStats = analyze(optimized, '优化方案1');

    // 输出对比
    console.log('\n' +
        '策略'.padEnd(20) +
        '交易'.padEnd(6) +
        '胜率'.padEnd(8) +
        '盈亏比'.padEnd(8) +
        '净利润'.padEnd(12) +
        '收益率'.padEnd(10) +
        '最大回撤'.padEnd(10) +
        '手续费'.padEnd(10) +
        '过滤信号'
    );
    console.log('-'.repeat(90));

    [originalStats, optimizedStats].forEach(r => {
        if (r) {
            console.log(
                r.label.padEnd(20) +
                `${r.trades}`.padEnd(6) +
                `${(r.winRate * 100).toFixed(1)}%`.padEnd(8) +
                `${r.profitFactor.toFixed(2)}`.padEnd(8) +
                `${r.netProfit >= 0 ? '+' : ''}${r.netProfit.toFixed(2)}`.padEnd(12) +
                `${r.totalReturn >= 0 ? '+' : ''}${r.totalReturn.toFixed(2)}%`.padEnd(10) +
                `${r.maxDrawdown.toFixed(2)}%`.padEnd(10) +
                `${r.totalCommission.toFixed(2)}`.padEnd(10) +
                `${r.filteredSignals || 0}`
            );
        }
    });

    // 结论
    console.log('\n' + '='.repeat(80));
    console.log('📌 优化方案1效果');
    console.log('='.repeat(80));

    const profitDiff = optimizedStats.netProfit - originalStats.netProfit;
    const winRateDiff = optimizedStats.winRate - originalStats.winRate;

    console.log(`\n  净利润变化: ${profitDiff >= 0 ? '+' : ''}${profitDiff.toFixed(2)} USDT`);
    console.log(`  胜率变化: ${winRateDiff >= 0 ? '+' : ''}${(winRateDiff * 100).toFixed(1)}%`);
    console.log(`  交易次数: ${originalStats.trades} → ${optimizedStats.trades}`);
    console.log(`  过滤信号: ${optimizedStats.filteredSignals}笔`);

    if (profitDiff > 0) {
        console.log('\n  ✅ 优化方案1有效！');
    } else {
        console.log('\n  ⚠️ 优化方案1效果不明显');
    }
}

main();
