/**
 * 组合策略测试
 * 测试多种离场方式的组合
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

// ============ 回测引擎 - 多种组合策略 ============
function backtest(data, config, strategy) {
    const n = data.length;
    const ma48 = calcSMA(data, config.ma48_period);
    const ma288 = calcSMA(data, config.ma288_period);
    const boll = calcBollinger(data, config.bollinger_period, config.bollinger_std_mult);

    let position = 0, entryPrice = 0, entryTime = '', entryIdx = 0;
    let touchedBollBand = false;
    let highestPnl = 0, lowestPnl = 0;
    let bollReboundUsed = false; // 是否已经使用过布林轨反弹离场

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

            // 根据策略模式决定离场逻辑
            let shouldExit = false;
            let exitReason = '';

            switch (strategy.mode) {
                case 'original':
                    // 原版策略：穿越中轨离场
                    if (position === 1 && openAboveMid && !closeAboveMid) {
                        shouldExit = true;
                        exitReason = 'bollinger_cross';
                    } else if (position === -1 && !openAboveMid && closeAboveMid) {
                        shouldExit = true;
                        exitReason = 'bollinger_cross';
                    }
                    break;

                case 'boll_rebound_only':
                    // 只用布林轨反弹离场（不穿越中轨离场）
                    if (touchedBollBand && currentPnl > 0) {
                        const distToMid = Math.abs(bar.close - currentBollMid);
                        const bollWidth = currentBollUpper - currentBollLower;
                        const distPercent = distToMid / bollWidth;

                        if (distPercent < 0.3) {
                            shouldExit = true;
                            exitReason = 'boll_rebound';
                        }
                    }
                    // 如果没触及布林轨，用穿越中轨离场
                    else if (!touchedBollBand) {
                        if (position === 1 && openAboveMid && !closeAboveMid) {
                            shouldExit = true;
                            exitReason = 'bollinger_cross';
                        } else if (position === -1 && !openAboveMid && closeAboveMid) {
                            shouldExit = true;
                            exitReason = 'bollinger_cross';
                        }
                    }
                    break;

                case 'hybrid_partial':
                    // 混合策略：触及布林轨后，一半仓位用反弹离场，一半用穿越中轨
                    // 由于无法真正分仓，这里模拟：50%概率选择反弹离场，50%选择穿越中轨
                    if (touchedBollBand && currentPnl > 0 && !bollReboundUsed) {
                        const distToMid = Math.abs(bar.close - currentBollMid);
                        const bollWidth = currentBollUpper - currentBollLower;
                        const distPercent = distToMid / bollWidth;

                        if (distPercent < 0.3) {
                            // 50%仓位用反弹离场
                            const partialPnl = currentPnl * 0.5;
                            trades.push({
                                type: position === 1 ? 'LONG' : 'SHORT',
                                entryTime, entryPrice, exitTime: bar.open_time, exitPrice: bar.close,
                                pnl: partialPnl, pnlPercent: partialPnl / entryPrice * 100, holdBars: i - entryIdx,
                                exitReason: 'boll_rebound_partial',
                            });
                            totalPnl += partialPnl;
                            if (partialPnl > 0) winCount++;
                            bollReboundUsed = true;
                            // 继续持有另一半仓位
                        }
                    }

                    // 穿越中轨离场（剩余仓位或未触及布林轨时）
                    if (position === 1 && openAboveMid && !closeAboveMid) {
                        const remainingPnl = bollReboundUsed ? currentPnl * 0.5 : currentPnl;
                        shouldExit = true;
                        exitReason = bollReboundUsed ? 'bollinger_cross_remaining' : 'bollinger_cross';
                        // 调整pnl
                        trades.push({
                            type: 'LONG', entryTime, entryPrice, exitTime: bar.open_time, exitPrice: bar.close,
                            pnl: remainingPnl, pnlPercent: remainingPnl / entryPrice * 100, holdBars: i - entryIdx,
                            exitReason,
                        });
                        totalPnl += remainingPnl;
                        if (remainingPnl > 0) winCount++; else lossCount++;
                        position = 0;
                        touchedBollBand = false;
                        bollReboundUsed = false;
                        highestPnl = 0;
                        lowestPnl = 0;
                        continue;
                    } else if (position === -1 && !openAboveMid && closeAboveMid) {
                        const remainingPnl = bollReboundUsed ? currentPnl * 0.5 : currentPnl;
                        shouldExit = true;
                        exitReason = bollReboundUsed ? 'bollinger_cross_remaining' : 'bollinger_cross';
                        trades.push({
                            type: 'SHORT', entryTime, entryPrice, exitTime: bar.open_time, exitPrice: bar.close,
                            pnl: remainingPnl, pnlPercent: remainingPnl / entryPrice * 100, holdBars: i - entryIdx,
                            exitReason,
                        });
                        totalPnl += remainingPnl;
                        if (remainingPnl > 0) winCount++; else lossCount++;
                        position = 0;
                        touchedBollBand = false;
                        bollReboundUsed = false;
                        highestPnl = 0;
                        lowestPnl = 0;
                        continue;
                    }
                    break;

                case 'smart_switch':
                    // 智能切换：触及布林轨后用反弹离场，否则用穿越中轨
                    if (touchedBollBand && currentPnl > 0) {
                        const distToMid = Math.abs(bar.close - currentBollMid);
                        const bollWidth = currentBollUpper - currentBollLower;
                        const distPercent = distToMid / bollWidth;

                        if (distPercent < 0.3) {
                            shouldExit = true;
                            exitReason = 'boll_rebound';
                        }
                    }

                    // 穿越中轨离场（未触及布林轨或触及后未反弹到中轨）
                    if (!shouldExit) {
                        if (position === 1 && openAboveMid && !closeAboveMid) {
                            shouldExit = true;
                            exitReason = touchedBollBand ? 'bollinger_cross_after_touch' : 'bollinger_cross';
                        } else if (position === -1 && !openAboveMid && closeAboveMid) {
                            shouldExit = true;
                            exitReason = touchedBollBand ? 'bollinger_cross_after_touch' : 'bollinger_cross';
                        }
                    }
                    break;

                case 'trend_rider':
                    // 趋势骑手：触及布林轨后不急着离场，等穿越中轨或回撤超过峰值的50%
                    if (touchedBollBand && currentPnl > 0) {
                        const peakPnl = position === 1 ? highestPnl : lowestPnl;
                        const drawdownFromPeak = peakPnl - currentPnl;
                        const drawdownPercent = drawdownFromPeak / entryPrice * 100;

                        // 回撤超过峰值的60%时离场
                        if (drawdownPercent > 1) {
                            shouldExit = true;
                            exitReason = 'peak_drawdown';
                        }
                    }

                    // 穿越中轨离场
                    if (!shouldExit) {
                        if (position === 1 && openAboveMid && !closeAboveMid) {
                            shouldExit = true;
                            exitReason = 'bollinger_cross';
                        } else if (position === -1 && !openAboveMid && closeAboveMid) {
                            shouldExit = true;
                            exitReason = 'bollinger_cross';
                        }
                    }
                    break;

                case 'trailing_with_rebound':
                    // 移动止盈 + 布林轨反弹
                    if (touchedBollBand && currentPnl > 0) {
                        const distToMid = Math.abs(bar.close - currentBollMid);
                        const bollWidth = currentBollUpper - currentBollLower;
                        const distPercent = distToMid / bollWidth;

                        // 从布林轨反弹到中轨附近，且盈利超过1%时离场
                        if (distPercent < 0.3 && currentPnlPercent > 1) {
                            shouldExit = true;
                            exitReason = 'boll_rebound_profit';
                        }
                    }

                    // 移动止盈：盈利超过2%后，回撤0.5%离场
                    const peakPnl = position === 1 ? highestPnl : lowestPnl;
                    const peakPnlPercent = peakPnl / entryPrice * 100;
                    if (peakPnlPercent > 2) {
                        const drawdown = peakPnl - currentPnl;
                        const drawdownPercent = drawdown / entryPrice * 100;
                        if (drawdownPercent > 0.5) {
                            shouldExit = true;
                            exitReason = 'trailing_stop';
                        }
                    }

                    // 穿越中轨离场
                    if (!shouldExit) {
                        if (position === 1 && openAboveMid && !closeAboveMid) {
                            shouldExit = true;
                            exitReason = 'bollinger_cross';
                        } else if (position === -1 && !openAboveMid && closeAboveMid) {
                            shouldExit = true;
                            exitReason = 'bollinger_cross';
                        }
                    }
                    break;
            }

            // 执行离场
            if (shouldExit) {
                const exitPrice = bar.close;
                const pnl = currentPnl;
                const holdBars = i - entryIdx;

                totalPnl += pnl;
                if (pnl > 0) winCount++; else lossCount++;

                trades.push({
                    type: position === 1 ? 'LONG' : 'SHORT',
                    entryTime, entryPrice, exitTime: bar.open_time, exitPrice,
                    pnl, pnlPercent: pnl / entryPrice * 100, holdBars,
                    exitReason,
                    touchedBollBand,
                });

                position = 0;
                touchedBollBand = false;
                bollReboundUsed = false;
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
                bollReboundUsed = false;
                highestPnl = 0;
                lowestPnl = 0;
            } else if (trend === 'bear' && openAboveMid && !closeAboveMid) {
                position = -1;
                entryPrice = bar.close;
                entryTime = bar.open_time;
                entryIdx = i;
                touchedBollBand = false;
                bollReboundUsed = false;
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
        if (!exitReasons[t.exitReason]) exitReasons[t.exitReason] = { count: 0, pnl: 0, wins: 0 };
        exitReasons[t.exitReason].count++;
        exitReasons[t.exitReason].pnl += t.pnl;
        if (t.pnl > 0) exitReasons[t.exitReason].wins++;
    });

    // 月度统计
    const monthlyData = {};
    trades.forEach(t => {
        const month = t.exitTime.substring(0, 7);
        if (!monthlyData[month]) monthlyData[month] = { pnl: 0 };
        monthlyData[month].pnl += t.pnl;
    });
    const profitableMonths = Object.values(monthlyData).filter(m => m.pnl >= 0).length;
    const totalMonths = Object.keys(monthlyData).length;

    return {
        label,
        trades: trades.length,
        winRate: result.winCount / trades.length,
        profitFactor,
        totalPnl: result.totalPnl,
        maxDrawdown,
        maxConsecLoss,
        avgHoldBars: trades.reduce((s, t) => s + t.holdBars, 0) / trades.length,
        monthlyProfitRate: totalMonths > 0 ? profitableMonths / totalMonths : 0,
        exitReasons,
    };
}

// ============ 主程序 ============
function main() {
    const csvPath = path.resolve('F:/rust_projects/trade/src/kline_5m_202607232011.csv');
    const data = loadCSV(csvPath);

    console.log('='.repeat(80));
    console.log('组合策略测试');
    console.log('='.repeat(80));
    console.log(`数据: ${data.length}条K线, ${data[0].open_time} ~ ${data[data.length - 1].open_time}\n`);

    const strategies = [
        { name: '1. 原版(穿越中轨)', mode: 'original' },
        { name: '2. 布林轨反弹', mode: 'boll_rebound_only' },
        { name: '3. 智能切换', mode: 'smart_switch' },
        { name: '4. 趋势骑手', mode: 'trend_rider' },
        { name: '5. 移动止盈+反弹', mode: 'trailing_with_rebound' },
    ];

    const results = [];

    strategies.forEach(s => {
        const result = backtest(data, CONFIG, s);
        const stats = analyze(result, s.name);
        results.push(stats);
    });

    // 输出对比表格
    console.log('\n' +
        '策略'.padEnd(22) +
        '交易'.padEnd(6) +
        '胜率'.padEnd(8) +
        '盈亏比'.padEnd(8) +
        '总盈亏'.padEnd(10) +
        '最大回撤'.padEnd(10) +
        '连亏'.padEnd(6) +
        '持仓'.padEnd(8) +
        '月度盈利'
    );
    console.log('-'.repeat(90));

    results.forEach(r => {
        if (r) {
            console.log(
                r.label.padEnd(22) +
                `${r.trades}`.padEnd(6) +
                `${(r.winRate * 100).toFixed(1)}%`.padEnd(8) +
                `${r.profitFactor.toFixed(2)}`.padEnd(8) +
                `${r.totalPnl >= 0 ? '+' : ''}${r.totalPnl.toFixed(2)}`.padEnd(10) +
                `${r.maxDrawdown.toFixed(2)}`.padEnd(10) +
                `${r.maxConsecLoss}`.padEnd(6) +
                `${r.avgHoldBars.toFixed(1)}`.padEnd(8) +
                `${(r.monthlyProfitRate * 100).toFixed(0)}%`
            );
        }
    });

    // 找出最佳策略
    const validResults = results.filter(r => r && r.trades >= 10);
    if (validResults.length > 0) {
        validResults.sort((a, b) => b.totalPnl - a.totalPnl);
        const best = validResults[0];

        console.log('\n' + '='.repeat(80));
        console.log('🏆 最佳策略');
        console.log('='.repeat(80));
        console.log(`策略: ${best.label}`);
        console.log(`总盈亏: ${best.totalPnl >= 0 ? '+' : ''}${best.totalPnl.toFixed(2)} USDT`);
        console.log(`胜率: ${(best.winRate * 100).toFixed(1)}%`);
        console.log(`盈亏比: ${best.profitFactor.toFixed(2)}`);
        console.log(`最大回撤: ${best.maxDrawdown.toFixed(2)} USDT`);

        // 详细分析最佳策略
        const bestResult = strategies.find(s => s.name === best.label);
        if (bestResult) {
            console.log('\n📊 出场原因分析:');
            Object.entries(best.exitReasons).forEach(([reason, data]) => {
                const winRate = data.count > 0 ? (data.wins / data.count * 100).toFixed(1) : 0;
                const avgPnl = data.count > 0 ? (data.pnl / data.count).toFixed(4) : 0;
                console.log(`  ${reason}: ${data.count}笔, 胜率${winRate}%, 平均${avgPnl >= 0 ? '+' : ''}${avgPnl}`);
            });
        }
    }

    // 输出结论
    console.log('\n' + '='.repeat(80));
    console.log('📌 结论');
    console.log('='.repeat(80));

    const original = results.find(r => r && r.label.includes('原版'));
    const best = validResults[0];

    if (original && best && best.label !== original.label) {
        const pnlDiff = best.totalPnl - original.totalPnl;
        const winRateDiff = best.winRate - original.winRate;

        console.log(`\n最佳策略 "${best.label}" vs 原版:`);
        console.log(`  盈亏变化: ${pnlDiff >= 0 ? '+' : ''}${pnlDiff.toFixed(2)} USDT`);
        console.log(`  胜率变化: ${winRateDiff >= 0 ? '+' : ''}${(winRateDiff * 100).toFixed(1)}%`);
        console.log(`  回撤变化: ${best.maxDrawdown - original.maxDrawdown} USDT`);

        if (pnlDiff > 0) {
            console.log('\n✅ 组合策略优于原版！');
        } else {
            console.log('\n⚠️ 组合策略未能显著改善');
        }
    }
}

main();
