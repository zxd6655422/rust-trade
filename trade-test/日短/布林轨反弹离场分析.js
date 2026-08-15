/**
 * 布林轨反弹离场分析
 * 逻辑：在有盈利的情况下，如果价格从布林上下轨反弹接近中轨，就提前离场
 * 目标：减少"盈利变亏损"的情况
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
function backtest(data, config, exitMode) {
    const n = data.length;
    const ma48 = calcSMA(data, config.ma48_period);
    const ma288 = calcSMA(data, config.ma288_period);
    const boll = calcBollinger(data, config.bollinger_period, config.bollinger_std_mult);

    let position = 0, entryPrice = 0, entryTime = '', entryIdx = 0;
    let touchedBollBand = false; // 是否触及过布林轨
    let highestPnl = 0, lowestPnl = 0;

    const trades = [];
    let totalPnl = 0, winCount = 0, lossCount = 0;

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

            // 更新最高/最低盈利
            if (position === 1) {
                if (currentPnl > highestPnl) highestPnl = currentPnl;
            } else {
                if (currentPnl > lowestPnl) lowestPnl = currentPnl;
            }

            // 检查是否触及布林轨
            if (position === 1 && bar.high >= currentBollUpper) {
                touchedBollBand = true;
            }
            if (position === -1 && bar.low <= currentBollLower) {
                touchedBollBand = true;
            }

            // 布林轨反弹离场逻辑
            if (exitMode === 'boll_rebound' && touchedBollBand && currentPnl > 0) {
                // 计算当前价与中轨的距离
                const distToMid = Math.abs(bar.close - currentBollMid);
                const bollWidth = currentBollUpper - currentBollLower;
                const distPercent = distToMid / bollWidth; // 距离占布林带宽度的比例

                // 如果价格从布林轨反弹回到中轨附近（距离中轨<30%布林带宽度）
                if (distPercent < 0.3) {
                    const exitPrice = bar.close;
                    const pnl = currentPnl;
                    const holdBars = i - entryIdx;

                    totalPnl += pnl;
                    if (pnl > 0) winCount++; else lossCount++;

                    trades.push({
                        type: position === 1 ? 'LONG' : 'SHORT',
                        entryTime, entryPrice, exitTime: bar.open_time, exitPrice,
                        pnl, pnlPercent: pnl / entryPrice * 100, holdBars,
                        exitReason: 'boll_rebound',
                        touchedBollBand,
                        highestPnl: position === 1 ? highestPnl : lowestPnl,
                    });

                    position = 0;
                    touchedBollBand = false;
                    highestPnl = 0;
                    lowestPnl = 0;
                    continue;
                }
            }

            // 原有平仓条件：价格反向穿越中轨
            if (position === 1 && openAboveMid && !closeAboveMid) {
                const exitPrice = bar.close;
                const pnl = exitPrice - entryPrice;
                const holdBars = i - entryIdx;

                totalPnl += pnl;
                if (pnl > 0) winCount++; else lossCount++;

                trades.push({
                    type: 'LONG', entryTime, entryPrice, exitTime: bar.open_time, exitPrice,
                    pnl, pnlPercent: pnl / entryPrice * 100, holdBars,
                    exitReason: 'bollinger_cross',
                    touchedBollBand,
                    highestPnl,
                });

                position = 0;
                touchedBollBand = false;
                highestPnl = 0;
                lowestPnl = 0;
            }
            else if (position === -1 && !openAboveMid && closeAboveMid) {
                const exitPrice = bar.close;
                const pnl = entryPrice - exitPrice;
                const holdBars = i - entryIdx;

                totalPnl += pnl;
                if (pnl > 0) winCount++; else lossCount++;

                trades.push({
                    type: 'SHORT', entryTime, entryPrice, exitTime: bar.open_time, exitPrice,
                    pnl, pnlPercent: pnl / entryPrice * 100, holdBars,
                    exitReason: 'bollinger_cross',
                    touchedBollBand,
                    highestPnl: lowestPnl,
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
                position = 1;
                entryPrice = bar.close;
                entryTime = bar.open_time;
                entryIdx = i;
                touchedBollBand = false;
                highestPnl = 0;
                lowestPnl = 0;
            } else if (trend === 'bear' && openAboveMid && !closeAboveMid) {
                position = -1;
                entryPrice = bar.close;
                entryTime = bar.open_time;
                entryIdx = i;
                touchedBollBand = false;
                highestPnl = 0;
                lowestPnl = 0;
            }
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
            exitReason: 'force_close',
        });
    }

    return { trades, totalPnl, winCount, lossCount };
}

// ============ 统计分析 ============
function analyze(result, label) {
    const trades = result.trades;
    if (trades.length === 0) return null;

    const wins = trades.filter(t => t.pnl > 0);
    const losses = trades.filter(t => t.pnl <= 0);
    const avgWin = wins.length > 0 ? wins.reduce((s, t) => s + t.pnl, 0) / wins.length : 0;
    const avgLoss = losses.length > 0 ? losses.reduce((s, t) => s + t.pnl, 0) / losses.length : 0;
    const profitFactor = avgLoss !== 0 ? Math.abs(avgWin / avgLoss) : Infinity;

    let peak = 0, maxDrawdown = 0, currentPnl = 0;
    trades.forEach(t => {
        currentPnl += t.pnl;
        if (currentPnl > peak) peak = currentPnl;
        const dd = peak - currentPnl;
        if (dd > maxDrawdown) maxDrawdown = dd;
    });

    let maxConsecLoss = 0, curLoss = 0;
    trades.forEach(t => {
        if (t.pnl <= 0) { curLoss++; maxConsecLoss = Math.max(maxConsecLoss, curLoss); }
        else curLoss = 0;
    });

    // 出场原因统计
    const exitReasons = {};
    trades.forEach(t => {
        if (!exitReasons[t.exitReason]) exitReasons[t.exitReason] = { count: 0, pnl: 0 };
        exitReasons[t.exitReason].count++;
        exitReasons[t.exitReason].pnl += t.pnl;
    });

    return {
        label,
        trades: trades.length,
        winRate: result.winCount / trades.length,
        profitFactor,
        totalPnl: result.totalPnl,
        maxDrawdown,
        maxConsecLoss,
        exitReasons,
    };
}

// ============ 盈亏转换分析 ============
function analyzeProfitLossConversion(trades) {
    // 分析"盈利变亏损"和"亏损变盈利"的情况
    let profitToLoss = 0;  // 盈利变亏损
    let lossToProfit = 0;  // 亏损变盈利（本策略没有这种情况）

    // 分析触及布林轨后的情况
    const touchedBoll = trades.filter(t => t.touchedBollBand);
    const touchedBollWin = touchedBoll.filter(t => t.pnl > 0);
    const touchedBollLoss = touchedBoll.filter(t => t.pnl <= 0);

    // 分析没有触及布林轨的情况
    const notTouchedBoll = trades.filter(t => !t.touchedBollBand);
    const notTouchedBollWin = notTouchedBoll.filter(t => t.pnl > 0);
    const notTouchedBollLoss = notTouchedBoll.filter(t => t.pnl <= 0);

    return {
        touchedBoll: {
            total: touchedBoll.length,
            win: touchedBollWin.length,
            loss: touchedBollLoss.length,
            winRate: touchedBoll.length > 0 ? touchedBollWin.length / touchedBoll.length : 0,
        },
        notTouchedBoll: {
            total: notTouchedBoll.length,
            win: notTouchedBollWin.length,
            loss: notTouchedBollLoss.length,
            winRate: notTouchedBoll.length > 0 ? notTouchedBollWin.length / notTouchedBoll.length : 0,
        },
    };
}

// ============ 主程序 ============
function main() {
    const csvPath = path.resolve('F:/rust_projects/trade/src/kline_5m_202607232011.csv');
    const data = loadCSV(csvPath);

    console.log('='.repeat(70));
    console.log('布林轨反弹离场策略分析');
    console.log('='.repeat(70));
    console.log(`数据: ${data.length}条K线, ${data[0].open_time} ~ ${data[data.length - 1].open_time}\n`);

    // 测试两种模式
    const original = backtest(data, CONFIG, 'original');
    const bollRebound = backtest(data, CONFIG, 'boll_rebound');

    const originalStats = analyze(original, '原版(穿越中轨离场)');
    const bollReboundStats = analyze(bollRebound, '布林轨反弹离场');

    // 输出对比
    console.log('\n' +
        '策略'.padEnd(25) +
        '交易'.padEnd(6) +
        '胜率'.padEnd(8) +
        '盈亏比'.padEnd(8) +
        '总盈亏'.padEnd(10) +
        '最大回撤'.padEnd(10) +
        '连亏'
    );
    console.log('-'.repeat(75));

    [originalStats, bollReboundStats].forEach(r => {
        if (r) {
            console.log(
                r.label.padEnd(25) +
                `${r.trades}`.padEnd(6) +
                `${(r.winRate * 100).toFixed(1)}%`.padEnd(8) +
                `${r.profitFactor.toFixed(2)}`.padEnd(8) +
                `${r.totalPnl >= 0 ? '+' : ''}${r.totalPnl.toFixed(2)}`.padEnd(10) +
                `${r.maxDrawdown.toFixed(2)}`.padEnd(10) +
                `${r.maxConsecLoss}`
            );
        }
    });

    // 出场原因分析
    console.log('\n📊 出场原因分析');

    ['原版', '布林轨反弹'].forEach((label, idx) => {
        const stats = idx === 0 ? originalStats : bollReboundStats;
        if (stats) {
            console.log(`\n  ${label}:`);
            Object.entries(stats.exitReasons).forEach(([reason, data]) => {
                const avgPnl = data.pnl / data.count;
                console.log(`    ${reason}: ${data.count}笔, 盈亏${data.pnl >= 0 ? '+' : ''}${data.pnl.toFixed(2)}, 平均${avgPnl >= 0 ? '+' : ''}${avgPnl.toFixed(4)}`);
            });
        }
    });

    // 盈亏转换分析
    console.log('\n📈 触及布林轨 vs 未触及布林轨');

    const conversion = analyzeProfitLossConversion(original.trades);
    console.log('\n  原版策略:');
    console.log(`    触及布林轨: ${conversion.touchedBoll.total}笔, 胜率${(conversion.touchedBoll.winRate * 100).toFixed(1)}%`);
    console.log(`    未触及布林轨: ${conversion.notTouchedBoll.total}笔, 胜率${(conversion.notTouchedBoll.winRate * 100).toFixed(1)}%`);

    // 典型案例：盈利变亏损
    console.log('\n📋 典型案例：盈利变亏损的情况');

    const profitThenLoss = original.trades.filter(t => {
        // 找出那些最大浮盈超过0.3%但最终亏损的交易
        return t.highestPnl > 0 && t.pnl <= 0 && t.highestPnl / t.entryPrice * 100 > 0.3;
    });

    if (profitThenLoss.length > 0) {
        console.log(`\n  原版策略中，最大浮盈>0.3%但最终亏损的交易: ${profitThenLoss.length}笔`);
        profitThenLoss.slice(0, 5).forEach((t, i) => {
            const peakPnlPercent = t.highestPnl / t.entryPrice * 100;
            console.log(`    #${i + 1} ${t.type} ${t.entryTime.substring(5, 16)} | 峰值+${peakPnlPercent.toFixed(2)}% → 最终${t.pnlPercent.toFixed(2)}%`);
        });
    }

    // 布林轨反弹策略中的类似案例
    const bollReboundCases = bollRebound.trades.filter(t => t.exitReason === 'boll_rebound');
    if (bollReboundCases.length > 0) {
        console.log(`\n  布林轨反弹离场: ${bollReboundCases.length}笔`);
        const bollReboundWins = bollReboundCases.filter(t => t.pnl > 0);
        console.log(`    其中盈利: ${bollReboundWins.length}笔 (${(bollReboundWins.length / bollReboundCases.length * 100).toFixed(1)}%)`);

        bollReboundCases.slice(0, 5).forEach((t, i) => {
            const emoji = t.pnl > 0 ? '✅' : '❌';
            console.log(`    ${emoji} #${i + 1} ${t.type} ${t.entryTime.substring(5, 16)} | ${t.pnlPercent >= 0 ? '+' : ''}${t.pnlPercent.toFixed(2)}%`);
        });
    }

    // 结论
    console.log('\n' + '='.repeat(70));
    console.log('📌 结论');
    console.log('='.repeat(70));

    if (bollReboundStats && originalStats) {
        const pnlDiff = bollReboundStats.totalPnl - originalStats.totalPnl;
        const winRateDiff = bollReboundStats.winRate - originalStats.winRate;

        console.log(`\n  布林轨反弹离场效果:`);
        console.log(`    总盈亏: ${pnlDiff >= 0 ? '+' : ''}${pnlDiff.toFixed(2)} USDT (${pnlDiff >= 0 ? '改善' : '下降'})`);
        console.log(`    胜率变化: ${winRateDiff >= 0 ? '+' : ''}${(winRateDiff * 100).toFixed(1)}%`);
        console.log(`    交易次数: ${originalStats.trades} → ${bollReboundStats.trades}`);

        if (pnlDiff > 0 && winRateDiff > 0) {
            console.log('\n  ✅ 布林轨反弹离场策略有效！');
            console.log('    - 减少了"盈利变亏损"的情况');
            console.log('    - 提高了胜率');
        } else if (pnlDiff > 0) {
            console.log('\n  ⚠️ 策略有改善，但胜率提升不明显');
        } else {
            console.log('\n  ❌ 策略未能改善，可能过早离场错过了趋势');
        }
    }
}

main();
