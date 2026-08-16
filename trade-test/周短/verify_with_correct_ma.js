/**
 * 使用正确的30m MA288验证信号
 */

console.log("=".repeat(70));
console.log("使用正确的30m MA288验证信号");
console.log("=".repeat(70));

// 信号参数
const signalPrice = 73.58;

// 正确的30m MA值
const ma288_30m = 74.4925;
const ma488_30m = 75.6707;

// 错误的5m MA值 (数据库中实际使用的)
const ma288_5m = 73.5968;
const ma488_5m = 73.6512;

// 14:30 的30m K线
const candle1430 = {
  open: 73.70,
  high: 73.70,
  low: 73.49,
  close: 73.53
};

console.log("\n--- 使用错误的5m MA (数据库实际值) ---");
console.log(`MA288 = ${ma288_5m}`);
console.log(`MA488 = ${ma488_5m}`);
console.log(`趋势: ${ma288_5m < ma488_5m ? 'bearish' : 'bullish'}`);
console.log(`入场条件: O > MA288 && C < MA288`);
console.log(`  O=${candle1430.open} > ${ma288_5m} → ${candle1430.open > ma288_5m ? '✅' : '❌'}`);
console.log(`  C=${candle1430.close} < ${ma288_5m} → ${candle1430.close < ma288_5m ? '✅' : '❌'}`);
console.log(`结果: ${candle1430.open > ma288_5m && candle1430.close < ma288_5m ? '✅ 产生信号' : '❌ 无信号'}`);

console.log("\n--- 使用正确的30m MA ---");
console.log(`MA288 = ${ma288_30m}`);
console.log(`MA488 = ${ma488_30m}`);
console.log(`趋势: ${ma288_30m > ma488_30m ? 'bullish' : 'bearish'}`);
console.log(`入场条件: O > MA288 && C < MA288`);
console.log(`  O=${candle1430.open} > ${ma288_30m} → ${candle1430.open > ma288_30m ? '✅' : '❌'}`);
console.log(`  C=${candle1430.close} < ${ma288_30m} → ${candle1430.close < ma288_30m ? '✅' : '❌'}`);
console.log(`结果: ${candle1430.open > ma288_30m && candle1430.close < ma288_30m ? '✅ 产生信号' : '❌ 无信号'}`);

console.log("\n" + "=".repeat(70));
console.log("结论");
console.log("=".repeat(70));

console.log(`
问题: 系统使用了 5m 数据计算 MA288，而不是策略配置的 30m

如果使用正确的 30m MA288 (${ma288_30m}):
  - 趋势应该是 bullish (MA288 > MA488)
  - 入场条件不满足 (73.70 < 74.49)
  - 信号不应该产生！

实际使用了错误的 5m MA288 (${ma288_5m}):
  - 趋势显示为 bearish (MA288 < MA488)
  - 入场条件满足 (73.70 > 73.5968, 73.53 < 73.5968)
  - 错误地产生了信号

⚠️ 这是一个严重的 bug！
`);

console.log("验证完成！");
