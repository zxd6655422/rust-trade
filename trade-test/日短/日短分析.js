/**
 * 日短策略回测分析
 * 策略逻辑：
 * 1. 5分钟K线，指标：MA48、MA288、MA488、布林带(中轨100,标准差倍数2.0)
 * 2. 趋势判断：MA48持续在MA288上方且上升+布林中轨上升 → 做多意图；反之做空意图
 * 3. 入场：价格穿越布林中轨
 * 4. 出场：价格反向穿越布林中轨
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
    // 趋势判断需要连续确认的K线数
    trend_confirm_bars: 3,
};

// ============ 数据加载 ============
function loadCSV(filePath) {
    const content = fs.readFileSync(filePath, 'utf-8');
    const lines = content.trim().split('\n');
    const header = lines[0].split(',').map(h => h.replace(/"/g, '').trim());

    const data = [];
    for (let i = 1; i < lines.length; i++) {
        const values = lines[i].split(',');
        if (values.length < 7) continue;

        const row = {
            symbol: values[0].trim(),
            open_time: values[1].trim(),
            open: parseFloat(values[2]),
            high: parseFloat(values[3]),
            low: parseFloat(values[4]),
            close: parseFloat(values[5]),
            volume: parseFloat(values[6]),
            trade_count: parseInt(values[7]) || 0,
        };
        data.push(row);
    }

    // 按时间正序排列（从旧到新）
    data.sort((a, b) => new Date(a.open_time) - new Date(b.open_time));
    return data;
}

// ============ 技术指标计算 ============
function calcSMA(data, period, field = 'close') {
    const result = new Array(data.length).fill(null);
    for (let i = period - 1; i < data.length; i++) {
        let sum = 0;
        for (let j = 0; j < period; j++) {
            sum += data[i - j][field];
        }
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
        for (let j = 0; j < period; j++) {
            sum += data[i - j].close;
        }
        const ma = sum / period;
        middle[i] = ma;

        let sqSum = 0;
        for (let j = 0; j < period; j++) {
            sqSum += Math.pow(data[i - j].close - ma, 2);
        }
        const std = Math.sqrt(sqSum / period);
        upper[i] = ma + stdMult * std;
        lower[i] = ma - stdMult * std;
    }

    return { middle, upper, lower };
}

// ============ 趋势判断 ============
function getTrendIntent(ma48, ma288, bollMiddle, index, confirmBars) {
    if (index < confirmBars) return 'neutral';

    let bullCount = 0;
    let bearCount = 0;

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

    // 计算指标
    const ma48 = calcSMA(data, config.ma48_period);
    const ma288 = calcSMA(data, config.ma288_period);
    const ma488 = calcSMA(data, config.ma488_period);
    const boll = calcBollinger(data, config.bollinger_period, config.bollinger_std_mult);

    // 交易状态
    let position = 0; // 1=多, -1=空, 0=无
    let entryPrice = 0;
    let entryTime = '';

    // 交易记录
    const trades = [];
    let totalPnl = 0;
    let winCount = 0;
    let lossCount = 0;

    // 从有足够的指标数据开始
    const startIdx = Math.max(config.ma488_period, config.bollinger_period) + config.trend_confirm_bars;

    for (let i = startIdx; i < n; i++) {
        const bar = data[i];
        const prevBar = data[i - 1];

        // 获取当前指标值
        const currentMa48 = ma48[i];
        const currentMa288 = ma288[i];
        const currentMa488 = ma488[i];
        const currentBollMid = boll.middle[i];

        if (currentMa48 === null || currentMa288 === null || currentBollMid === null) continue;

        // 趋势判断
        const trend = getTrendIntent(ma48, ma288, boll.middle, i, config.trend_confirm_bars);

        // 当前K线的开收盘价相对于布林中轨
        const openAboveMid = bar.open > currentBollMid;
        const closeAboveMid = bar.close > currentBollMid;

        // ===== 开仓逻辑 =====
        if (position === 0) {
            // 做多条件：有做多意图，开盘<中轨，收盘>中轨
            if (trend === 'bull' && !openAboveMid && closeAboveMid) {
                position = 1;
                entryPrice = bar.close;
                entryTime = bar.open_time;
            }
            // 做空条件：有做空意图，开盘>中轨，收盘<中轨
            else if (trend === 'bear' && openAboveMid && !closeAboveMid) {
                position = -1;
                entryPrice = bar.close;
                entryTime = bar.open_time;
            }
        }
        // ===== 平仓逻辑 =====
        else if (position === 1) {
            // 多单平仓：开盘>中轨，收盘<中轨
            if (openAboveMid && !closeAboveMid) {
                const pnl = bar.close - entryPrice;
                totalPnl += pnl;
                if (pnl > 0) winCount++; else lossCount++;

                trades.push({
                    type: 'LONG',
                    entryTime,
                    entryPrice,
                    exitTime: bar.open_time,
                    exitPrice: bar.close,
                    pnl,
                    pnlPercent: (pnl / entryPrice * 100).toFixed(4),
                });

                position = 0;
            }
        }
        else if (position === -1) {
            // 空单平仓：开盘<中轨，收盘>中轨
            if (!openAboveMid && closeAboveMid) {
                const pnl = entryPrice - bar.close;
                totalPnl += pnl;
                if (pnl > 0) winCount++; else lossCount++;

                trades.push({
                    type: 'SHORT',
                    entryTime,
                    entryPrice,
                    exitTime: bar.open_time,
                    exitPrice: bar.close,
                    pnl,
                    pnlPercent: (pnl / entryPrice * 100).toFixed(4),
                });

                position = 0;
            }
        }
    }

    // 如果还有持仓，以最后价格平仓
    if (position !== 0) {
        const lastBar = data[n - 1];
        const pnl = position === 1
            ? lastBar.close - entryPrice
            : entryPrice - lastBar.close;
        totalPnl += pnl;
        if (pnl > 0) winCount++; else lossCount++;

        trades.push({
            type: position === 1 ? 'LONG' : 'SHORT',
            entryTime,
            entryPrice,
            exitTime: lastBar.open_time,
            exitPrice: lastBar.close,
            pnl,
            pnlPercent: (pnl / entryPrice * 100).toFixed(4),
            note: '强制平仓',
        });
    }

    return {
        trades,
        summary: {
            totalTrades: trades.length,
            winCount,
            lossCount,
            winRate: trades.length > 0 ? (winCount / trades.length * 100).toFixed(2) : 0,
            totalPnl: totalPnl.toFixed(4),
            avgPnl: trades.length > 0 ? (totalPnl / trades.length).toFixed(4) : 0,
        },
        indicators: {
            ma48: ma48.slice(-10),
            ma288: ma288.slice(-10),
            bollMiddle: boll.middle.slice(-10),
        }
    };
}

// ============ 主程序 ============
function main() {
    const csvPath = path.resolve('F:/rust_projects/trade/src/kline_5m_202607232011.csv');

    console.log('='.repeat(60));
    console.log('日短策略回测分析');
    console.log('='.repeat(60));
    console.log(`数据文件: ${csvPath}`);

    // 加载数据
    const data = loadCSV(csvPath);
    console.log(`数据条数: ${data.length}`);
    console.log(`时间范围: ${data[0].open_time} ~ ${data[data.length - 1].open_time}`);
    console.log('='.repeat(60));

    // 执行回测
    const result = backtest(data, CONFIG);

    // 输出结果
    console.log('\n【回测结果】');
    console.log(`总交易次数: ${result.summary.totalTrades}`);
    console.log(`盈利次数: ${result.summary.winCount}`);
    console.log(`亏损次数: ${result.summary.lossCount}`);
    console.log(`胜率: ${result.summary.winRate}%`);
    console.log(`总盈亏: ${result.summary.totalPnl}`);
    console.log(`平均盈亏: ${result.summary.avgPnl}`);

    if (result.trades.length > 0) {
        console.log('\n【交易明细】');
        result.trades.forEach((t, i) => {
            console.log(`\n#${i + 1} ${t.type}`);
            console.log(`  入场: ${t.entryTime} @ ${t.entryPrice}`);
            console.log(`  出场: ${t.exitTime} @ ${t.exitPrice}`);
            console.log(`  盈亏: ${t.pnl.toFixed(4)} (${t.pnlPercent}%)`);
            if (t.note) console.log(`  备注: ${t.note}`);
        });
    }

    // 输出最新指标
    console.log('\n【最新指标值】');
    const lastIdx = data.length - 1;
    const lastBar = data[lastIdx];
    console.log(`最新K线: ${lastBar.open_time}`);
    console.log(`  O=${lastBar.open} H=${lastBar.high} L=${lastBar.low} C=${lastBar.close}`);

    // 趋势判断
    const trend = getTrendIntent(
        calcSMA(data, CONFIG.ma48_period),
        calcSMA(data, CONFIG.ma288_period),
        calcBollinger(data, CONFIG.bollinger_period, CONFIG.bollinger_std_mult).middle,
        lastIdx,
        CONFIG.trend_confirm_bars
    );
    console.log(`当前趋势意图: ${trend === 'bull' ? '做多' : trend === 'bear' ? '做空' : '中性'}`);

    console.log('\n' + '='.repeat(60));
    console.log('分析完成');
}

main();
