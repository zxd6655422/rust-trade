/**
 * 资金管理回测 - 完整的回测系统
 * 包含：初始资金、仓位管理、手续费、滑点
 */

const fs = require('fs');
const path = require('path');

// ============ 配置 ============
const CONFIG = {
    // 策略参数
    ma48_period: 48,
    ma288_period: 288,
    ma488_period: 488,
    bollinger_period: 100,
    bollinger_std_mult: 2.0,
    trend_confirm_bars: 3,

    // 资金管理参数
    initial_capital: 10000,      // 初始资金 10000 USDT
    position_size_percent: 0.1,  // 每次开仓使用10%资金
    leverage: 1,                 // 杠杆倍数（1倍=不加杠杆）

    // 交易成本
    commission_rate: 0.0004,     // 手续费 0.04%（Binance现货taker）
    slippage_rate: 0.0001,       // 滑点 0.01%

    // 策略模式
    strategy_mode: 'trailing_with_rebound', // 使用最佳策略
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
            trade_count: parseInt(values[7]) || 0,
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

// ============ 完整回测引擎 ============
function backtest(data, config) {
    const n = data.length;
    const ma48 = calcSMA(data, config.ma48_period);
    const ma288 = calcSMA(data, config.ma288_period);
    const boll = calcBollinger(data, config.bollinger_period, config.bollinger_std_mult);

    let capital = config.initial_capital;
    let position = 0; // 持仓数量（正数=多，负数=空）
    let entryPrice = 0, entryTime = '', entryIdx = 0;
    let entryCapital = 0; // 入场时使用的资金
    let touchedBollBand = false;
    let highestPnl = 0, lowestPnl = 0;

    const trades = [];
    const equityCurve = []; // 资金曲线
    let totalCommission = 0;

    const startIdx = Math.max(config.ma488_period, config.bollinger_period) + config.trend_confirm_bars;

    for (let i = startIdx; i < n; i++) {
        const bar = data[i];
        const currentMa48 = ma48[i];
        const currentMa288 = ma288[i];
        const currentBollMid = boll.middle[i];
        const currentBollUpper = boll.upper[i];
        const currentBollLower = boll.lower[i];

        if (currentMa48 === null || currentMa288 === null || currentBollMid === null) {
            equityCurve.push({ time: bar.open_time, equity: capital });
            continue;
        }

        const trend = getTrendIntent(ma48, ma288, boll.middle, i, config.trend_confirm_bars);
        const openAboveMid = bar.open > currentBollMid;
        const closeAboveMid = bar.close > currentBollMid;

        // 计算当前持仓的浮动盈亏
        let unrealizedPnl = 0;
        if (position !== 0) {
            const currentPrice = bar.close;
            const priceDiff = position > 0
                ? currentPrice - entryPrice
                : entryPrice - currentPrice;
            unrealizedPnl = priceDiff * Math.abs(position);
        }

        // 记录资金曲线
        equityCurve.push({
            time: bar.open_time,
            equity: capital + unrealizedPnl,
        });

        // 持仓中
        if (position !== 0) {
            const currentPnlPercent = unrealizedPnl / entryCapital * 100;

            // 更新最高/最低盈利
            if (position > 0) {
                if (unrealizedPnl > highestPnl) highestPnl = unrealizedPnl;
            } else {
                if (unrealizedPnl > lowestPnl) lowestPnl = unrealizedPnl;
            }

            // 检查是否触及布林轨
            if (position > 0 && bar.high >= currentBollUpper) {
                touchedBollBand = true;
            }
            if (position < 0 && bar.low <= currentBollLower) {
                touchedBollBand = true;
            }

            // 离场逻辑
            let shouldExit = false;
            let exitReason = '';

            // 布林轨反弹离场
            if (touchedBollBand && unrealizedPnl > 0) {
                const distToMid = Math.abs(bar.close - currentBollMid);
                const bollWidth = currentBollUpper - currentBollLower;
                const distPercent = distToMid / bollWidth;

                // 盈利超过1%且反弹到中轨附近
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

                // 计算手续费（开仓+平仓）
                const exitCommission = Math.abs(position) * exitPrice * config.commission_rate;
                const entryCommission = Math.abs(position) * entryPrice * config.commission_rate;
                const slippage = Math.abs(position) * exitPrice * config.slippage_rate;
                const totalCost = exitCommission + entryCommission + slippage;
                totalCommission += totalCost;

                const netPnl = grossPnl - totalCost;
                const holdBars = i - entryIdx;

                capital += netPnl;

                trades.push({
                    type: position > 0 ? 'LONG' : 'SHORT',
                    entryTime, entryPrice,
                    exitTime: bar.open_time, exitPrice,
                    positionSize: Math.abs(position),
                    grossPnl, netPnl, totalCost,
                    pnlPercent: netPnl / entryCapital * 100,
                    holdBars, exitReason,
                    capitalAfter: capital,
                });

                position = 0;
                touchedBollBand = false;
                highestPnl = 0;
                lowestPnl = 0;
            }
        }

        // 开仓逻辑
        if (position === 0) {
            if (trend === 'bull' && !openAboveMid && closeAboveMid) {
                // 做多
                const availableCapital = capital * config.position_size_percent * config.leverage;
                const entryPriceWithSlippage = bar.close * (1 + config.slippage_rate);
                position = availableCapital / entryPriceWithSlippage;
                entryPrice = entryPriceWithSlippage;
                entryTime = bar.open_time;
                entryIdx = i;
                entryCapital = availableCapital;
                touchedBollBand = false;
                highestPnl = 0;
                lowestPnl = 0;
            } else if (trend === 'bear' && openAboveMid && !closeAboveMid) {
                // 做空
                const availableCapital = capital * config.position_size_percent * config.leverage;
                const entryPriceWithSlippage = bar.close * (1 - config.slippage_rate);
                position = -availableCapital / entryPriceWithSlippage;
                entryPrice = entryPriceWithSlippage;
                entryTime = bar.open_time;
                entryIdx = i;
                entryCapital = availableCapital;
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
            entryTime, entryPrice,
            exitTime: lastBar.open_time, exitPrice,
            positionSize: Math.abs(position),
            grossPnl, netPnl, totalCost,
            pnlPercent: netPnl / entryCapital * 100,
            holdBars: n - 1 - entryIdx,
            exitReason: 'force_close',
            capitalAfter: capital,
        });
    }

    return { trades, finalCapital: capital, totalCommission, equityCurve };
}

// ============ 分析报告 ============
function analyze(result, config) {
    const trades = result.trades;
    if (trades.length === 0) {
        console.log('无交易记录');
        return;
    }

    const wins = trades.filter(t => t.netPnl > 0);
    const losses = trades.filter(t => t.netPnl <= 0);

    console.log('='.repeat(80));
    console.log('【资金管理回测报告】');
    console.log('='.repeat(80));

    // 1. 资金概况
    console.log('\n💰 一、资金概况');
    console.log(`  初始资金: ${config.initial_capital.toFixed(2)} USDT`);
    console.log(`  最终资金: ${result.finalCapital.toFixed(2)} USDT`);
    console.log(`  总收益: ${result.finalCapital - config.initial_capital >= 0 ? '+' : ''}${(result.finalCapital - config.initial_capital).toFixed(2)} USDT`);
    console.log(`  收益率: ${((result.finalCapital / config.initial_capital - 1) * 100).toFixed(2)}%`);
    console.log(`  总手续费: ${result.totalCommission.toFixed(2)} USDT`);

    // 2. 交易统计
    console.log('\n📊 二、交易统计');
    console.log(`  总交易次数: ${trades.length}`);
    console.log(`  盈利: ${wins.length}笔 (${(wins.length / trades.length * 100).toFixed(1)}%)`);
    console.log(`  亏损: ${losses.length}笔 (${(losses.length / trades.length * 100).toFixed(1)}%)`);

    // 3. 盈亏分析
    const totalGrossPnl = trades.reduce((s, t) => s + t.grossPnl, 0);
    const totalNetPnl = trades.reduce((s, t) => s + t.netPnl, 0);
    const avgWin = wins.length > 0 ? wins.reduce((s, t) => s + t.netPnl, 0) / wins.length : 0;
    const avgLoss = losses.length > 0 ? losses.reduce((s, t) => s + t.netPnl, 0) / losses.length : 0;
    const profitFactor = avgLoss !== 0 ? Math.abs(avgWin / avgLoss) : Infinity;

    console.log('\n📈 三、盈亏分析');
    console.log(`  毛利: ${totalGrossPnl >= 0 ? '+' : ''}${totalGrossPnl.toFixed(2)} USDT`);
    console.log(`  净利: ${totalNetPnl >= 0 ? '+' : ''}${totalNetPnl.toFixed(2)} USDT`);
    console.log(`  平均盈利: +${avgWin.toFixed(2)} USDT`);
    console.log(`  平均亏损: ${avgLoss.toFixed(2)} USDT`);
    console.log(`  盈亏比: ${profitFactor.toFixed(2)}`);

    // 4. 最大回撤
    let peak = config.initial_capital, maxDrawdown = 0, maxDrawdownPercent = 0;
    result.equityCurve.forEach(point => {
        if (point.equity > peak) peak = point.equity;
        const dd = peak - point.equity;
        const ddPercent = dd / peak * 100;
        if (dd > maxDrawdown) {
            maxDrawdown = dd;
            maxDrawdownPercent = ddPercent;
        }
    });

    console.log('\n📉 四、风险指标');
    console.log(`  最大回撤: ${maxDrawdown.toFixed(2)} USDT (${maxDrawdownPercent.toFixed(2)}%)`);

    // 5. 月度统计
    console.log('\n📅 五、月度统计');
    const monthlyData = {};
    trades.forEach(t => {
        const month = t.exitTime.substring(0, 7);
        if (!monthlyData[month]) monthlyData[month] = { pnl: 0, count: 0, wins: 0 };
        monthlyData[month].pnl += t.netPnl;
        monthlyData[month].count++;
        if (t.netPnl > 0) monthlyData[month].wins++;
    });

    let profitableMonths = 0;
    Object.keys(monthlyData).sort().forEach(month => {
        const d = monthlyData[month];
        const emoji = d.pnl >= 0 ? '✅' : '❌';
        if (d.pnl >= 0) profitableMonths++;
        console.log(`  ${emoji} ${month}: ${d.count}笔, ${d.pnl >= 0 ? '+' : ''}${d.pnl.toFixed(2)} USDT, 胜率${(d.wins / d.count * 100).toFixed(0)}%`);
    });
    console.log(`  盈利月份: ${profitableMonths}/${Object.keys(monthlyData).length}`);

    // 6. 出场原因分析
    console.log('\n📊 六、出场原因分析');
    const exitReasons = {};
    trades.forEach(t => {
        if (!exitReasons[t.exitReason]) exitReasons[t.exitReason] = { count: 0, pnl: 0, wins: 0 };
        exitReasons[t.exitReason].count++;
        exitReasons[t.exitReason].pnl += t.netPnl;
        if (t.netPnl > 0) exitReasons[t.exitReason].wins++;
    });

    Object.entries(exitReasons).forEach(([reason, data]) => {
        const winRate = data.count > 0 ? (data.wins / data.count * 100).toFixed(1) : 0;
        console.log(`  ${reason}: ${data.count}笔, 胜率${winRate}%, 盈亏${data.pnl >= 0 ? '+' : ''}${data.pnl.toFixed(2)} USDT`);
    });

    // 7. 最近10笔交易
    console.log('\n📋 七、最近10笔交易');
    trades.slice(-10).forEach((t, i) => {
        const idx = trades.length - 10 + i + 1;
        const emoji = t.netPnl > 0 ? '✅' : '❌';
        console.log(`  ${emoji} #${idx} ${t.type.padEnd(5)} ${t.entryTime.substring(5, 16)} → ${t.exitTime.substring(5, 16)} | ${t.netPnl >= 0 ? '+' : ''}${t.netPnl.toFixed(2)} USDT (${t.pnlPercent >= 0 ? '+' : ''}${t.pnlPercent.toFixed(2)}%)`);
    });

    // 8. 资金曲线关键点
    console.log('\n📈 八、资金曲线关键点');
    const minEquity = Math.min(...result.equityCurve.map(e => e.equity));
    const maxEquity = Math.max(...result.equityCurve.map(e => e.equity));
    console.log(`  最低资金: ${minEquity.toFixed(2)} USDT`);
    console.log(`  最高资金: ${maxEquity.toFixed(2)} USDT`);

    // 9. 年化收益率估算
    const tradingDays = result.equityCurve.length > 0
        ? (new Date(result.equityCurve[result.equityCurve.length - 1].time) - new Date(result.equityCurve[0].time)) / (1000 * 60 * 60 * 24)
        : 0;
    const annualizedReturn = tradingDays > 0
        ? (Math.pow(result.finalCapital / config.initial_capital, 365 / tradingDays) - 1) * 100
        : 0;

    console.log('\n📊 九、年化收益估算');
    console.log(`  交易天数: ${tradingDays.toFixed(0)}天`);
    console.log(`  年化收益率: ${annualizedReturn.toFixed(2)}%`);

    // 10. 结论
    console.log('\n' + '='.repeat(80));
    console.log('📌 结论');
    console.log('='.repeat(80));
    console.log(`\n  使用 ${config.initial_capital} USDT 初始资金:`);
    console.log(`  - 7个月收益: ${(result.finalCapital - config.initial_capital).toFixed(2)} USDT`);
    console.log(`  - 收益率: ${((result.finalCapital / config.initial_capital - 1) * 100).toFixed(2)}%`);
    console.log(`  - 年化收益: ${annualizedReturn.toFixed(2)}%`);
    console.log(`  - 最大回撤: ${maxDrawdownPercent.toFixed(2)}%`);
    console.log(`  - 总手续费: ${result.totalCommission.toFixed(2)} USDT`);
}

// ============ 主程序 ============
function main() {
    const csvPath = path.resolve('F:/rust_projects/trade/src/kline_5m_202607232011.csv');
    const data = loadCSV(csvPath);

    console.log('日短策略 - 资金管理回测');
    console.log(`数据: ${data.length}条K线, ${data[0].open_time} ~ ${data[data.length - 1].open_time}\n`);
    console.log(`策略配置:`);
    console.log(`  初始资金: ${CONFIG.initial_capital} USDT`);
    console.log(`  仓位比例: ${CONFIG.position_size_percent * 100}%`);
    console.log(`  杠杆: ${CONFIG.leverage}倍`);
    console.log(`  手续费: ${CONFIG.commission_rate * 100}%`);
    console.log(`  滑点: ${CONFIG.slippage_rate * 100}%`);

    const result = backtest(data, CONFIG);
    analyze(result, CONFIG);
}

main();
