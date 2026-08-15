/**
 * 止损幅度分析 - 分析每笔交易的盈亏幅度
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

    for (let i = period - 1; i < data.length; i++) {
        let sum = 0;
        for (let j = 0; j < period; j++) sum += data[i - j].close;
        const ma = sum / period;
        middle[i] = ma;

        let sqSum = 0;
        for (let j = 0; j < period; j++) sqSum += Math.pow(data[i - j].close - ma, 2);
        const std = Math.sqrt(sqSum / period);
    }
    return { middle };
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

        // 计算入场时的布林带宽度（用于估算止损幅度）
        const bollMidAtEntry = boll.middle[entryIdx] || 0;

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
            const pnlPercent = pnl / entryPrice * 100;
            const holdBars = i - entryIdx;

            // 计算持仓期间的最大不利变动（实际止损幅度）
            let maxAdverse = 0;
            for (let j = entryIdx; j <= i; j++) {
                const adverse = entryPrice - data[j].low; // 做多时的不利变动
                if (adverse > maxAdverse) maxAdverse = adverse;
            }

            trades.push({
                type: 'LONG',
                entryTime, entryPrice, entryIdx,
                exitTime: bar.open_time, exitPrice, exitIdx: i,
                pnl, pnlPercent, holdBars,
                maxAdverse, maxAdversePercent: maxAdverse / entryPrice * 100,
                bollMidAtEntry: boll.middle[entryIdx],
                bollMidAtExit: currentBollMid,
                entryToBollMid: Math.abs(entryPrice - boll.middle[entryIdx]),
            });
            position = 0;
        }
        else if (position === -1 && !openAboveMid && closeAboveMid) {
            const exitPrice = bar.close;
            const pnl = entryPrice - exitPrice;
            const pnlPercent = pnl / entryPrice * 100;
            const holdBars = i - entryIdx;

            // 计算持仓期间的最大不利变动
            let maxAdverse = 0;
            for (let j = entryIdx; j <= i; j++) {
                const adverse = data[j].high - entryPrice; // 做空时的不利变动
                if (adverse > maxAdverse) maxAdverse = adverse;
            }

            trades.push({
                type: 'SHORT',
                entryTime, entryPrice, entryIdx,
                exitTime: bar.open_time, exitPrice, exitIdx: i,
                pnl, pnlPercent, holdBars,
                maxAdverse, maxAdversePercent: maxAdverse / entryPrice * 100,
                bollMidAtEntry: boll.middle[entryIdx],
                bollMidAtExit: currentBollMid,
                entryToBollMid: Math.abs(entryPrice - boll.middle[entryIdx]),
            });
            position = 0;
        }
    }

    return trades;
}

// ============ 深度分析 ============
function deepAnalysis(trades) {
    console.log('='.repeat(70));
    console.log('【止损幅度分析报告】');
    console.log('='.repeat(70));

    if (trades.length === 0) {
        console.log('无交易记录');
        return;
    }

    const wins = trades.filter(t => t.pnl > 0);
    const losses = trades.filter(t => t.pnl <= 0);

    // 1. 基础统计
    console.log('\n📊 一、整体盈亏分布');
    console.log(`  总交易次数: ${trades.length}`);
    console.log(`  盈利: ${wins.length}笔 (${(wins.length / trades.length * 100).toFixed(1)}%)`);
    console.log(`  亏损: ${losses.length}笔 (${(losses.length / trades.length * 100).toFixed(1)}%)`);

    // 2. 盈亏幅度分析
    console.log('\n📈 二、盈亏幅度分析');

    const allPnlPercent = trades.map(t => t.pnlPercent);
    const winPnlPercent = wins.map(t => t.pnlPercent);
    const lossPnlPercent = losses.map(t => t.pnlPercent);

    console.log('\n  所有交易:');
    console.log(`    平均盈亏: ${mean(allPnlPercent).toFixed(3)}%`);
    console.log(`    中位数: ${median(allPnlPercent).toFixed(3)}%`);
    console.log(`    最大盈利: ${Math.max(...allPnlPercent).toFixed(3)}%`);
    console.log(`    最大亏损: ${Math.min(...allPnlPercent).toFixed(3)}%`);

    if (winPnlPercent.length > 0) {
        console.log('\n  盈利交易:');
        console.log(`    平均盈利: +${mean(winPnlPercent).toFixed(3)}%`);
        console.log(`    中位数: +${median(winPnlPercent).toFixed(3)}%`);
        console.log(`    最大盈利: +${Math.max(...winPnlPercent).toFixed(3)}%`);
        console.log(`    最小盈利: +${Math.min(...winPnlPercent).toFixed(3)}%`);
    }

    if (lossPnlPercent.length > 0) {
        console.log('\n  亏损交易:');
        console.log(`    平均亏损: ${mean(lossPnlPercent).toFixed(3)}%`);
        console.log(`    中位数: ${median(lossPnlPercent).toFixed(3)}%`);
        console.log(`    最大亏损: ${Math.min(...lossPnlPercent).toFixed(3)}%`);
        console.log(`    最小亏损: ${Math.max(...lossPnlPercent).toFixed(3)}%`);
    }

    // 3. 止损幅度分布
    console.log('\n📊 三、止损幅度分布（亏损交易）');

    const lossRanges = [
        { label: '< 0.1%', min: -Infinity, max: -0.1 },
        { label: '0.1% ~ 0.2%', min: -0.2, max: -0.1 },
        { label: '0.2% ~ 0.3%', min: -0.3, max: -0.2 },
        { label: '0.3% ~ 0.5%', min: -0.5, max: -0.3 },
        { label: '0.5% ~ 1%', min: -1, max: -0.5 },
        { label: '1% ~ 2%', min: -2, max: -1 },
        { label: '> 2%', min: -Infinity, max: -2 },
    ];

    lossRanges.forEach(r => {
        const count = losses.filter(t => t.pnlPercent >= r.min && t.pnlPercent < r.max).length;
        const bar = '█'.repeat(Math.ceil(count / 2));
        const percent = (count / losses.length * 100).toFixed(1);
        console.log(`  ${r.label.padEnd(15)}: ${count.toString().padStart(3)}笔 (${percent.padStart(5)}%) ${bar}`);
    });

    // 4. 盈利幅度分布
    console.log('\n📊 四、盈利幅度分布（盈利交易）');

    const winRanges = [
        { label: '< 0.1%', min: 0, max: 0.1 },
        { label: '0.1% ~ 0.2%', min: 0.1, max: 0.2 },
        { label: '0.2% ~ 0.5%', min: 0.2, max: 0.5 },
        { label: '0.5% ~ 1%', min: 0.5, max: 1 },
        { label: '1% ~ 2%', min: 1, max: 2 },
        { label: '> 2%', min: 2, max: Infinity },
    ];

    winRanges.forEach(r => {
        const count = wins.filter(t => t.pnlPercent >= r.min && t.pnlPercent < r.max).length;
        const bar = '█'.repeat(Math.ceil(count / 2));
        const percent = count > 0 ? (count / wins.length * 100).toFixed(1) : '0.0';
        console.log(`  ${r.label.padEnd(15)}: ${count.toString().padStart(3)}笔 (${percent.padStart(5)}%) ${bar}`);
    });

    // 5. 入场价与布林中轨的距离
    console.log('\n📊 五、入场价与布林中轨的距离');

    const distances = trades.map(t => t.entryToBollMid / t.entryPrice * 100);
    console.log(`  平均距离: ${mean(distances).toFixed(3)}%`);
    console.log(`  中位数: ${median(distances).toFixed(3)}%`);
    console.log(`  最大距离: ${Math.max(...distances).toFixed(3)}%`);
    console.log(`  最小距离: ${Math.min(...distances).toFixed(3)}%`);

    // 6. 持仓期间最大不利变动分析
    console.log('\n📊 六、持仓期间最大不利变动（浮亏）');

    const allMaxAdverse = trades.map(t => t.maxAdversePercent);
    const winMaxAdverse = wins.map(t => t.maxAdversePercent);
    const lossMaxAdverse = losses.map(t => t.maxAdversePercent);

    console.log('\n  所有交易:');
    console.log(`    平均最大浮亏: ${mean(allMaxAdverse).toFixed(3)}%`);
    console.log(`    最大浮亏: ${Math.max(...allMaxAdverse).toFixed(3)}%`);

    console.log('\n  盈利交易:');
    console.log(`    平均最大浮亏: ${mean(winMaxAdverse).toFixed(3)}%`);
    console.log(`    最大浮亏: ${Math.max(...winMaxAdverse).toFixed(3)}%`);

    console.log('\n  亏损交易:');
    console.log(`    平均最大浮亏: ${mean(lossMaxAdverse).toFixed(3)}%`);
    console.log(`    最大浮亏: ${Math.max(...lossMaxAdverse).toFixed(3)}%`);

    // 7. 盈亏比分析
    console.log('\n📊 七、盈亏比详细分析');

    const avgWin = mean(winPnlPercent);
    const avgLoss = Math.abs(mean(lossPnlPercent));
    const profitFactor = avgLoss > 0 ? avgWin / avgLoss : Infinity;

    console.log(`  平均盈利: +${avgWin.toFixed(3)}%`);
    console.log(`  平均亏损: -${avgLoss.toFixed(3)}%`);
    console.log(`  盈亏比: ${profitFactor.toFixed(2)}`);

    // 中位数盈亏比
    const medianWin = median(winPnlPercent);
    const medianLoss = Math.abs(median(lossPnlPercent));
    const medianProfitFactor = medianLoss > 0 ? medianWin / medianLoss : Infinity;

    console.log(`  中位数盈利: +${medianWin.toFixed(3)}%`);
    console.log(`  中位数亏损: -${medianLoss.toFixed(3)}%`);
    console.log(`  中位数盈亏比: ${medianProfitFactor.toFixed(2)}`);

    // 8. 特殊情况分析
    console.log('\n📊 八、特殊情况分析');

    // 大亏损交易（超过1%）
    const bigLosses = losses.filter(t => t.pnlPercent < -1);
    console.log(`\n  大亏损交易（>1%）: ${bigLosses.length}笔`);
    if (bigLosses.length > 0) {
        bigLosses.forEach(t => {
            console.log(`    ${t.type} ${t.entryTime.substring(5, 16)} | ${t.pnlPercent.toFixed(3)}% | 最大浮亏: ${t.maxAdversePercent.toFixed(3)}%`);
        });
    }

    // 大盈利交易（超过2%）
    const bigWins = wins.filter(t => t.pnlPercent > 2);
    console.log(`\n  大盈利交易（>2%）: ${bigWins.length}笔`);
    if (bigWins.length > 0) {
        bigWins.forEach(t => {
            console.log(`    ${t.type} ${t.entryTime.substring(5, 16)} | +${t.pnlPercent.toFixed(3)}% | 最大浮亏: ${t.maxAdversePercent.toFixed(3)}%`);
        });
    }

    // 9. 总结
    console.log('\n' + '='.repeat(70));
    console.log('📌 总结');
    console.log('='.repeat(70));

    const medianLossPercent = Math.abs(median(lossPnlPercent));
    const avgLossPercent = Math.abs(mean(lossPnlPercent));

    console.log(`\n  止损幅度统计:`);
    console.log(`    平均止损: -${avgLossPercent.toFixed(3)}%`);
    console.log(`    中位数止损: -${medianLossPercent.toFixed(3)}%`);
    console.log(`    最大止损: ${Math.min(...lossPnlPercent).toFixed(3)}%`);

    console.log(`\n  止损特点:`);
    if (medianLossPercent < 0.2) {
        console.log('    ✅ 止损幅度很小（中位数<0.2%），因为穿越中轨就止损');
        console.log('    ✅ 这是布林中轨策略的优势：止损可控');
    } else if (medianLossPercent < 0.5) {
        console.log('    ⚠️ 止损幅度适中（中位数0.2-0.5%）');
    } else {
        console.log('    ❌ 止损幅度较大（中位数>0.5%），需要优化');
    }

    console.log(`\n  盈亏比: ${profitFactor.toFixed(2)}`);
    if (profitFactor > 3) {
        console.log('    ✅ 优秀的盈亏比，平均盈利是平均亏损的3倍以上');
    } else if (profitFactor > 2) {
        console.log('    ⚠️ 良好的盈亏比');
    } else {
        console.log('    ❌ 盈亏比偏低');
    }
}

// ============ 统计函数 ============
function mean(arr) {
    return arr.reduce((s, v) => s + v, 0) / arr.length;
}

function median(arr) {
    const sorted = [...arr].sort((a, b) => a - b);
    const mid = Math.floor(sorted.length / 2);
    return sorted.length % 2 !== 0 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}

// ============ 主程序 ============
function main() {
    const csvPath = path.resolve('F:/rust_projects/trade/src/kline_5m_202607232011.csv');
    const data = loadCSV(csvPath);

    console.log('日短策略 - 止损幅度分析');
    console.log(`数据: ${data.length}条K线, ${data[0].open_time} ~ ${data[data.length - 1].open_time}\n`);

    const trades = backtestWithDetails(data, CONFIG);
    deepAnalysis(trades);
}

main();
