#!/usr/bin/env python3
"""
方案C简化测试版本
"""

import pandas as pd
import numpy as np
import os
from datetime import datetime

def test_strategy_c():
    """测试策略C的基本功能"""
    
    # 加载示例数据
    data_dir = "F:\\rust-projects\\rust-trade\\trade-test\\src\\sample_data"
    file_path = os.path.join(data_dir, "kline_30m_BTC.csv")
    
    if not os.path.exists(file_path):
        print(f"数据文件不存在: {file_path}")
        return
    
    df = pd.read_csv(file_path)
    print(f"加载数据: {len(df)} 根K线")
    print(f"数据列: {list(df.columns)}")
    
    # 计算MA192
    df['ma192'] = df['close'].rolling(window=192).mean()
    
    # 计算BOLL(100, 2.0)
    df['boll_mid'] = df['close'].rolling(window=100).mean()
    df['boll_std'] = df['close'].rolling(window=100).std()
    df['boll_upper'] = df['boll_mid'] + (2.0 * df['boll_std'])
    df['boll_lower'] = df['boll_mid'] - (2.0 * df['boll_std'])
    
    # 计算BOLL带宽
    df['boll_width'] = (df['boll_upper'] - df['boll_lower']) / df['boll_mid']
    
    # 计算带宽百分比
    df['boll_width_pct'] = df['boll_width'].rolling(window=200).apply(
        lambda x: (x.iloc[-1] - x.min()) / (x.max() - x.min()) if x.max() != x.min() else 0.5
    )
    
    # 压缩检测
    compression_threshold = 0.3
    df['is_compressed'] = df['boll_width_pct'] < compression_threshold
    
    # 计算连续压缩的bar数
    compression_groups = (~df['is_compressed']).cumsum()
    df['compression_bars'] = df.groupby(compression_groups)['is_compressed'].cumsum()
    
    # 有效压缩（持续至少10根bar）
    min_compression_bars = 10
    df['valid_compression'] = (df['is_compressed']) & (df['compression_bars'] >= min_compression_bars)
    
    # MA192与BOLL中轨关系
    df['ma_above_mid'] = df['ma192'] > df['boll_mid']
    df['ma_below_mid'] = df['ma192'] < df['boll_mid']
    
    # 穿越信号
    df['cross_above_ma'] = (df['close'] > df['ma192']) & (df['close'].shift(1) <= df['ma192'].shift(1))
    df['cross_below_ma'] = (df['close'] < df['ma192']) & (df['close'].shift(1) >= df['ma192'].shift(1))
    
    # 生成信号
    df['signal'] = 0
    
    # 做多信号
    long_condition = (
        df['ma_above_mid'] &  # MA192在中轨之上
        df['cross_above_ma'] &  # 收盘价向上穿越MA192
        df['valid_compression']  # 处于有效压缩状态
    )
    
    # 做空信号
    short_condition = (
        df['ma_below_mid'] &  # MA192在中轨之下
        df['cross_below_ma'] &  # 收盘价向下穿越MA192
        df['valid_compression']  # 处于有效压缩状态
    )
    
    df.loc[long_condition, 'signal'] = 1
    df.loc[short_condition, 'signal'] = -1
    
    # 统计信号
    long_signals = (df['signal'] == 1).sum()
    short_signals = (df['signal'] == -1).sum()
    
    print(f"做多信号: {long_signals}")
    print(f"做空信号: {short_signals}")
    print(f"总信号: {long_signals + short_signals}")
    
    # 显示一些统计信息
    print(f"\n数据统计:")
    print(f"时间范围: {df['open_time'].iloc[0]} 到 {df['open_time'].iloc[-1]}")
    print(f"价格范围: {df['close'].min():.2f} 到 {df['close'].max():.2f}")
    
    # 显示压缩统计
    compressed_bars = df['is_compressed'].sum()
    print(f"压缩bar数: {compressed_bars} ({compressed_bars/len(df)*100:.1f}%)")
    
    valid_compressed_bars = df['valid_compression'].sum()
    print(f"有效压缩bar数: {valid_compressed_bars} ({valid_compressed_bars/len(df)*100:.1f}%)")
    
    # 保存结果
    output_dir = "F:\\rust-projects\\rust-trade\\trade-test\\src\\feature_report"
    os.makedirs(output_dir, exist_ok=True)
    
    # 保存信号数据
    signal_data = df[df['signal'] != 0][['open_time', 'close', 'ma192', 'boll_mid', 'boll_width_pct', 'compression_bars', 'signal']].copy()
    signal_data['signal_type'] = signal_data['signal'].map({1: 'LONG', -1: 'SHORT'})
    
    output_file = os.path.join(output_dir, "strategy_c_signals.csv")
    signal_data.to_csv(output_file, index=False)
    
    print(f"\n信号数据已保存到: {output_file}")
    
    # 显示前几个信号
    if len(signal_data) > 0:
        print(f"\n前5个信号:")
        print(signal_data.head())
    
    return df, signal_data

if __name__ == "__main__":
    print("=" * 80)
    print("方案C：BOLL压缩突破策略 - 简化测试")
    print("=" * 80)
    
    df, signal_data = test_strategy_c()