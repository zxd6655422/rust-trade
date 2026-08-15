/**
 * 移动止盈测试
 * 测试不同的移动止盈参数对策略的影响
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

// ============ 回测引擎 - 带移动止盈 ============
function backtestWithTrailingStop(data, config, trailingConfig) {
    const n = data.length;
    const ma48 = calcSMA(data, config.ma48_period);
    const ma288 = calcSMA(data, config.ma288_period);
    const boll = calcBollinger(data, config.bollinger_period, config.bollinger_std_mult);

    let position = 0, entryPrice = 0, entryTime = '', entryIdx = 0;
    let highestPnl = 0, lowestPnl = 0; // 用于移动止盈
    let trailingActive = false; // 移动止盈是否激活

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

        // 持仓中 - 计算当前盈亏
        if (position !== 0) {
            const currentPnl = position === 1
                ? bar.close - entryPrice
                : entryPrice - bar.close;
            const currentPnlPercent = currentPnl / entryPrice * 100;

            // 更新最高/最低盈亏
            if (position === 1) {
                if (currentPnl > highestPnl) highestPnl = currentPnl;
            } else {
                if (currentPnl > lowestPnl) lowestPnl = currentPnl;
            }

            // 检查移动止盈条件
            if (trailingConfig.enabled) {
                const peakPnl = position === 1 ? highestPnl : lowestPnl;
                const peakPnlPercent = peakPnl / entryPrice * 100;

                // 激活条件：盈利达到激活阈值
                if (!trailingActive && peakPnlPercent >= trailingConfig.activatePercent) {
                    trailingActive = true;
                }

                // 止盈条件：从最高盈利回撤超过回撤比例
                if (trailingActive) {
                    const drawdownFromPeak = peakPnl - currentPnl;
                    const drawdownPercent = drawdownFromPeak / entryPrice * 100;

                    if (drawdownPercent >= trailingConfig.drawdownPercent) {
                        // 移动止盈触发
                        const exitPrice = bar.close;
                        const pnl = currentPnl;
                        const holdBars = i - entryIdx;

                        totalPnl += pnl;
                        if (pnl > 0) winCount++; else lossCount++;

                        trades.push({
                            type: position === 1 ? 'LONG' : 'SHORT',
                            entryTime, entryPrice, exitTime: bar.open_time, exitPrice,
                            pnl, pnlPercent: pnl / entryPrice * 100, holdBars,
                            exitReason: 'trailing_stop',
                            peakPnl, peakPnlPercent,
                        });

                        position = 0;
                        trailingActive = false;
                        highestPnl = 0;
                        lowestPnl = 0;
                        continue;
                    }
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
                    peakPnl: highestPnl, peakPnlPercent: highestPnl / entryPrice * 100,
                });

                position = 0;
                trailingActive = false;
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
                    peakPnl: lowestPnl, peakPnlPercent: lowestPnl / entryPrice * 100,
                });

                position = 0;
                trailingActive = false;
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
                highestPnl = 0;
                lowestPnl = 0;
                trailingActive = false;
            } else if (trend === 'bear' && openAboveMid && !closeAboveMid) {
                position = -1;
                entryPrice = bar.close;
                entryTime = bar.open_time;
                entryIdx = i;
                highestPnl = 0;
                lowestPnl = 0;
                trailingActive = false;
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

// ============ 统计函数 ============
function calcStats(result, label) {
    const trades = result.trades;
    if (trades.length === 0) return { label, trades: 0 };

    const wins = trades.filter(t => t.pnl > 0);
    const losses = trades.filter(t => t.pnl <= 0);
    const avgWin = wins.length > 0 ? wins.reduce((s, t) => s + t.pnl, 0) / wins.length : 0;
    const avgLoss = losses.length > 0 ? losses.reduce((s, t) => s + t.pnl, 0) / losses.length : 0;
    const profitFactor = avgLoss !== 0 ? Math.abs(avgWin / avgLoss) : Infinity;

    let maxConsecLoss = 0, curLoss = 0;
    trades.forEach(t => {
        if (t.pnl <= 0) { curLoss++; maxConsecLoss = Math.max(maxConsecLoss, curLoss); }
        else curLoss = 0;
    });

    let peak = 0, maxDrawdown = 0, currentPnl = 0;
    trades.forEach(t => {
        currentPnl += t.pnl;
        if (currentPnl > peak) peak = currentPnl;
        const dd = peak - currentPnl;
        if (dd > maxDrawdown) maxDrawdown = dd;
    });

    // 平均持仓时间
    const avgHoldBars = trades.reduce((s, t) => s + t.holdBars, 0) / trades.length;

    // 月度统计
    const monthlyData = {};
    trades.forEach(t => {
        const month = t.exitTime.substring(0, 7);
        if (!monthlyData[month]) monthlyData[month] = { pnl: 0 };
        monthlyData[month].pnl += t.pnl;
    });
    const profitableMonths = Object.values(monthlyData).filter(m => m.pnl >= 0).length;
    const totalMonths = Object.keys(monthlyData).length;

    // 移动止盈触发次数
    const trailingStopCount = trades.filter(t => t.exitReason === 'trailing_stop').length;
    const bollingerCrossCount = trades.filter(t => t.exitReason === 'bollinger_cross').length;

    // 移动止盈交易的平均盈利
    const trailingTrades = trades.filter(t => t.exitReason === 'trailing_stop');
    const trailingPnl = trailingTrades.reduce((s, t) => s + t.pnl, 0);

    return {
        label,
        trades: trades.length,
        winRate: result.winCount / trades.length,
        profitFactor,
        totalPnl: result.totalPnl,
        maxDrawdown,
        maxConsecLoss,
        avgHoldBars,
        monthlyProfitRate: totalMonths > 0 ? profitableMonths / totalMonths : 0,
        trailingStopCount,
        bollingerCrossCount,
        trailingPnl,
    };
}

// ============ 参数优化测试 ============
function optimizeParameters(data, config) {
    console.log('='.repeat(70));
    console.log('移动止盈参数优化测试');
    console.log('='.repeat(70));

    const testCases = [
        { name: '原版(无移动止盈)', trailing: { enabled: false } },
        // 激活1%, 回撤0.3%
        { name: '激活1% 回撤0.3%', trailing: { enabled: true, activatePercent: 1, drawdownPercent: 0.3 } },
        // 激活1%, 回撤0.5%
        { name: '激活1% 回撤0.5%', trailing: { enabled: true, activatePercent: 1, drawdownPercent: 0.5 } },
        // 激活1%, 回撤1%
        { name: '激活1% 回撤1%', trailing: { enabled: true, activatePercent: 1, drawdownPercent: 1 } },
        // 激活0.5%, 回撤0.2%
        { name: '激活0.5% 回撤0.2%', trailing: { enabled: true, activatePercent: 0.5, drawdownPercent: 0.2 } },
        // 激活0.5%, 回撤0.3%
        { name: '激活0.5% 回撤0.3%', trailing: { enabled: true, activatePercent: 0.5, drawdownPercent: 0.3 } },
        // 激活0.5%, 回撤0.5%
        { name: '激活0.5% 回撤0.5%', trailing: { enabled: true, activatePercent: 0.5, drawdownPercent: 0.5 } },
        // 激活2%, 回撤0.5%
        { name: '激活2% 回撤0.5%', trailing: { enabled: true, activatePercent: 2, drawdownPercent: 0.5 } },
        // 激活2%, 回撤1%
        { name: '激活2% 回撤1%', trailing: { enabled: true, activatePercent: 2, drawdownPercent: 1 } },
        // 激活0.3%, 回撤0.15%
        { name: '激活0.3% 回撤0.15%', trailing: { enabled: true, activatePercent: 0.3, drawdownPercent: 0.15 } },
        // 激活1.5%, 回撤0.5%
        { name: '激活1.5% 回撤0.5%', trailing: { enabled: true, activatePercent: 1.5, drawdownPercent: 0.5 } },
        // 激活1.5%, 回撤0.8%
        { name: '激活1.5% 回撤0.8%', trailing: { enabled: true, activatePercent: 1.5, drawdownPercent: 0.8 } },
    ];

    const results = [];

    testCases.forEach(tc => {
        const result = backtestWithTrailingStop(data, config, tc.trailing);
        const stats = calcStats(result, tc.name);
        results.push(stats);
    });

    // 输出表格
    console.log('\n' +
        '策略'.padEnd(22) +
        '交易'.padEnd(6) +
        '胜率'.padEnd(8) +
        '盈亏比'.padEnd(8) +
        '总盈亏'.padEnd(10) +
        '最大回撤'.padEnd(10) +
        '连亏'.padEnd(6) +
        '持仓'.padEnd(8) +
        '止盈触发'.padEnd(10) +
        '止盈盈亏'
    );
    console.log('-'.repeat(100));

    results.forEach(r => {
        if (r.trades > 0) {
            console.log(
                r.label.padEnd(22) +
                `${r.trades}`.padEnd(6) +
                `${(r.winRate * 100).toFixed(1)}%`.padEnd(8) +
                `${r.profitFactor.toFixed(2)}`.padEnd(8) +
                `${r.totalPnl >= 0 ? '+' : ''}${r.totalPnl.toFixed(2)}`.padEnd(10) +
                `${r.maxDrawdown.toFixed(2)}`.padEnd(10) +
                `${r.maxConsecLoss}`.padEnd(6) +
                `${r.avgHoldBars.toFixed(1)}`.padEnd(8) +
                `${r.trailingStopCount}`.padEnd(10) +
                `${r.trailingPnl >= 0 ? '+' : ''}${r.trailingPnl.toFixed(2)}`
            );
        }
    });

    // 找出最佳策略
    const validResults = results.filter(r => r.trades >= 10);
    if (validResults.length > 0) {
        // 按总盈亏排序
        validResults.sort((a, b) => b.totalPnl - a.totalPnl);
        const best = validResults[0];

        console.log('\n' + '='.repeat(70));
        console.log('🏆 最佳参数组合');
        console.log('='.repeat(70));
        console.log(`策略: ${best.label}`);
        console.log(`总盈亏: ${best.totalPnl >= 0 ? '+' : ''}${best.totalPnl.toFixed(2)} USDT`);
        console.log(`胜率: ${(best.winRate * 100).toFixed(1)}%`);
        console.log(`盈亏比: ${best.profitFactor.toFixed(2)}`);
        console.log(`交易次数: ${best.trades}`);
        console.log(`最大回撤: ${best.maxDrawdown.toFixed(2)} USDT`);
        console.log(`平均持仓: ${best.avgHoldBars.toFixed(1)}根K线`);
        console.log(`移动止盈触发: ${best.trailingStopCount}次`);

        return best;
    }

    return null;
}

// ============ 详细分析最佳策略 ============
function analyzeBestStrategy(data, config, trailingConfig) {
    console.log('\n' + '='.repeat(70));
    console.log('最佳策略详细分析');
    console.log('='.repeat(70));

    const result = backtestWithTrailingStop(data, config, trailingConfig);
    const trades = result.trades;

    // 按出场原因分类
    const trailingTrades = trades.filter(t => t.exitReason === 'trailing_stop');
    const bollingerTrades = trades.filter(t => t.exitReason === 'bollinger_cross');

    console.log('\n📊 出场原因分析');
    console.log(`  移动止盈出场: ${trailingTrades.length}笔`);
    console.log(`  布林中轨出场: ${bollingerTrades.length}笔`);

    if (trailingTrades.length > 0) {
        const trailingWins = trailingTrades.filter(t => t.pnl > 0);
        const trailingPnl = trailingTrades.reduce((s, t) => s + t.pnl, 0);
        console.log(`\n  移动止盈交易:`);
        console.log(`    盈利: ${trailingWins.length}笔 (${(trailingWins.length / trailingTrades.length * 100).toFixed(1)}%)`);
        console.log(`    总盈亏: ${trailingPnl >= 0 ? '+' : ''}${trailingPnl.toFixed(2)} USDT`);
        console.log(`    平均盈亏: ${(trailingPnl / trailingTrades.length).toFixed(4)} USDT`);
    }

    if (bollingerTrades.length > 0) {
        const bollingerWins = bollingerTrades.filter(t => t.pnl > 0);
        const bollingerPnl = bollingerTrades.reduce((s, t) => s + t.pnl, 0);
        console.log(`\n  布林中轨出场:`);
        console.log(`    盈利: ${bollingerWins.length}笔 (${(bollingerWins.length / bollingerTrades.length * 100).toFixed(1)}%)`);
        console.log(`    总盈亏: ${bollingerPnl >= 0 ? '+' : ''}${bollingerPnl.toFixed(2)} USDT`);
    }

    // 移动止盈交易的峰值盈利分析
    if (trailingTrades.length > 0) {
        console.log('\n📈 移动止盈交易峰值分析');
        const avgPeak = trailingTrades.reduce((s, t) => s + (t.peakPnlPercent || 0), 0) / trailingTrades.length;
        const avgExit = trailingTrades.reduce((s, t) => s + t.pnlPercent, 0) / trailingTrades.length;
        console.log(`  平均峰值盈利: ${avgPeak.toFixed(2)}%`);
        console.log(`  平均出场盈利: ${avgExit.toFixed(2)}%`);
        console.log(`  平均回吐: ${(avgPeak - avgExit).toFixed(2)}%`);
    }

    // 最近10笔交易
    console.log('\n📋 最近10笔交易');
    trades.slice(-10).forEach((t, i) => {
        const idx = trades.length - 10 + i + 1;
        const emoji = t.pnl > 0 ? '✅' : '❌';
        const reason = t.exitReason === 'trailing_stop' ? '🎯' : '📊';
        console.log(`  ${emoji}${reason} #${idx} ${t.type.padEnd(5)} ${t.entryTime.substring(5, 16)} → ${t.exitTime.substring(5, 16)} | ${t.pnl >= 0 ? '+' : ''}${t.pnl.toFixed(4)} (${t.pnlPercent >= 0 ? '+' : ''}${t.pnlPercent.toFixed(2)}%)`);
    });

    return result;
}

// ============ 主程序 ============
function main() {
    const csvPath = path.resolve('F:/rust_projects/trade/src/kline_5m_202607232011.csv');
    const data = loadCSV(csvPath);

    console.log('日短策略 - 移动止盈测试');
    console.log(`数据: ${data.length}条K线, ${data[0].open_time} ~ ${data[data.length - 1].open_time}\n`);

    const best = optimizeParameters(data, CONFIG);

    if (best) {
        // 找到最佳的trailingConfig
        const bestTrailing = best.label === '原版(无移动止盈)'
            ? { enabled: false }
            : parseTrailingConfig(best.label);

        if (bestTrailing) {
            analyzeBestStrategy(data, CONFIG, bestTrailing);
        }
    }
}

// 解析移动止盈配置
function parseTrailingConfig(label) {
    const match = label.match(/激活(\d+\.?\d*)% 回撤(\d+\.?\d*)%/);
    if (match) {
        return {
            enabled: true,
            activatePercent: parseFloat(match[1]),
            drawdownPercent: parseFloat(match[2]),
        };
    }
    return null;
}

main();
