#!/usr/bin/env python3
"""
方案C MA192止损分析
分析使用MA192作为止损位的可行性
"""

import pandas as pd
import numpy as np
import os
from datetime import datetime

def analyze_ma192_stop():
    """分析MA192止损"""
    
    # 数据目录
    data_dir = "F:\\rust-projects\\data_2026-08-13"
    
    # 测试币种
    symbols = ['BTC', 'ETH', 'SOL']
    
    # 基础参数
    base_params = {
        'ma_period': 192,
        'boll_period': 100,
        'boll_std': 2.0,
        'compression_threshold': 0.3,
        'min_compression_bars': 10,
        'hard_stop_pct': 2.0,
        'boll_stop_enabled': False,  # 禁用BOLL中轨止损
        'ma192_stop_enabled': True   # 启用MA192止损
    }
    
    results = {}
    
    for symbol in symbols:
        print(f"\n分析 {symbol}...")
        
        # 加载数据
        file_path = os.path.join(data_dir, f"kline_30m_{symbol}.csv")
        if not os.path.exists(file_path):
            print(f"  数据文件不存在: {file_path}")
            continue
        
        df = pd.read_csv(file_path)
        
        # 处理时间列
        if 'open_time' in df.columns:
            df['open_time'] = pd.to_datetime(df['open_time'].str.replace(r'\s+\+\d+$', '', regex=True))
        
        # 确保数值列正确
        numeric_cols = ['open', 'high', 'low', 'close', 'volume']
        for col in numeric_cols:
            if col in df.columns:
                df[col] = pd.to_numeric(df[col], errors='coerce')
        
        # 按时间排序
        df = df.sort_values('open_time').reset_index(drop=True)
        
        # 计算指标
        df['ma192'] = df['close'].rolling(window=base_params['ma_period']).mean()
        df['boll_mid'] = df['close'].rolling(window=base_params['boll_period']).mean()
        df['boll_std_val'] = df['close'].rolling(window=base_params['boll_period']).std()
        df['boll_upper'] = df['boll_mid'] + (base_params['boll_std'] * df['boll_std_val'])
        df['boll_lower'] = df['boll_mid'] - (base_params['boll_std'] * df['boll_std_val'])
        df['boll_width'] = (df['boll_upper'] - df['boll_lower']) / df['boll_mid']
        df['boll_width_pct'] = df['boll_width'].rolling(window=200).apply(
            lambda x: (x.iloc[-1] - x.min()) / (x.max() - x.min()) if x.max() != x.min() else 0.5
        )
        df['is_compressed'] = df['boll_width_pct'] < base_params['compression_threshold']
        compression_groups = (~df['is_compressed']).cumsum()
        df['compression_bars'] = df.groupby(compression_groups)['is_compressed'].cumsum()
        df['valid_compression'] = (df['is_compressed']) & (df['compression_bars'] >= base_params['min_compression_bars'])
        df['ma_above_mid'] = df['ma192'] > df['boll_mid']
        df['ma_below_mid'] = df['ma192'] < df['boll_mid']
        df['cross_above_ma'] = (df['close'] > df['ma192']) & (df['close'].shift(1) <= df['ma192'].shift(1))
        df['cross_below_ma'] = (df['close'] < df['ma192']) & (df['close'].shift(1) >= df['ma192'].shift(1))
        
        # 生成信号
        long_condition = (
            df['ma_above_mid'] &
            df['cross_above_ma'] &
            df['valid_compression']
        )
        
        short_condition = (
            df['ma_below_mid'] &
            df['cross_below_ma'] &
            df['valid_compression']
        )
        
        df['signal'] = 0
        df.loc[long_condition, 'signal'] = 1
        df.loc[short_condition, 'signal'] = -1
        
        # 运行回测
        initial_capital = 10000.0
        capital = initial_capital
        position = 0
        entry_price = 0.0
        entry_ma192 = 0.0  # 入场时的MA192值
        trades = []
        
        # 遍历每个bar
        for i in range(1, len(df)):
            current_bar = df.iloc[i]
            
            # 如果有持仓，检查出场条件
            if position != 0:
                if position == 1:
                    current_profit_pct = (current_bar['close'] - entry_price) / entry_price * 100
                else:
                    current_profit_pct = (entry_price - current_bar['close']) / entry_price * 100
                
                # 检查出场条件
                exit_reason = None
                exit_price = current_bar['close']
                
                # 1. 硬止损
                if current_profit_pct <= -base_params['hard_stop_pct']:
                    exit_reason = 'hard_stop'
                
                # 2. MA192止损
                elif base_params['ma192_stop_enabled']:
                    if position == 1 and current_bar['close'] < entry_ma192:
                        exit_reason = 'ma192_stop'
                    elif position == -1 and current_bar['close'] > entry_ma192:
                        exit_reason = 'ma192_stop'
                
                # 执行出场
                if exit_reason:
                    if position == 1:
                        pnl_pct = (exit_price - entry_price) / entry_price * 100
                    else:
                        pnl_pct = (entry_price - exit_price) / entry_price * 100
                    
                    pnl_amount = capital * (pnl_pct / 100)
                    capital += pnl_amount
                    
                    trade = {
                        'pnl_pct': pnl_pct,
                        'pnl_amount': pnl_amount,
                        'exit_reason': exit_reason
                    }
                    trades.append(trade)
                    
                    position = 0
                    entry_price = 0.0
                    entry_ma192 = 0.0
            
            # 如果没有持仓，检查入场信号
            if position == 0 and current_bar['signal'] != 0:
                signal = current_bar['signal']
                entry_price = current_bar['close']
                entry_ma192 = current_bar['ma192']  # 记录入场时的MA192值
                position = signal
        
        # 计算统计
        if trades:
            trades_df = pd.DataFrame(trades)
            total_trades = len(trades_df)
            winning_trades = len(trades_df[trades_df['pnl_pct'] > 0])
            losing_trades = len(trades_df[trades_df['pnl_pct'] <= 0])
            
            win_rate = winning_trades / total_trades * 100 if total_trades > 0 else 0
            total_pnl_pct = trades_df['pnl_pct'].sum()
            compound_return = (capital - initial_capital) / initial_capital * 100
            
            # 计算最大回撤
            capital_curve = [initial_capital]
            current_capital = initial_capital
            for trade in trades:
                current_capital += trade['pnl_amount']
                capital_curve.append(current_capital)
            
            capital_series = pd.Series(capital_curve)
            rolling_max = capital_series.expanding().max()
            drawdowns = (capital_series - rolling_max) / rolling_max * 100
            max_drawdown_pct = drawdowns.min()
            
            # 计算盈亏比
            avg_win = trades_df[trades_df['pnl_pct'] > 0]['pnl_pct'].mean() if winning_trades > 0 else 0
            avg_loss = abs(trades_df[trades_df['pnl_pct'] <= 0]['pnl_pct'].mean()) if losing_trades > 0 else 0
            profit_factor = avg_win / avg_loss if avg_loss > 0 else float('inf')
            
            # 分析离场原因
            exit_reasons = trades_df['exit_reason'].value_counts().to_dict()
            
            print(f"  回测结果:")
            print(f"    总交易数: {total_trades}")
            print(f"    胜率: {win_rate:.1f}%")
            print(f"    复利收益: {compound_return:.2f}%")
            print(f"    最大回撤: {max_drawdown_pct:.2f}%")
            print(f"    盈亏比: {profit_factor:.2f}")
            print(f"    离场原因: {exit_reasons}")
            
            results[symbol] = {
                'total_trades': total_trades,
                'win_rate': win_rate,
                'compound_return': compound_return,
                'max_drawdown': max_drawdown_pct,
                'profit_factor': profit_factor,
                'exit_reasons': exit_reasons
            }
        else:
            print(f"  无交易")
            results[symbol] = {'error': '无交易'}
    
    return results

def main():
    """主函数"""
    print("=" * 80)
    print("方案C MA192止损分析")
    print("=" * 80)
    
    results = analyze_ma192_stop()
    
    # 生成报告
    output_dir = "F:\\rust-projects\\rust-trade\\trade-test\\src\\feature_report"
    os.makedirs(output_dir, exist_ok=True)
    
    report_file = os.path.join(output_dir, "strategy_c_ma192_stop_report.md")
    
    with open(report_file, 'w', encoding='utf-8') as f:
        f.write("# 方案C MA192止损分析报告\n\n")
        f.write(f"> 生成时间：{datetime.now().strftime('%Y-%m-%d %H:%M')}\n")
        f.write("> 分析目标：分析使用MA192作为止损位的可行性\n\n")
        
        f.write("## 一、策略逻辑\n\n")
        f.write("### 1.1 入场条件\n\n")
        f.write("- MA192 > BOLL中轨（上升趋势）\n")
        f.write("- 收盘价向上穿越MA192\n")
        f.write("- 处于有效压缩状态\n\n")
        
        f.write("### 1.2 出场条件\n\n")
        f.write("- **MA192止损**：价格跌破入场时的MA192值\n")
        f.write("- **硬止损**：固定百分比止损（2%）\n")
        f.write("- **注意**：禁用BOLL中轨止损\n\n")
        
        f.write("## 二、各币种回测结果\n\n")
        f.write("| 币种 | 交易数 | 胜率 | 复利收益 | 最大回撤 | 盈亏比 |\n")
        f.write("|------|--------|------|----------|----------|--------|\n")
        
        for symbol, result in results.items():
            if 'error' in result:
                f.write(f"| {symbol} | 错误 | - | - | - | - |\n")
            else:
                f.write(f"| {symbol} | {result['total_trades']} | {result['win_rate']:.1f}% | "
                       f"{result['compound_return']:.2f}% | {result['max_drawdown']:.2f}% | "
                       f"{result['profit_factor']:.2f} |\n")
        
        f.write("\n")
        
        f.write("## 三、离场原因分析\n\n")
        
        for symbol, result in results.items():
            if 'error' in result:
                continue
            
            f.write(f"### {symbol}\n\n")
            f.write("| 离场原因 | 数量 | 占比 |\n")
            f.write("|----------|------|------|\n")
            
            total = sum(result['exit_reasons'].values())
            for reason, count in result['exit_reasons'].items():
                f.write(f"| {reason} | {count} | {count/total*100:.1f}% |\n")
            
            f.write("\n")
        
        f.write("## 四、MA192止损 vs BOLL中轨止损对比\n\n")
        f.write("| 维度 | MA192止损 | BOLL中轨止损 |\n")
        f.write("|------|-----------|--------------|\n")
        f.write("| 止损位 | 入场时的MA192值 | 实时BOLL中轨值 |\n")
        f.write("| 止损逻辑 | 价格跌破入场时均线 | 价格跌破实时中轨 |\n")
        f.write("| 灵活性 | 固定止损位 | 动态止损位 |\n")
        f.write("| 适用场景 | 趋势跟随 | 震荡市场 |\n\n")
        
        f.write("## 五、MA192止损的优缺点\n\n")
        f.write("### 5.1 优点\n\n")
        f.write("1. **止损位固定**：入场时确定，不会随市场变化\n")
        f.write("2. **逻辑清晰**：价格跌破均线止损，符合趋势跟随逻辑\n")
        f.write("3. **减少噪音**：不会因为市场短期波动而频繁止损\n")
        f.write("4. **持仓更耐心**：给予价格更多波动空间\n\n")
        
        f.write("### 5.2 缺点\n\n")
        f.write("1. **止损幅度可能较大**：如果入场时MA192离入场价较远\n")
        f.write("2. **可能错过反转**：如果市场趋势反转但未跌破MA192\n")
        f.write("3. **需要更多资金**：较大的止损幅度需要更多资金支持\n\n")
        
        f.write("## 六、下一步优化建议\n\n")
        f.write("1. **测试不同MA周期**：尝试MA96、MA144等不同周期\n")
        f.write("2. **组合止损**：MA192止损 + 硬止损组合\n")
        f.write("3. **移动止损**：盈利后将止损位移动到入场价\n")
        f.write("4. **时间止损**：持仓超过一定时间后止损\n\n")
        
        f.write("## 七、相关文件\n\n")
        f.write("- 分析脚本: `src/strategy_c_ma192_stop_analysis.py`\n")
        f.write("- 本报告: `src/feature_report/strategy_c_ma192_stop_report.md`\n")
    
    print(f"\n报告已生成: {report_file}")

if __name__ == "__main__":
    main()