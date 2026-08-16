/**
 * 穿越中轨止损分析
 * 分析141笔止损交易的共同特点
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
    let touchedBollBand = false;
    let highestPnl = 0, lowestPnl = 0;
    let maxFavorableInFirst5 = 0, maxAdverseInFirst5 = 0; // 前5根K线的最大有利/不利变动

    const trades = [];

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
            const currentPnl = position === 1
                ? bar.close - entryPrice
                : entryPrice - bar.close;
            const currentPnlPercent = currentPnl / entryPrice * 100;
            const holdBars = i - entryIdx;

            // 更新最高/最低盈利
            if (position === 1) {
                if (currentPnl > highestPnl) highestPnl = currentPnl;
            } else {
                if (currentPnl > lowestPnl) lowestPnl = currentPnl;
            }

            // 记录前5根K线的最大有利/不利变动
            if (holdBars <= 5) {
                if (position === 1) {
                    const favorable = bar.high - entryPrice;
                    const adverse = entryPrice - bar.low;
                    if (favorable > maxFavorableInFirst5) maxFavorableInFirst5 = favorable;
                    if (adverse > maxAdverseInFirst5) maxAdverseInFirst5 = adverse;
                } else {
                    const favorable = entryPrice - bar.low;
                    const adverse = bar.high - entryPrice;
                    if (favorable > maxFavorableInFirst5) maxFavorableInFirst5 = favorable;
                    if (adverse > maxAdverseInFirst5) maxAdverseInFirst5 = adverse;
                }
            }

            // 检查是否触及布林轨
            if (position === 1 && bar.high >= currentBollUpper) touchedBollBand = true;
            if (position === -1 && bar.low <= currentBollLower) touchedBollBand = true;

            // 布林轨反弹离场
            if (touchedBollBand && currentPnl > 0) {
                const distToMid = Math.abs(bar.close - currentBollMid);
                const bollWidth = currentBollUpper - currentBollLower;
                const distPercent = distToMid / bollWidth;

                if (distPercent < 0.3 && currentPnlPercent > 1) {
                    position = 0;
                    touchedBollBand = false;
                    highestPnl = 0;
                    lowestPnl = 0;
                    maxFavorableInFirst5 = 0;
                    maxAdverseInFirst5 = 0;
                    continue;
                }
            }

            // 移动止盈
            const peakPnl = position === 1 ? highestPnl : lowestPnl;
            const peakPnlPercent = peakPnl / entryPrice * 100;
            if (peakPnlPercent > 2) {
                const drawdown = peakPnl - currentPnl;
                const drawdownPercent = drawdown / entryPrice * 100;
                if (drawdownPercent > 0.5) {
                    position = 0;
                    touchedBollBand = false;
                    highestPnl = 0;
                    lowestPnl = 0;
                    maxFavorableInFirst5 = 0;
                    maxAdverseInFirst5 = 0;
                    continue;
                }
            }

            // 穿越中轨离场（重点分析这部分）
            if (position === 1 && openAboveMid && !closeAboveMid) {
                const exitPrice = bar.close;
                const pnl = exitPrice - entryPrice;
                const pnlPercent = pnl / entryPrice * 100;

                // 计算入场价与中轨的距离
                const entryToBollMid = Math.abs(entryPrice - boll.middle[entryIdx]);
                const entryToBollMidPercent = entryToBollMid / entryPrice * 100;

                // 计算入场时的布林带宽度
                const bollWidthAtEntry = boll.upper[entryIdx] - boll.lower[entryIdx];
                const bollWidthPercent = bollWidthAtEntry / entryPrice * 100;

                // 计算入场后是否触及布林轨
                let touchedUpperBeforeExit = false;
                for (let j = entryIdx; j <= i; j++) {
                    if (data[j].high >= boll.upper[j]) {
                        touchedUpperBeforeExit = true;
                        break;
                    }
                }

                // 计算入场后第一根K线的方向
                const firstBarAfterEntry = data[entryIdx + 1];
                const firstBarDirection = firstBarAfterEntry ? (firstBarAfterEntry.close > entryPrice ? 'favorable' : 'adverse') : 'unknown';

                // 计算入场时的趋势强度
                const ma48AtEntry = ma48[entryIdx];
                const ma288AtEntry = ma288[entryIdx];
                const trendStrength = ma48AtEntry && ma288AtEntry ? (ma48AtEntry - ma288AtEntry) / ma288AtEntry * 100 : 0;

                trades.push({
                    type: 'LONG',
                    entryTime, entryPrice, entryIdx,
                    exitTime: bar.open_time, exitPrice, exitIdx: i,
                    pnl, pnlPercent, holdBars,
                    exitReason: 'bollinger_cross',
                    maxFavorable: highestPnl,
                    maxAdverse: lowestPnl,
                    maxFavorablePercent: highestPnl / entryPrice * 100,
                    maxAdversePercent: lowestPnl / entryPrice * 100,
                    touchedBollBand,
                    touchedUpperBeforeExit,
                    entryToBollMidPercent,
                    bollWidthPercent,
                    trendStrength,
                    firstBarDirection,
                    maxFavorableInFirst5Percent: maxFavorableInFirst5 / entryPrice * 100,
                    maxAdverseInFirst5Percent: maxAdverseInFirst5 / entryPrice * 100,
                    // 分类标签
                    isImmediateLoss: maxAdverseInFirst5 > maxFavorableInFirst5 && maxFavorableInFirst5 / entryPrice * 100 < 0.1,
                    hadProfit: highestPnl > 0,
                    profitToLoss: highestPnl > 0 && pnl <= 0,
                });

                position = 0;
                touchedBollBand = false;
                highestPnl = 0;
                lowestPnl = 0;
                maxFavorableInFirst5 = 0;
                maxAdverseInFirst5 = 0;
            }
            else if (position === -1 && !openAboveMid && closeAboveMid) {
                const exitPrice = bar.close;
                const pnl = entryPrice - exitPrice;
                const pnlPercent = pnl / entryPrice * 100;

                const entryToBollMid = Math.abs(entryPrice - boll.middle[entryIdx]);
                const entryToBollMidPercent = entryToBollMid / entryPrice * 100;

                const bollWidthAtEntry = boll.upper[entryIdx] - boll.lower[entryIdx];
                const bollWidthPercent = bollWidthAtEntry / entryPrice * 100;

                let touchedLowerBeforeExit = false;
                for (let j = entryIdx; j <= i; j++) {
                    if (data[j].low <= boll.lower[j]) {
                        touchedLowerBeforeExit = true;
                        break;
                    }
                }

                const firstBarAfterEntry = data[entryIdx + 1];
                const firstBarDirection = firstBarAfterEntry ? (firstBarAfterEntry.close < entryPrice ? 'favorable' : 'adverse') : 'unknown';

                const ma48AtEntry = ma48[entryIdx];
                const ma288AtEntry = ma288[entryIdx];
                const trendStrength = ma48AtEntry && ma288AtEntry ? (ma288AtEntry - ma48AtEntry) / ma48AtEntry * 100 : 0;

                trades.push({
                    type: 'SHORT',
                    entryTime, entryPrice, entryIdx,
                    exitTime: bar.open_time, exitPrice, exitIdx: i,
                    pnl, pnlPercent, holdBars,
                    exitReason: 'bollinger_cross',
                    maxFavorable: lowestPnl,
                    maxAdverse: highestPnl,
                    maxFavorablePercent: lowestPnl / entryPrice * 100,
                    maxAdversePercent: highestPnl / entryPrice * 100,
                    touchedBollBand,
                    touchedUpperBeforeExit: touchedLowerBeforeExit,
                    entryToBollMidPercent,
                    bollWidthPercent,
                    trendStrength,
                    firstBarDirection,
                    maxFavorableInFirst5Percent: maxFavorableInFirst5 / entryPrice * 100,
                    maxAdverseInFirst5Percent: maxAdverseInFirst5 / entryPrice * 100,
                    isImmediateLoss: maxAdverseInFirst5 > maxFavorableInFirst5 && maxFavorableInFirst5 / entryPrice * 100 < 0.1,
                    hadProfit: lowestPnl > 0,
                    profitToLoss: lowestPnl > 0 && pnl <= 0,
                });

                position = 0;
                touchedBollBand = false;
                highestPnl = 0;
                lowestPnl = 0;
                maxFavorableInFirst5 = 0;
                maxAdverseInFirst5 = 0;
            }
        }

        // 开仓
        if (position === 0) {
            if (trend === 'bull' && !openAboveMid && closeAboveMid) {
                position = 1;
                entryPrice = bar.close;
                entryTime = bar.open_time;
                entryIdx = i;
                touchedBollBand = false;
                highestPnl = 0;
                lowestPnl = 0;
                maxFavorableInFirst5 = 0;
                maxAdverseInFirst5 = 0;
            } else if (trend === 'bear' && openAboveMid && !closeAboveMid) {
                position = -1;
                entryPrice = bar.close;
                entryTime = bar.open_time;
                entryIdx = i;
                touchedBollBand = false;
                highestPnl = 0;
                lowestPnl = 0;
                maxFavorableInFirst5 = 0;
                maxAdverseInFirst5 = 0;
            }
        }
    }

    return trades;
}

// ============ 深度分析 ============
function deepAnalysis(trades) {
    console.log('='.repeat(80));
    console.log('【穿越中轨止损交易深度分析】');
    console.log('='.repeat(80));

    if (trades.length === 0) {
        console.log('无交易记录');
        return;
    }

    // 1. 基础分类统计
    const immediateLosses = trades.filter(t => t.isImmediateLoss);
    const hadProfitTrades = trades.filter(t => t.hadProfit);
    const profitToLossTrades = trades.filter(t => t.profitToLoss);

    console.log('\n📊 一、止损交易分类');
    console.log(`  总止损交易: ${trades.length}笔`);
    console.log(`  入场即亏（前5根K线就亏）: ${immediateLosses.length}笔 (${(immediateLosses.length / trades.length * 100).toFixed(1)}%)`);
    console.log(`  曾有浮盈: ${hadProfitTrades.length}笔 (${(hadProfitTrades.length / trades.length * 100).toFixed(1)}%)`);
    console.log(`  盈利变亏损: ${profitToLossTrades.length}笔 (${(profitToLossTrades.length / trades.length * 100).toFixed(1)}%)`);

    // 2. 入场即亏的交易分析
    console.log('\n📉 二、入场即亏的交易特点');
    if (immediateLosses.length > 0) {
        const avgHoldBars = immediateLosses.reduce((s, t) => s + t.holdBars, 0) / immediateLosses.length;
        const avgPnl = immediateLosses.reduce((s, t) => s + t.pnlPercent, 0) / immediateLosses.length;
        const avgAdverse = immediateLosses.reduce((s, t) => s + t.maxAdversePercent, 0) / immediateLosses.length;
        const avgEntryToMid = immediateLosses.reduce((s, t) => s + t.entryToBollMidPercent, 0) / immediateLosses.length;

        console.log(`  平均持仓: ${avgHoldBars.toFixed(1)}根K线 (${(avgHoldBars * 5).toFixed(0)}分钟)`);
        console.log(`  平均亏损: ${avgPnl.toFixed(3)}%`);
        console.log(`  平均最大浮亏: ${avgAdverse.toFixed(3)}%`);
        console.log(`  入场价与中轨距离: ${avgEntryToMid.toFixed(3)}%`);

        // 第一根K线方向
        const firstBarFavorable = immediateLosses.filter(t => t.firstBarDirection === 'favorable').length;
        const firstBarAdverse = immediateLosses.filter(t => t.firstBarDirection === 'adverse').length;
        console.log(`\n  第一根K线方向:`);
        console.log(`    有利: ${firstBarFavorable}笔 (${(firstBarFavorable / immediateLosses.length * 100).toFixed(1)}%)`);
        console.log(`    不利: ${firstBarAdverse}笔 (${(firstBarAdverse / immediateLosses.length * 100).toFixed(1)}%)`);
    }

    // 3. 盈利变亏损的交易分析
    console.log('\n📈 三、盈利变亏损的交易特点');
    if (profitToLossTrades.length > 0) {
        const avgHoldBars = profitToLossTrades.reduce((s, t) => s + t.holdBars, 0) / profitToLossTrades.length;
        const avgPnl = profitToLossTrades.reduce((s, t) => s + t.pnlPercent, 0) / profitToLossTrades.length;
        const avgMaxProfit = profitToLossTrades.reduce((s, t) => s + t.maxFavorablePercent, 0) / profitToLossTrades.length;
        const avgProfitLost = profitToLossTrades.reduce((s, t) => s + (t.maxFavorablePercent - t.pnlPercent), 0) / profitToLossTrades.length;

        console.log(`  平均持仓: ${avgHoldBars.toFixed(1)}根K线 (${(avgHoldBars * 5).toFixed(0)}分钟)`);
        console.log(`  平均亏损: ${avgPnl.toFixed(3)}%`);
        console.log(`  平均最大浮盈: +${avgMaxProfit.toFixed(3)}%`);
        console.log(`  平均利润回吐: ${avgProfitLost.toFixed(3)}%`);

        // 是否触及布林轨
        const touchedBoll = profitToLossTrades.filter(t => t.touchedUpperBeforeExit).length;
        console.log(`\n  触及布林轨: ${touchedBoll}笔 (${(touchedBoll / profitToLossTrades.length * 100).toFixed(1)}%)`);
    }

    // 4. 持仓时间分布
    console.log('\n⏱️ 四、持仓时间分布');
    const holdRanges = [
        { label: '≤5根(25分钟)', min: 0, max: 5 },
        { label: '6-10根(30-50分钟)', min: 6, max: 10 },
        { label: '11-20根(55-100分钟)', min: 11, max: 20 },
        { label: '21-50根(105-250分钟)', min: 21, max: 50 },
        { label: '>50根(>250分钟)', min: 51, max: Infinity },
    ];

    holdRanges.forEach(r => {
        const count = trades.filter(t => t.holdBars >= r.min && t.holdBars <= r.max).length;
        const bar = '█'.repeat(Math.ceil(count / 3));
        const percent = (count / trades.length * 100).toFixed(1);
        console.log(`  ${r.label.padEnd(25)}: ${count.toString().padStart(3)}笔 (${percent.padStart(5)}%) ${bar}`);
    });

    // 5. 入场价与中轨距离分布
    console.log('\n📏 五、入场价与中轨距离分布');
    const distRanges = [
        { label: '< 0.05%', min: 0, max: 0.05 },
        { label: '0.05% ~ 0.1%', min: 0.05, max: 0.1 },
        { label: '0.1% ~ 0.2%', min: 0.1, max: 0.2 },
        { label: '0.2% ~ 0.5%', min: 0.2, max: 0.5 },
        { label: '> 0.5%', min: 0.5, max: Infinity },
    ];

    distRanges.forEach(r => {
        const tradesInRange = trades.filter(t => t.entryToBollMidPercent >= r.min && t.entryToBollMidPercent < r.max);
        const count = tradesInRange.length;
        const bar = '█'.repeat(Math.ceil(count / 3));
        const percent = (count / trades.length * 100).toFixed(1);
        console.log(`  ${r.label.padEnd(15)}: ${count.toString().padStart(3)}笔 (${percent.padStart(5)}%) ${bar}`);
    });

    // 6. 趋势强度分析
    console.log('\n📊 六、入场时趋势强度');
    const trendRanges = [
        { label: '弱趋势 (<0.5%)', min: 0, max: 0.5 },
        { label: '中趋势 (0.5-1%)', min: 0.5, max: 1 },
        { label: '强趋势 (1-2%)', min: 1, max: 2 },
        { label: '超强趋势 (>2%)', min: 2, max: Infinity },
    ];

    trendRanges.forEach(r => {
        const tradesInRange = trades.filter(t => t.trendStrength >= r.min && t.trendStrength < r.max);
        const count = tradesInRange.length;
        if (count > 0) {
            const avgPnl = tradesInRange.reduce((s, t) => s + t.pnlPercent, 0) / count;
            const bar = '█'.repeat(Math.ceil(count / 3));
            console.log(`  ${r.label.padEnd(20)}: ${count.toString().padStart(3)}笔, 平均${avgPnl >= 0 ? '+' : ''}${avgPnl.toFixed(3)}% ${bar}`);
        }
    });

    // 7. 布林带宽度分析
    console.log('\n📏 七、入场时布林带宽度');
    const bollWidthRanges = [
        { label: '窄幅 (<2%)', min: 0, max: 2 },
        { label: '中等 (2-4%)', min: 2, max: 4 },
        { label: '宽幅 (4-6%)', min: 4, max: 6 },
        { label: '超宽 (>6%)', min: 6, max: Infinity },
    ];

    bollWidthRanges.forEach(r => {
        const tradesInRange = trades.filter(t => t.bollWidthPercent >= r.min && t.bollWidthPercent < r.max);
        const count = tradesInRange.length;
        if (count > 0) {
            const avgPnl = tradesInRange.reduce((s, t) => s + t.pnlPercent, 0) / count;
            const bar = '█'.repeat(Math.ceil(count / 3));
            console.log(`  ${r.label.padEnd(15)}: ${count.toString().padStart(3)}笔, 平均${avgPnl >= 0 ? '+' : ''}${avgPnl.toFixed(3)}% ${bar}`);
        }
    });

    // 8. 典型案例
    console.log('\n📋 八、典型案例');

    // 入场即亏的典型案例
    console.log('\n  入场即亏的典型案例:');
    immediateLosses.slice(0, 3).forEach((t, i) => {
        console.log(`    #${i + 1} ${t.type} ${t.entryTime.substring(5, 16)} | 持仓${t.holdBars}根 | ${t.pnlPercent.toFixed(3)}% | 第一根K线: ${t.firstBarDirection}`);
    });

    // 盈利变亏损的典型案例
    console.log('\n  盈利变亏损的典型案例:');
    profitToLossTrades.sort((a, b) => a.pnlPercent - b.pnlPercent).slice(0, 3).forEach((t, i) => {
        console.log(`    #${i + 1} ${t.type} ${t.entryTime.substring(5, 16)} | 峰值+${t.maxFavorablePercent.toFixed(3)}% → 最终${t.pnlPercent.toFixed(3)}% | 利润回吐: ${(t.maxFavorablePercent - t.pnlPercent).toFixed(3)}%`);
    });

    // 9. 总结
    console.log('\n' + '='.repeat(80));
    console.log('📌 总结');
    console.log('='.repeat(80));

    console.log(`\n  穿越中轨止损的141笔交易:`);
    console.log(`  - 入场即亏: ${immediateLosses.length}笔 (${(immediateLosses.length / trades.length * 100).toFixed(1)}%)`);
    console.log(`  - 盈利变亏损: ${profitToLossTrades.length}笔 (${(profitToLossTrades.length / trades.length * 100).toFixed(1)}%)`);
    console.log(`  - 其他: ${trades.length - immediateLosses.length - profitToLossTrades.length}笔`);

    const shortHold = trades.filter(t => t.holdBars <= 5).length;
    console.log(`\n  持仓≤5根K线: ${shortHold}笔 (${(shortHold / trades.length * 100).toFixed(1)}%)`);
    console.log(`  平均入场价与中轨距离: ${(trades.reduce((s, t) => s + t.entryToBollMidPercent, 0) / trades.length).toFixed(3)}%`);

    if (immediateLosses.length > trades.length * 0.5) {
        console.log('\n  ⚠️ 超过一半的止损是"入场即亏"，说明入场信号质量差');
    }

    if (profitToLossTrades.length > trades.length * 0.3) {
        console.log('  ⚠️ 超过30%的止损是"盈利变亏损"，说明离场时机有问题');
    }
}

// ============ 主程序 ============
function main() {
    const csvPath = path.resolve('F:/rust_projects/trade/src/kline_5m_202607232011.csv');
    const data = loadCSV(csvPath);

    console.log('穿越中轨止损交易深度分析');
    console.log(`数据: ${data.length}条K线, ${data[0].open_time} ~ ${data[data.length - 1].open_time}\n`);

    const allTrades = backtestWithDetails(data, CONFIG);

    // 只分析穿越中轨止损的交易
    const bollingerCrossTrades = allTrades.filter(t => t.exitReason === 'bollinger_cross' && t.pnl <= 0);

    console.log(`总交易: ${allTrades.length}笔`);
    console.log(`穿越中轨止损: ${bollingerCrossTrades.length}笔\n`);

    deepAnalysis(bollingerCrossTrades);
}

main();
