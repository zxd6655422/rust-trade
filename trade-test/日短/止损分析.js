/**
 * 止损分析 - 分析平仓后价格走势
 * 1. 每次入场后的价格走势
 * 2. 平仓后价格是否继续原方向运动
 * 3. 如果不止损，最终会盈还是亏
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
    // 分析平仓后多少根K线
    follow_up_bars: 50,
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

// ============ 回测并记录详细信息 ============
function backtestWithDetails(data, config) {
    const n = data.length;
    const ma48 = calcSMA(data, config.ma48_period);
    const ma288 = calcSMA(data, config.ma288_period);
    const boll = calcBollinger(data, config.bollinger_period, config.bollinger_std_mult);

    let position = 0, entryPrice = 0, entryTime = '', entryIdx = 0;
    const trades = [];

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
            const exitPrice = bar.close;
            const pnl = exitPrice - entryPrice;
            const holdBars = i - entryIdx;

            // 记录平仓后的价格走势
            const followUp = analyzeFollowUp(data, i, position, config.follow_up_bars);

            trades.push({
                type: 'LONG',
                entryTime, entryPrice, entryIdx,
                exitTime: bar.open_time, exitPrice, exitIdx: i,
                pnl, pnlPercent: pnl / entryPrice * 100, holdBars,
                bollMidAtEntry: boll.middle[entryIdx],
                bollMidAtExit: currentBollMid,
                trendAtEntry: getTrendIntent(ma48, ma288, boll.middle, entryIdx, config.trend_confirm_bars),
                trendAtExit: trend,
                followUp,
            });
            position = 0;
        }
        else if (position === -1 && !openAboveMid && closeAboveMid) {
            const exitPrice = bar.close;
            const pnl = entryPrice - exitPrice;
            const holdBars = i - entryIdx;

            const followUp = analyzeFollowUp(data, i, position, config.follow_up_bars);

            trades.push({
                type: 'SHORT',
                entryTime, entryPrice, entryIdx,
                exitTime: bar.open_time, exitPrice, exitIdx: i,
                pnl, pnlPercent: pnl / entryPrice * 100, holdBars,
                bollMidAtEntry: boll.middle[entryIdx],
                bollMidAtExit: currentBollMid,
                trendAtEntry: getTrendIntent(ma48, ma288, boll.middle, entryIdx, config.trend_confirm_bars),
                trendAtExit: trend,
                followUp,
            });
            position = 0;
        }
    }

    return trades;
}

// ============ 分析平仓后价格走势 ============
function analyzeFollowUp(data, exitIdx, direction, followBars) {
    const result = {
        maxFavorable: 0,    // 最大有利变动
        maxAdverse: 0,      // 最大不利变动
        priceAt5: null,     // 5根K线后价格
        priceAt10: null,    // 10根K线后价格
        priceAt20: null,    // 20根K线后价格
        priceAt50: null,    // 50根K线后价格
        continuedMove: false, // 是否继续原方向运动
    };

    const exitPrice = data[exitIdx].close;
    let maxFavorable = 0, maxAdverse = 0;

    for (let i = 1; i <= followBars; i++) {
        const idx = exitIdx + i;
        if (idx >= data.length) break;

        const price = data[idx].close;
        const diff = direction === 1
            ? price - exitPrice  // 做多，价格上涨有利
            : exitPrice - price; // 做空，价格下跌有利

        if (diff > maxFavorable) maxFavorable = diff;
        if (diff < maxAdverse) maxAdverse = diff;

        // 记录特定时间点的价格
        if (i === 5) result.priceAt5 = price;
        if (i === 10) result.priceAt10 = price;
        if (i === 20) result.priceAt20 = price;
        if (i === 50) result.priceAt50 = price;
    }

    result.maxFavorable = maxFavorable;
    result.maxAdverse = maxAdverse;

    // 判断是否继续原方向运动（平仓后20根K线内最大有利变动 > 最大不利变动）
    result.continuedMove = maxFavorable > Math.abs(maxAdverse) && maxFavorable > 0;

    return result;
}

// ============ 深度分析 ============
function deepAnalysis(trades) {
    console.log('='.repeat(70));
    console.log('【止损/平仓分析报告】');
    console.log('='.repeat(70));
    console.log(`分析范围: 平仓后 ${CONFIG.follow_up_bars} 根K线的价格走势`);

    if (trades.length === 0) {
        console.log('无交易记录');
        return;
    }

    // 1. 基础统计
    const wins = trades.filter(t => t.pnl > 0);
    const losses = trades.filter(t => t.pnl <= 0);

    console.log('\n📊 一、基础统计');
    console.log(`  总交易次数: ${trades.length}`);
    console.log(`  盈利: ${wins.length}笔 (${(wins.length / trades.length * 100).toFixed(1)}%)`);
    console.log(`  亏损: ${losses.length}笔 (${(losses.length / trades.length * 100).toFixed(1)}%)`);

    // 2. 平仓后价格继续原方向运动的统计
    const continuedAfterWin = wins.filter(t => t.followUp.continuedMove).length;
    const continuedAfterLoss = losses.filter(t => t.followUp.continuedMove).length;

    console.log('\n📈 二、平仓后价格走势分析');
    console.log('\n  盈利交易平仓后:');
    console.log(`    继续原方向运动: ${continuedAfterWin}笔 (${(continuedAfterWin / wins.length * 100).toFixed(1)}%)`);
    console.log(`    反转或停滞: ${wins.length - continuedAfterWin}笔 (${((wins.length - continuedAfterWin) / wins.length * 100).toFixed(1)}%)`);

    console.log('\n  亏损交易平仓后:');
    console.log(`    继续原方向运动: ${continuedAfterLoss}笔 (${(continuedAfterLoss / losses.length * 100).toFixed(1)}%)`);
    console.log(`    反转（证明止损正确）: ${losses.length - continuedAfterLoss}笔 (${((losses.length - continuedAfterLoss) / losses.length * 100).toFixed(1)}%)`);

    // 3. 如果不止损的后果分析
    console.log('\n💰 三、如果不止损的后果分析');

    // 分析亏损交易如果不止损的结果
    let ifNoStopLoss = 0;
    let wouldRecover = 0;
    let wouldLoseMore = 0;

    losses.forEach(t => {
        const futurePrice = t.followUp.priceAt20 || t.followUp.priceAt10 || t.followUp.priceAt5;
        if (futurePrice === null) return;

        const ifHeld = t.type === 'LONG'
            ? futurePrice - t.entryPrice
            : t.entryPrice - futurePrice;

        ifNoStopLoss += ifHeld;

        if (ifHeld > 0) wouldRecover++;
        else wouldLoseMore++;
    });

    console.log(`\n  亏损交易（${losses.length}笔）如果持有20根K线后:`);
    console.log(`    能回本盈利: ${wouldRecover}笔 (${(wouldRecover / losses.length * 100).toFixed(1)}%)`);
    console.log(`    亏损扩大: ${wouldLoseMore}笔 (${(wouldLoseMore / losses.length * 100).toFixed(1)}%)`);

    // 计算平仓 vs 持有的盈亏对比
    const actualLoss = losses.reduce((s, t) => s + t.pnl, 0);
    console.log(`\n    实际平仓亏损: ${actualLoss.toFixed(4)} USDT`);
    console.log(`    如果持有20根K线: ${ifNoStopLoss >= 0 ? '+' : ''}${ifNoStopLoss.toFixed(4)} USDT`);
    console.log(`    差异: ${ifNoStopLoss > actualLoss ? '❌ 持有更好' : '✅ 平仓正确'}`);

    // 4. 最大有利/不利变动分析
    console.log('\n📊 四、平仓后最大价格变动');

    const avgMaxFavorable = trades.reduce((s, t) => s + t.followUp.maxFavorable, 0) / trades.length;
    const avgMaxAdverse = trades.reduce((s, t) => s + t.followUp.maxAdverse, 0) / trades.length;

    console.log(`  平均最大有利变动: +${avgMaxFavorable.toFixed(4)} USDT`);
    console.log(`  平均最大不利变动: ${avgMaxAdverse.toFixed(4)} USDT`);

    // 分盈利亏损统计
    const avgFavorableWin = wins.reduce((s, t) => s + t.followUp.maxFavorable, 0) / wins.length;
    const avgAdverseWin = wins.reduce((s, t) => s + t.followUp.maxAdverse, 0) / wins.length;
    const avgFavorableLoss = losses.reduce((s, t) => s + t.followUp.maxFavorable, 0) / losses.length;
    const avgAdverseLoss = losses.reduce((s, t) => s + t.followUp.maxAdverse, 0) / losses.length;

    console.log(`\n  盈利交易:`);
    console.log(`    平均最大有利变动: +${avgFavorableWin.toFixed(4)} USDT`);
    console.log(`    平均最大不利变动: ${avgAdverseWin.toFixed(4)} USDT`);

    console.log(`\n  亏损交易:`);
    console.log(`    平均最大有利变动: +${avgFavorableLoss.toFixed(4)} USDT`);
    console.log(`    平均最大不利变动: ${avgAdverseLoss.toFixed(4)} USDT`);

    // 5. 典型案例分析
    console.log('\n📋 五、典型案例分析');

    // 找出平仓后继续原方向运动最多的交易
    const continuedTrades = trades.filter(t => t.followUp.continuedMove);
    if (continuedTrades.length > 0) {
        // 按最大有利变动排序
        continuedTrades.sort((a, b) => b.followUp.maxFavorable - a.followUp.maxFavorable);

        console.log('\n  ❌ 平仓后继续原方向运动的典型案例（可能过早离场）:');
        continuedTrades.slice(0, 5).forEach((t, i) => {
            console.log(`\n    #${i + 1} ${t.type} ${t.entryTime.substring(5, 16)}`);
            console.log(`      入场: ${t.entryPrice} → 平仓: ${t.exitPrice} (${t.pnl >= 0 ? '+' : ''}${t.pnl.toFixed(4)})`);
            console.log(`      平仓后最大有利变动: +${t.followUp.maxFavorable.toFixed(4)}`);
            console.log(`      如果持有可多赚: +${(t.followUp.maxFavorable - Math.abs(t.pnl)).toFixed(4)}`);
        });
    }

    // 找出平仓后反转的亏损交易
    const reversedLosses = losses.filter(t => !t.followUp.continuedMove);
    if (reversedLosses.length > 0) {
        reversedLosses.sort((a, b) => a.followUp.maxAdverse - b.followUp.maxAdverse);

        console.log('\n  ✅ 亏损平仓后价格反转的案例（止损正确）:');
        reversedLosses.slice(0, 5).forEach((t, i) => {
            console.log(`\n    #${i + 1} ${t.type} ${t.entryTime.substring(5, 16)}`);
            console.log(`      入场: ${t.entryPrice} → 平仓: ${t.exitPrice} (${t.pnl.toFixed(4)})`);
            console.log(`      平仓后最大不利变动: ${t.followUp.maxAdverse.toFixed(4)}`);
            console.log(`      止损避免了: ${t.followUp.maxAdverse.toFixed(4)} 的额外亏损`);
        });
    }

    // 6. 持仓时间与后续走势关系
    console.log('\n⏱️ 六、持仓时间与后续走势关系');

    const shortHold = trades.filter(t => t.holdBars <= 5);
    const mediumHold = trades.filter(t => t.holdBars > 5 && t.holdBars <= 20);
    const longHold = trades.filter(t => t.holdBars > 20);

    [
        { name: '短线 (≤5根K线)', trades: shortHold },
        { name: '中线 (6-20根K线)', trades: mediumHold },
        { name: '长线 (>20根K线)', trades: longHold },
    ].forEach(group => {
        if (group.trades.length === 0) return;
        const continued = group.trades.filter(t => t.followUp.continuedMove).length;
        const winRate = group.trades.filter(t => t.pnl > 0).length / group.trades.length;
        console.log(`\n  ${group.name}:`);
        console.log(`    交易数: ${group.trades.length}`);
        console.log(`    胜率: ${(winRate * 100).toFixed(1)}%`);
        console.log(`    平仓后继续原方向: ${continued}笔 (${(continued / group.trades.length * 100).toFixed(1)}%)`);
    });

    // 7. 价格在不同时间点的走势
    console.log('\n📈 七、平仓后价格走势曲线');

    const timePoints = [5, 10, 20, 50];
    timePoints.forEach(bars => {
        const prices = trades.map(t => {
            if (bars === 5) return t.followUp.priceAt5;
            if (bars === 10) return t.followUp.priceAt10;
            if (bars === 20) return t.followUp.priceAt20;
            return t.followUp.priceAt50;
        }).filter(p => p !== null);

        if (prices.length === 0) return;

        // 计算相对于平仓价的平均变动
        let avgDiff = 0;
        trades.forEach((t, i) => {
            if (prices[i] !== undefined) {
                const diff = t.type === 'LONG'
                    ? prices[i] - t.exitPrice
                    : t.exitPrice - prices[i];
                avgDiff += diff;
            }
        });
        avgDiff /= prices.length;

        const continuedCount = trades.filter(t => {
            if (bars === 5) return t.followUp.priceAt5 !== null;
            if (bars === 10) return t.followUp.priceAt10 !== null;
            if (bars === 20) return t.followUp.priceAt20 !== null;
            return t.followUp.priceAt50 !== null;
        }).length;

        console.log(`  ${bars}根K线后平均变动: ${avgDiff >= 0 ? '+' : ''}${avgDiff.toFixed(4)} USDT (样本: ${continuedCount})`);
    });

    // 8. 结论
    console.log('\n' + '='.repeat(70));
    console.log('📌 结论');
    console.log('='.repeat(70));

    const continuedRate = (continuedAfterWin + continuedAfterLoss) / trades.length;
    const recoveryRate = wouldRecover / losses.length;

    console.log(`\n  平仓后价格继续原方向运动的比例: ${(continuedRate * 100).toFixed(1)}%`);

    if (continuedRate > 0.6) {
        console.log('  ⚠️ 超过60%的交易平仓后价格继续原方向，可能存在过早离场问题');
        console.log('  建议: 考虑增加持仓时间或使用移动止损');
    } else if (continuedRate > 0.4) {
        console.log('  ⚠️ 约40-60%的交易平仓后价格继续原方向，离场时机有待优化');
    } else {
        console.log('  ✅ 大部分交易平仓后价格反转，离场时机较为合理');
    }

    console.log(`\n  亏损交易如果持有20根K线后能回本的比例: ${(recoveryRate * 100).toFixed(1)}%`);
    if (recoveryRate > 0.5) {
        console.log('  ⚠️ 超过50%的亏损交易如果持有能回本，止损可能过早');
    } else {
        console.log('  ✅ 大部分亏损交易止损后继续亏损，止损策略有效');
    }
}

// ============ 主程序 ============
function main() {
    const csvPath = path.resolve('F:/rust_projects/trade/src/kline_5m_202607232011.csv');
    const data = loadCSV(csvPath);

    console.log('日短策略 - 止损/平仓分析');
    console.log(`数据: ${data.length}条K线, ${data[0].open_time} ~ ${data[data.length - 1].open_time}`);
    console.log(`分析平仓后 ${CONFIG.follow_up_bars} 根K线的价格走势\n`);

    const trades = backtestWithDetails(data, CONFIG);
    deepAnalysis(trades);
}

main();
