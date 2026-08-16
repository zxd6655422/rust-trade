/**
 * 日短策略深度分析
 * 重点分析：盈亏比、最大回撤、连续亏损、收益分布等
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

// ============ 回测引擎 ============
function backtest(data, config) {
    const n = data.length;
    const ma48 = calcSMA(data, config.ma48_period);
    const ma288 = calcSMA(data, config.ma288_period);
    const boll = calcBollinger(data, config.bollinger_period, config.bollinger_std_mult);

    let position = 0, entryPrice = 0, entryTime = '', entryIdx = 0;
    const trades = [];
    let totalPnl = 0, winCount = 0, lossCount = 0;

    const startIdx = Math.max(config.ma488_period, config.bollinger_period) + config.trend_confirm_bars;

    for (let i = startIdx; i < n; i++) {
        const bar = data[i];
        const currentMa48 = ma48[i];
        const currentMa288 = ma288[i];
        const currentBollMid = boll.middle[i];

        if (currentMa48 === null || currentMa288 === null || currentBollMid === null) continue;

        const trend = getTrendIntent(ma48, ma288, boll.middle, i, config.trend_confirm_bars);
        const openAboveMid = bar.open > currentBollMid;
        const closeAboveMid = bar.close > currentBollMid;

        // 开仓
        if (position === 0) {
            if (trend === 'bull' && !openAboveMid && closeAboveMid) {
                position = 1;
                entryPrice = bar.close;
                entryTime = bar.open_time;
                entryIdx = i;
            } else if (trend === 'bear' && openAboveMid && !closeAboveMid) {
                position = -1;
                entryPrice = bar.close;
                entryTime = bar.open_time;
                entryIdx = i;
            }
        }
        // 平仓
        else if (position === 1 && openAboveMid && !closeAboveMid) {
            const pnl = bar.close - entryPrice;
            const holdBars = i - entryIdx;
            totalPnl += pnl;
            if (pnl > 0) winCount++; else lossCount++;

            trades.push({
                type: 'LONG', entryTime, entryPrice,
                exitTime: bar.open_time, exitPrice: bar.close,
                pnl, pnlPercent: pnl / entryPrice * 100, holdBars,
            });
            position = 0;
        }
        else if (position === -1 && !openAboveMid && closeAboveMid) {
            const pnl = entryPrice - bar.close;
            const holdBars = i - entryIdx;
            totalPnl += pnl;
            if (pnl > 0) winCount++; else lossCount++;

            trades.push({
                type: 'SHORT', entryTime, entryPrice,
                exitTime: bar.open_time, exitPrice: bar.close,
                pnl, pnlPercent: pnl / entryPrice * 100, holdBars,
            });
            position = 0;
        }
    }

    // 强制平仓
    if (position !== 0) {
        const lastBar = data[n - 1];
        const pnl = position === 1 ? lastBar.close - entryPrice : entryPrice - lastBar.close;
        totalPnl += pnl;
        if (pnl > 0) winCount++; else lossCount++;
        trades.push({
            type: position === 1 ? 'LONG' : 'SHORT', entryTime, entryPrice,
            exitTime: lastBar.open_time, exitPrice: lastBar.close,
            pnl, pnlPercent: pnl / entryPrice * 100, holdBars: n - 1 - entryIdx,
            note: '强制平仓',
        });
    }

    return { trades, totalPnl, winCount, lossCount };
}

// ============ 深度统计分析 ============
function deepAnalysis(trades, totalPnl, winCount, lossCount, data) {
    if (trades.length === 0) {
        console.log('无交易记录');
        return;
    }

    // 1. 基础统计
    const wins = trades.filter(t => t.pnl > 0);
    const losses = trades.filter(t => t.pnl <= 0);
    const longTrades = trades.filter(t => t.type === 'LONG');
    const shortTrades = trades.filter(t => t.type === 'SHORT');

    const avgWin = wins.length > 0 ? wins.reduce((s, t) => s + t.pnl, 0) / wins.length : 0;
    const avgLoss = losses.length > 0 ? losses.reduce((s, t) => s + t.pnl, 0) / losses.length : 0;
    const profitFactor = avgLoss !== 0 ? Math.abs(avgWin / avgLoss) : Infinity;

    console.log('='.repeat(70));
    console.log('【日短策略深度分析报告】');
    console.log('='.repeat(70));

    console.log('\n📊 一、基础统计');
    console.log(`  总交易次数: ${trades.length}`);
    console.log(`  做多交易: ${longTrades.length} 笔`);
    console.log(`  做空交易: ${shortTrades.length} 笔`);
    console.log(`  盈利次数: ${winCount} (${(winCount / trades.length * 100).toFixed(2)}%)`);
    console.log(`  亏损次数: ${lossCount} (${(lossCount / trades.length * 100).toFixed(2)}%)`);

    console.log('\n📈 二、盈亏分析');
    console.log(`  总盈亏: ${totalPnl.toFixed(4)} USDT`);
    console.log(`  平均盈利: ${avgWin.toFixed(4)} USDT`);
    console.log(`  平均亏损: ${avgLoss.toFixed(4)} USDT`);
    console.log(`  盈亏比: ${profitFactor.toFixed(2)} (平均盈利/平均亏损)`);
    console.log(`  期望收益: ${(totalPnl / trades.length).toFixed(4)} USDT/笔`);

    // 2. 最大单笔盈亏
    const maxWin = Math.max(...wins.map(t => t.pnl));
    const maxLoss = Math.min(...losses.map(t => t.pnl));
    console.log(`  最大单笔盈利: ${maxWin.toFixed(4)} USDT`);
    console.log(`  最大单笔亏损: ${maxLoss.toFixed(4)} USDT`);

    // 3. 持仓时间分析
    const avgHoldBars = trades.reduce((s, t) => s + t.holdBars, 0) / trades.length;
    const avgHoldMinutes = avgHoldBars * 5;
    console.log(`  平均持仓: ${avgHoldBars.toFixed(1)} 根K线 (${avgHoldMinutes.toFixed(0)} 分钟)`);

    // 4. 连续盈亏分析
    let maxConsecWin = 0, maxConsecLoss = 0, curWin = 0, curLoss = 0;
    trades.forEach(t => {
        if (t.pnl > 0) {
            curWin++;
            curLoss = 0;
            maxConsecWin = Math.max(maxConsecWin, curWin);
        } else {
            curLoss++;
            curWin = 0;
            maxConsecLoss = Math.max(maxConsecLoss, curLoss);
        }
    });
    console.log(`  最大连续盈利: ${maxConsecWin} 次`);
    console.log(`  最大连续亏损: ${maxConsecLoss} 次`);

    // 5. 最大回撤计算
    let peak = 0, maxDrawdown = 0, currentPnl = 0;
    const equityCurve = [];
    trades.forEach(t => {
        currentPnl += t.pnl;
        equityCurve.push(currentPnl);
        if (currentPnl > peak) peak = currentPnl;
        const dd = peak - currentPnl;
        if (dd > maxDrawdown) maxDrawdown = dd;
    });
    console.log(`  最大回撤: ${maxDrawdown.toFixed(4)} USDT`);

    // 6. 月度统计
    console.log('\n📅 三、月度统计');
    const monthlyData = {};
    trades.forEach(t => {
        const month = t.exitTime.substring(0, 7); // YYYY-MM
        if (!monthlyData[month]) monthlyData[month] = { pnl: 0, count: 0, wins: 0 };
        monthlyData[month].pnl += t.pnl;
        monthlyData[month].count++;
        if (t.pnl > 0) monthlyData[month].wins++;
    });

    Object.keys(monthlyData).sort().forEach(month => {
        const d = monthlyData[month];
        const wr = d.count > 0 ? (d.wins / d.count * 100).toFixed(1) : 0;
        console.log(`  ${month}: ${d.count}笔, 盈亏${d.pnl >= 0 ? '+' : ''}${d.pnl.toFixed(4)}, 胜率${wr}%`);
    });

    // 7. 收益分布
    console.log('\n📊 四、收益分布');
    const ranges = [
        { label: '< -1%', min: -Infinity, max: -1 },
        { label: '-1% ~ -0.5%', min: -1, max: -0.5 },
        { label: '-0.5% ~ 0%', min: -0.5, max: 0 },
        { label: '0% ~ 0.5%', min: 0, max: 0.5 },
        { label: '0.5% ~ 1%', min: 0.5, max: 1 },
        { label: '> 1%', min: 1, max: Infinity },
    ];

    ranges.forEach(r => {
        const count = trades.filter(t => t.pnlPercent >= r.min && t.pnlPercent < r.max).length;
        const bar = '█'.repeat(Math.ceil(count / 2));
        console.log(`  ${r.label.padEnd(15)}: ${count.toString().padStart(3)} ${bar}`);
    });

    // 8. 按类型分析
    console.log('\n📈 五、多空分析');
    ['LONG', 'SHORT'].forEach(type => {
        const typeTrades = trades.filter(t => t.type === type);
        const typeWins = typeTrades.filter(t => t.pnl > 0);
        const typePnl = typeTrades.reduce((s, t) => s + t.pnl, 0);
        console.log(`  ${type}: ${typeTrades.length}笔, 盈利${typeWins.length}笔, 胜率${(typeWins.length / typeTrades.length * 100).toFixed(1)}%, 盈亏${typePnl >= 0 ? '+' : ''}${typePnl.toFixed(4)}`);
    });

    // 9. 最近20笔交易
    console.log('\n📋 六、最近20笔交易');
    const recent = trades.slice(-20);
    recent.forEach((t, i) => {
        const idx = trades.length - 20 + i + 1;
        const emoji = t.pnl > 0 ? '✅' : '❌';
        console.log(`  ${emoji} #${idx} ${t.type.padEnd(5)} ${t.entryTime.substring(5, 16)} → ${t.exitTime.substring(5, 16)} | ${t.pnl >= 0 ? '+' : ''}${t.pnl.toFixed(4)} (${t.pnlPercent >= 0 ? '+' : ''}${t.pnlPercent.toFixed(2)}%)`);
    });

    // 10. 策略评估
    console.log('\n' + '='.repeat(70));
    console.log('🔍 策略评估');
    console.log('='.repeat(70));

    const issues = [];
    const strengths = [];

    // 评估标准
    if (winCount / trades.length < 0.3) {
        issues.push(`胜率过低 (${(winCount / trades.length * 100).toFixed(1)}%)，需要更高的盈亏比补偿`);
    }
    if (profitFactor < 1.5) {
        issues.push(`盈亏比不足 (${profitFactor.toFixed(2)})，建议 > 1.5`);
    }
    if (maxConsecLoss > 10) {
        issues.push(`最大连续亏损 ${maxConsecLoss} 次，对心态影响大`);
    }
    if (maxDrawdown > 10) {
        issues.push(`最大回撤 ${maxDrawdown.toFixed(2)} USDT，风险较高`);
    }

    if (totalPnl > 0) strengths.push('总体盈利');
    if (profitFactor > 2) strengths.push('优秀的盈亏比');
    if (maxConsecLoss < 8) strengths.push('连续亏损可控');

    console.log('\n⚠️  潜在问题:');
    if (issues.length > 0) {
        issues.forEach(i => console.log(`  • ${i}`));
    } else {
        console.log('  无明显问题');
    }

    console.log('\n✅ 优势:');
    if (strengths.length > 0) {
        strengths.forEach(s => console.log(`  • ${s}`));
    } else {
        console.log('  无明显优势');
    }

    // 最终结论
    console.log('\n' + '='.repeat(70));
    console.log('📌 结论');
    console.log('='.repeat(70));

    const isViable = totalPnl > 0 && profitFactor > 1.2 && winCount / trades.length > 0.15;

    if (isViable) {
        console.log('策略具有一定可行性，但需要注意：');
        console.log('  1. 胜率较低，需要严格执行止损');
        console.log('  2. 建议先小仓位实盘验证');
        console.log('  3. 关注市场环境变化，趋势策略在震荡市表现差');
    } else {
        console.log('策略存在较大风险，建议：');
        console.log('  1. 优化入场条件，提高胜率');
        console.log('  2. 增加止损机制');
        console.log('  3. 考虑过滤震荡行情');
    }
}

// ============ 主程序 ============
function main() {
    const csvPath = path.resolve('F:/rust_projects/trade/src/kline_5m_202607232011.csv');
    const data = loadCSV(csvPath);

    console.log(`数据: ${data.length} 条K线, ${data[0].open_time} ~ ${data[data.length - 1].open_time}\n`);

    const result = backtest(data, CONFIG);
    deepAnalysis(result.trades, result.totalPnl, result.winCount, result.lossCount, data);
}

main();
