#!/usr/bin/env python3
"""
方案C策略重新分析
寻找更稳健的改进方向
"""

import pandas as pd
import numpy as np
import os
from datetime import datetime

def reanalyze_strategy():
    """重新分析策略问题"""
    
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
        
        # 分析策略问题
        print(f"  数据时间范围: {df['open_time'].min()} 到 {df['open_time'].max()}")
        
        # 分析信号分布
        long_signals = (df['ma_above_mid'] & df['cross_above_ma'] & df['valid_compression']).sum()
        short_signals = (df['ma_below_mid'] & df['cross_below_ma'] & df['valid_compression']).sum()
        total_signals = long_signals + short_signals
        
        print(f"  总信号数: {total_signals}")
        print(f"  做多信号: {long_signals}")
        print(f"  做空信号: {short_signals}")
        
        # 分析压缩状态
        compressed_bars = df['is_compressed'].sum()
        valid_compressed_bars = df['valid_compression'].sum()
        
        print(f"  压缩bar数: {compressed_bars} ({compressed_bars/len(df)*100:.1f}%)")
        print(f"  有效压缩bar数: {valid_compressed_bars} ({valid_compressed_bars/len(df)*100:.1f}%)")
        
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
        
        # 分析信号质量
        # 运行回测
        initial_capital = 10000.0
        capital = initial_capital
        position = 0
        entry_price = 0.0
        trades = []
        
        for i in range(1, len(df)):
            current_bar = df.iloc[i]
            
            if position != 0:
                if position == 1:
                    current_profit_pct = (current_bar['close'] - entry_price) / entry_price * 100
                else:
                    current_profit_pct = (entry_price - current_bar['close']) / entry_price * 100
                
                # 检查出场条件
                exit_reason = None
                
                # 硬止损
                if current_profit_pct <= -base_params['hard_stop_pct']:
                    exit_reason = 'hard_stop'
                
                # BOLL中轨止损
                elif base_params['boll_stop_enabled']:
                    if position == 1 and current_bar['close'] < current_bar['boll_mid']:
                        exit_reason = 'boll_mid_stop'
                    elif position == -1 and current_bar['close'] > current_bar['boll_mid']:
                        exit_reason = 'boll_mid_stop'
                
                if exit_reason:
                    if position == 1:
                        pnl_pct = (current_bar['close'] - entry_price) / entry_price * 100
                    else:
                        pnl_pct = (entry_price - current_bar['close']) / entry_price * 100
                    
                    pnl_amount = capital * (pnl_pct / 100)
                    capital += pnl_amount
                    
                    trades.append({
                        'pnl_pct': pnl_pct,
                        'pnl_amount': pnl_amount,
                        'exit_reason': exit_reason
                    })
                    
                    position = 0
                    entry_price = 0.0
            
            if position == 0 and current_bar['signal'] != 0:
                position = current_bar['signal']
                entry_price = current_bar['close']
        
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
    print("方案C策略重新分析")
    print("=" * 80)
    
    results = reanalyze_strategy()
    
    # 生成报告
    output_dir = "F:\\rust-projects\\rust-trade\\trade-test\\src\\feature_report"
    os.makedirs(output_dir, exist_ok=True)
    
    report_file = os.path.join(output_dir, "strategy_c_reanalysis_report.md")
    
    with open(report_file, 'w', encoding='utf-8') as f:
        f.write("# 方案C策略重新分析报告\n\n")
        f.write(f"> 生成时间：{datetime.now().strftime('%Y-%m-%d %H:%M')}\n")
        f.write("> 分析目标：重新审视策略问题，寻找改进方向\n\n")
        
        f.write("## 一、问题分析\n\n")
        f.write("### 1.1 时间切分验证暴露的问题\n\n")
        f.write("- 全样本结果可能包含过拟合成分\n")
        f.write("- 过滤规则可能针对特定时间段优化\n")
        f.write("- 策略本身可能不够稳健\n\n")
        
        f.write("## 二、各币种分析\n\n")
        
        for symbol, result in results.items():
            f.write(f"### {symbol}\n\n")
            
            if 'error' in result:
                f.write(f"错误: {result['error']}\n\n")
                continue
            
            f.write(f"- 总交易数: {result['total_trades']}\n")
            f.write(f"- 胜率: {result['win_rate']:.1f}%\n")
            f.write(f"- 复利收益: {result['compound_return']:.2f}%\n")
            f.write(f"- 最大回撤: {result['max_drawdown']:.2f}%\n")
            f.write(f"- 盈亏比: {result['profit_factor']:.2f}\n")
            f.write(f"- 离场原因: {result['exit_reasons']}\n\n")
        
        f.write("## 三、改进方向\n\n")
        f.write("### 3.1 策略逻辑问题\n\n")
        f.write("1. **入场条件可能过于宽松**：压缩状态判断可能不够严格\n")
        f.write("2. **止损可能过于敏感**：BOLL中轨止损可能太紧\n")
        f.write("3. **缺乏趋势确认**：仅依靠MA192和BOLL中轨关系可能不够\n\n")
        
        f.write("### 3.2 可能的改进方向\n\n")
        f.write("1. **增加趋势确认**：结合更高时间框架确认趋势\n")
        f.write("2. **优化止损策略**：使用更宽松的止损或移动止盈\n")
        f.write("3. **改进入场条件**：增加成交量或动量确认\n")
        f.write("4. **市场状态过滤**：根据市场状态调整策略参数\n\n")
        
        f.write("## 四、下一步建议\n\n")
        f.write("1. **重新设计入场条件**：增加更多确认条件\n")
        f.write("2. **优化止损策略**：测试不同的止损方式\n")
        f.write("3. **市场状态分析**：根据市场状态调整策略\n")
        f.write("4. **简化策略逻辑**：避免过度复杂化\n\n")
        
        f.write("## 五、相关文件\n\n")
        f.write("- 分析脚本: `src/strategy_c_reanalysis.py`\n")
        f.write("- 本报告: `src/feature_report/strategy_c_reanalysis_report.md`\n")
    
    print(f"\n报告已生成: {report_file}")

if __name__ == "__main__":
    main()