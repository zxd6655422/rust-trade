#!/usr/bin/env python3
"""
方案C止损分析
分析止损时的入场方向正确性和后续走势
"""

import pandas as pd
import numpy as np
import os
from datetime import datetime

def analyze_stop_quality():
    """分析止损质量"""
    
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
        'boll_stop_enabled': True
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
        
        # 运行回测并记录详细信息
        initial_capital = 10000.0
        capital = initial_capital
        position = 0
        entry_price = 0.0
        entry_time = None
        entry_idx = 0
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
                exit_time = current_bar['open_time']
                exit_idx = i
                
                # 1. 硬止损
                if current_profit_pct <= -base_params['hard_stop_pct']:
                    exit_reason = 'hard_stop'
                
                # 2. BOLL中轨止损
                elif base_params['boll_stop_enabled']:
                    if position == 1 and current_bar['close'] < current_bar['boll_mid']:
                        exit_reason = 'boll_mid_stop'
                    elif position == -1 and current_bar['close'] > current_bar['boll_mid']:
                        exit_reason = 'boll_mid_stop'
                
                # 执行出场
                if exit_reason:
                    if position == 1:
                        pnl_pct = (exit_price - entry_price) / entry_price * 100
                    else:
                        pnl_pct = (entry_price - exit_price) / entry_price * 100
                    
                    pnl_amount = capital * (pnl_pct / 100)
                    capital += pnl_amount
                    
                    # 计算后续走势（止损后100根bar）
                    future_bars = min(100, len(df) - exit_idx)
                    if future_bars > 0:
                        future_prices = df.iloc[exit_idx:exit_idx+future_bars]['close'].values
                        
                        # 计算后续最大盈利和最大亏损
                        if position == 1:  # 多头止损
                            future_max_profit = max((future_prices - exit_price) / exit_price * 100)
                            future_max_loss = min((future_prices - exit_price) / exit_price * 100)
                            # 计算后续是否继续下跌
                            future_final = (future_prices[-1] - exit_price) / exit_price * 100
                        else:  # 空头止损
                            future_max_profit = max((exit_price - future_prices) / exit_price * 100)
                            future_max_loss = min((exit_price - future_prices) / exit_price * 100)
                            # 计算后续是否继续上涨
                            future_final = (exit_price - future_prices[-1]) / exit_price * 100
                    else:
                        future_max_profit = 0
                        future_max_loss = 0
                        future_final = 0
                    
                    trade = {
                        'entry_time': entry_time,
                        'exit_time': exit_time,
                        'direction': 'LONG' if position == 1 else 'SHORT',
                        'entry_price': entry_price,
                        'exit_price': exit_price,
                        'pnl_pct': pnl_pct,
                        'pnl_amount': pnl_amount,
                        'exit_reason': exit_reason,
                        'hold_bars': exit_idx - entry_idx,
                        'future_max_profit': future_max_profit,
                        'future_max_loss': future_max_loss,
                        'future_final': future_final,
                        'future_bars': future_bars,
                        'entry_idx': entry_idx,
                        'exit_idx': exit_idx
                    }
                    trades.append(trade)
                    
                    position = 0
                    entry_price = 0.0
                    entry_time = None
                    entry_idx = 0
            
            # 如果没有持仓，检查入场信号
            if position == 0 and current_bar['signal'] != 0:
                signal = current_bar['signal']
                entry_price = current_bar['close']
                entry_time = current_bar['open_time']
                entry_idx = i
                position = signal
        
        # 分析止损质量
        if trades:
            trades_df = pd.DataFrame(trades)
            
            # 分析止损后的走势
            boll_stop_trades = trades_df[trades_df['exit_reason'] == 'boll_mid_stop']
            hard_stop_trades = trades_df[trades_df['exit_reason'] == 'hard_stop']
            
            print(f"  总交易数: {len(trades_df)}")
            print(f"  BOLL中轨止损: {len(boll_stop_trades)} ({len(boll_stop_trades)/len(trades_df)*100:.1f}%)")
            print(f"  硬止损: {len(hard_stop_trades)} ({len(hard_stop_trades)/len(trades_df)*100:.1f}%)")
            
            # 分析BOLL中轨止损后的走势
            if len(boll_stop_trades) > 0:
                print(f"\n  BOLL中轨止损后走势分析:")
                print(f"    平均后续最大盈利: {boll_stop_trades['future_max_profit'].mean():.2f}%")
                print(f"    平均后续最大亏损: {boll_stop_trades['future_max_loss'].mean():.2f}%")
                print(f"    平均后续最终收益: {boll_stop_trades['future_final'].mean():.2f}%")
                
                # 分析止损是否正确（后续是否继续反向运动）
                correct_stops = 0
                for _, trade in boll_stop_trades.iterrows():
                    if trade['direction'] == 'LONG':  # 多头止损
                        if trade['future_final'] < 0:  # 后续继续下跌，止损正确
                            correct_stops += 1
                    else:  # 空头止损
                        if trade['future_final'] > 0:  # 后续继续上涨，止损正确
                            correct_stops += 1
                
                correct_stop_rate = correct_stops / len(boll_stop_trades) * 100
                print(f"    止损正确率: {correct_stop_rate:.1f}%")
                
                # 分析止损后反弹的情况
                rebound_trades = boll_stop_trades[
                    ((boll_stop_trades['direction'] == 'LONG') & (boll_stop_trades['future_max_profit'] > 2)) |
                    ((boll_stop_trades['direction'] == 'SHORT') & (boll_stop_trades['future_max_profit'] > 2))
                ]
                
                print(f"    止损后反弹>2%的交易: {len(rebound_trades)} ({len(rebound_trades)/len(boll_stop_trades)*100:.1f}%)")
                
                # 分析止损时机
                early_stops = boll_stop_trades[boll_stop_trades['hold_bars'] < 10]
                print(f"    持仓<10bar的止损: {len(early_stops)} ({len(early_stops)/len(boll_stop_trades)*100:.1f}%)")
            
            # 保存详细结果
            results[symbol] = {
                'total_trades': len(trades_df),
                'boll_stop_trades': len(boll_stop_trades),
                'hard_stop_trades': len(hard_stop_trades),
                'boll_stop_analysis': {
                    'avg_future_max_profit': boll_stop_trades['future_max_profit'].mean() if len(boll_stop_trades) > 0 else 0,
                    'avg_future_max_loss': boll_stop_trades['future_max_loss'].mean() if len(boll_stop_trades) > 0 else 0,
                    'avg_future_final': boll_stop_trades['future_final'].mean() if len(boll_stop_trades) > 0 else 0,
                    'correct_stop_rate': correct_stop_rate if len(boll_stop_trades) > 0 else 0,
                    'rebound_rate': len(rebound_trades) / len(boll_stop_trades) * 100 if len(boll_stop_trades) > 0 else 0,
                    'early_stop_rate': len(early_stops) / len(boll_stop_trades) * 100 if len(boll_stop_trades) > 0 else 0
                }
            }
        else:
            print(f"  无交易")
            results[symbol] = {'error': '无交易'}
    
    return results

def main():
    """主函数"""
    print("=" * 80)
    print("方案C止损分析")
    print("=" * 80)
    
    results = analyze_stop_quality()
    
    # 生成报告
    output_dir = "F:\\rust-projects\\rust-trade\\trade-test\\src\\feature_report"
    os.makedirs(output_dir, exist_ok=True)
    
    report_file = os.path.join(output_dir, "strategy_c_stop_analysis_report.md")
    
    with open(report_file, 'w', encoding='utf-8') as f:
        f.write("# 方案C止损分析报告\n\n")
        f.write(f"> 生成时间：{datetime.now().strftime('%Y-%m-%d %H:%M')}\n")
        f.write("> 分析目标：分析止损时的入场方向正确性和后续走势\n\n")
        
        f.write("## 一、止损统计\n\n")
        f.write("| 币种 | 总交易数 | BOLL中轨止损 | 硬止损 | BOLL止损占比 |\n")
        f.write("|------|----------|--------------|--------|--------------|\n")
        
        for symbol, result in results.items():
            if 'error' in result:
                f.write(f"| {symbol} | 错误 | - | - | - |\n")
            else:
                f.write(f"| {symbol} | {result['total_trades']} | {result['boll_stop_trades']} | "
                       f"{result['hard_stop_trades']} | {result['boll_stop_trades']/result['total_trades']*100:.1f}% |\n")
        
        f.write("\n")
        
        f.write("## 二、BOLL中轨止损后走势分析\n\n")
        f.write("| 币种 | 平均后续最大盈利 | 平均后续最大亏损 | 平均后续最终收益 | 止损正确率 | 反弹>2%比例 | 早期止损比例 |\n")
        f.write("|------|------------------|------------------|------------------|------------|-------------|--------------|\n")
        
        for symbol, result in results.items():
            if 'error' in result:
                f.write(f"| {symbol} | - | - | - | - | - | - |\n")
            else:
                analysis = result['boll_stop_analysis']
                f.write(f"| {symbol} | {analysis['avg_future_max_profit']:.2f}% | {analysis['avg_future_max_loss']:.2f}% | "
                       f"{analysis['avg_future_final']:.2f}% | {analysis['correct_stop_rate']:.1f}% | "
                       f"{analysis['rebound_rate']:.1f}% | {analysis['early_stop_rate']:.1f}% |\n")
        
        f.write("\n")
        
        f.write("## 三、关键发现\n\n")
        
        # 分析结果
        for symbol, result in results.items():
            if 'error' in result:
                continue
            
            analysis = result['boll_stop_analysis']
            f.write(f"### {symbol}\n\n")
            f.write(f"- **止损正确率**: {analysis['correct_stop_rate']:.1f}%（后续继续反向运动的比例）\n")
            f.write(f"- **反弹比例**: {analysis['rebound_rate']:.1f}%（止损后反弹>2%的比例）\n")
            f.write(f"- **早期止损比例**: {analysis['early_stop_rate']:.1f}%（持仓<10bar的止损）\n")
            f.write(f"- **平均后续走势**: 最终收益 {analysis['avg_future_final']:.2f}%\n\n")
        
        f.write("## 四、问题分析\n\n")
        f.write("### 4.1 止损时机问题\n\n")
        f.write("1. **止损正确率低**: 如果止损正确率低，说明止损过于敏感\n")
        f.write("2. **反弹比例高**: 如果止损后反弹比例高，说明止损时机过早\n")
        f.write("3. **早期止损多**: 如果早期止损比例高，说明入场后很快被止损\n\n")
        
        f.write("### 4.2 可能的改进方向\n\n")
        f.write("1. **使用更宽松的止损**: 如BOLL下轨止损而非中轨\n")
        f.write("2. **添加时间缓冲**: 入场后等待一定时间再检查止损\n")
        f.write("3. **使用移动止损**: 根据盈利情况调整止损位\n")
        f.write("4. **增加入场确认**: 避免在震荡市场中频繁入场\n\n")
        
        f.write("## 五、下一步优化建议\n\n")
        f.write("1. **测试BOLL下轨止损**: 使用BOLL下轨作为止损位\n")
        f.write("2. **添加时间止损**: 持仓超过一定时间后止损\n")
        f.write("3. **使用移动止盈**: 让盈利单跑更远\n")
        f.write("4. **优化入场条件**: 增加更多确认条件\n\n")
        
        f.write("## 六、相关文件\n\n")
        f.write("- 分析脚本: `src/strategy_c_stop_analysis.py`\n")
        f.write("- 本报告: `src/feature_report/strategy_c_stop_analysis_report.md`\n")
    
    print(f"\n报告已生成: {report_file}")

if __name__ == "__main__":
    main()