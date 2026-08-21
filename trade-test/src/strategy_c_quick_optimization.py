#!/usr/bin/env python3
"""
方案C快速参数优化
"""

import pandas as pd
import numpy as np
import os
from datetime import datetime

def quick_optimization():
    """快速参数优化"""
    
    # 数据目录
    data_dir = "F:\\rust-projects\\rust-trade\\trade-test\\src\\sample_data"
    
    # 测试币种
    symbols = ['BTC', 'ETH', 'SOL']
    
    # 参数组合
    param_combinations = [
        # 基础参数
        {'ma_period': 192, 'boll_period': 100, 'boll_std': 2.0, 'compression_threshold': 0.3, 'min_compression_bars': 10, 'hard_stop_pct': 2.0},
        # 测试不同压缩阈值
        {'ma_period': 192, 'boll_period': 100, 'boll_std': 2.0, 'compression_threshold': 0.2, 'min_compression_bars': 10, 'hard_stop_pct': 2.0},
        {'ma_period': 192, 'boll_period': 100, 'boll_std': 2.0, 'compression_threshold': 0.4, 'min_compression_bars': 10, 'hard_stop_pct': 2.0},
        # 测试不同最小压缩bars
        {'ma_period': 192, 'boll_period': 100, 'boll_std': 2.0, 'compression_threshold': 0.3, 'min_compression_bars': 5, 'hard_stop_pct': 2.0},
        {'ma_period': 192, 'boll_period': 100, 'boll_std': 2.0, 'compression_threshold': 0.3, 'min_compression_bars': 15, 'hard_stop_pct': 2.0},
        # 测试不同硬止损
        {'ma_period': 192, 'boll_period': 100, 'boll_std': 2.0, 'compression_threshold': 0.3, 'min_compression_bars': 10, 'hard_stop_pct': 1.5},
        {'ma_period': 192, 'boll_period': 100, 'boll_std': 2.0, 'compression_threshold': 0.3, 'min_compression_bars': 10, 'hard_stop_pct': 2.5},
        # 测试不同BOLL标准差
        {'ma_period': 192, 'boll_period': 100, 'boll_std': 1.5, 'compression_threshold': 0.3, 'min_compression_bars': 10, 'hard_stop_pct': 2.0},
        {'ma_period': 192, 'boll_period': 100, 'boll_std': 2.5, 'compression_threshold': 0.3, 'min_compression_bars': 10, 'hard_stop_pct': 2.0},
    ]
    
    results = []
    
    for symbol in symbols:
        print(f"\n优化 {symbol}...")
        
        # 加载数据
        file_path = os.path.join(data_dir, f"kline_30m_{symbol}.csv")
        if not os.path.exists(file_path):
            print(f"  数据文件不存在: {file_path}")
            continue
        
        df = pd.read_csv(file_path)
        df['open_time'] = pd.to_datetime(df['open_time'])
        
        for i, params in enumerate(param_combinations):
            print(f"  参数组合 {i+1}/{len(param_combinations)}: {params}")
            
            # 计算指标
            df_temp = df.copy()
            
            # MA
            df_temp['ma192'] = df_temp['close'].rolling(window=params['ma_period']).mean()
            
            # BOLL
            df_temp['boll_mid'] = df_temp['close'].rolling(window=params['boll_period']).mean()
            df_temp['boll_std'] = df_temp['close'].rolling(window=params['boll_period']).std()
            df_temp['boll_upper'] = df_temp['boll_mid'] + (params['boll_std'] * df_temp['boll_std'])
            df_temp['boll_lower'] = df_temp['boll_mid'] - (params['boll_std'] * df_temp['boll_std'])
            
            # BOLL带宽
            df_temp['boll_width'] = (df_temp['boll_upper'] - df_temp['boll_lower']) / df_temp['boll_mid']
            
            # 带宽百分比
            df_temp['boll_width_pct'] = df_temp['boll_width'].rolling(window=200).apply(
                lambda x: (x.iloc[-1] - x.min()) / (x.max() - x.min()) if x.max() != x.min() else 0.5
            )
            
            # 压缩检测
            df_temp['is_compressed'] = df_temp['boll_width_pct'] < params['compression_threshold']
            
            # 连续压缩bars
            compression_groups = (~df_temp['is_compressed']).cumsum()
            df_temp['compression_bars'] = df_temp.groupby(compression_groups)['is_compressed'].cumsum()
            
            # 有效压缩
            df_temp['valid_compression'] = (df_temp['is_compressed']) & (df_temp['compression_bars'] >= params['min_compression_bars'])
            
            # MA与BOLL中轨关系
            df_temp['ma_above_mid'] = df_temp['ma192'] > df_temp['boll_mid']
            df_temp['ma_below_mid'] = df_temp['ma192'] < df_temp['boll_mid']
            
            # 穿越信号
            df_temp['cross_above_ma'] = (df_temp['close'] > df_temp['ma192']) & (df_temp['close'].shift(1) <= df_temp['ma192'].shift(1))
            df_temp['cross_below_ma'] = (df_temp['close'] < df_temp['ma192']) & (df_temp['close'].shift(1) >= df_temp['ma192'].shift(1))
            
            # 生成信号
            df_temp['signal'] = 0
            
            long_condition = (
                df_temp['ma_above_mid'] &
                df_temp['cross_above_ma'] &
                df_temp['valid_compression']
            )
            
            short_condition = (
                df_temp['ma_below_mid'] &
                df_temp['cross_below_ma'] &
                df_temp['valid_compression']
            )
            
            df_temp.loc[long_condition, 'signal'] = 1
            df_temp.loc[short_condition, 'signal'] = -1
            
            # 简化回测
            initial_capital = 10000.0
            capital = initial_capital
            position = 0
            entry_price = 0.0
            trades = []
            
            for j in range(1, len(df_temp)):
                current_bar = df_temp.iloc[j]
                
                if position != 0:
                    if position == 1:
                        current_profit_pct = (current_bar['close'] - entry_price) / entry_price * 100
                    else:
                        current_profit_pct = (entry_price - current_bar['close']) / entry_price * 100
                    
                    # 检查出场条件
                    exit_reason = None
                    
                    # 硬止损
                    if current_profit_pct <= -params['hard_stop_pct']:
                        exit_reason = 'hard_stop'
                    
                    # BOLL中轨止损
                    elif position == 1 and current_bar['close'] < current_bar['boll_mid']:
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
                win_rate = winning_trades / total_trades * 100
                total_pnl_pct = trades_df['pnl_pct'].sum()
                compound_return = (capital - initial_capital) / initial_capital * 100
                
                results.append({
                    'symbol': symbol,
                    'params': str(params),
                    'total_trades': total_trades,
                    'win_rate': win_rate,
                    'total_pnl_pct': total_pnl_pct,
                    'compound_return_pct': compound_return,
                    'final_capital': capital
                })
                
                print(f"    交易数: {total_trades}, 胜率: {win_rate:.1f}%, 复利: {compound_return:.2f}%")
            else:
                print(f"    无交易")
    
    # 保存结果
    if results:
        results_df = pd.DataFrame(results)
        
        output_dir = "F:\\rust-projects\\rust-trade\\trade-test\\src\\feature_report"
        os.makedirs(output_dir, exist_ok=True)
        
        output_file = os.path.join(output_dir, "strategy_c_quick_optimization.csv")
        results_df.to_csv(output_file, index=False)
        
        print(f"\n优化结果已保存到: {output_file}")
        
        # 显示最佳结果
        print("\n各币种最佳结果:")
        for symbol in symbols:
            symbol_results = results_df[results_df['symbol'] == symbol]
            if not symbol_results.empty:
                best = symbol_results.sort_values('compound_return_pct', ascending=False).iloc[0]
                print(f"{symbol}: 复利={best['compound_return_pct']:.2f}%, 胜率={best['win_rate']:.1f}%, 交易数={best['total_trades']}")
                print(f"  参数: {best['params']}")

if __name__ == "__main__":
    print("=" * 80)
    print("方案C快速参数优化")
    print("=" * 80)
    
    quick_optimization()