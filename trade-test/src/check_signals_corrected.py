#!/usr/bin/env python3
"""
检查特定时间点的信号（修正版）
"""

import pandas as pd
import numpy as np
import os
from datetime import datetime

def check_signals_corrected():
    """检查特定时间点的信号（修正版）"""
    
    # 数据目录
    data_dir = "F:\\rust-projects\\data_2026-08-13"
    
    # 加载BTC数据
    file_path = os.path.join(data_dir, "kline_30m_BTC.csv")
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
    df['ma192'] = df['close'].rolling(window=192).mean()
    df['boll_mid'] = df['close'].rolling(window=100).mean()
    df['boll_std_val'] = df['close'].rolling(window=100).std()
    df['boll_upper'] = df['boll_mid'] + (2.0 * df['boll_std_val'])
    df['boll_lower'] = df['boll_mid'] - (2.0 * df['boll_std_val'])
    df['boll_width'] = (df['boll_upper'] - df['boll_lower']) / df['boll_mid']
    df['boll_width_pct'] = df['boll_width'].rolling(window=200).apply(
        lambda x: (x.iloc[-1] - x.min()) / (x.max() - x.min()) if x.max() != x.min() else 0.5
    )
    df['is_compressed'] = df['boll_width_pct'] < 0.3
    compression_groups = (~df['is_compressed']).cumsum()
    df['compression_bars'] = df.groupby(compression_groups)['is_compressed'].cumsum()
    df['valid_compression'] = (df['is_compressed']) & (df['compression_bars'] >= 10)
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
    
    # 检查特定时间点
    target_times = [
        "2026-06-23 12:30:00",  # 预期做空信号
        "2026-08-03 22:00:00"   # 预期做多信号
    ]
    
    print("=" * 80)
    print("检查特定时间点的信号（修正版）")
    print("=" * 80)
    
    for target_time in target_times:
        print(f"\n检查时间: {target_time}")
        
        # 查找该时间点的数据
        target_dt = pd.to_datetime(target_time)
        target_data = df[df['open_time'] == target_dt]
        
        if len(target_data) == 0:
            print(f"  未找到该时间点的数据")
            continue
        
        target_row = target_data.iloc[0]
        idx = target_data.index[0]
        
        print(f"  价格: {target_row['close']:.2f}")
        print(f"  MA192: {target_row['ma192']:.2f}")
        print(f"  BOLL中轨: {target_row['boll_mid']:.2f}")
        print(f"  BOLL上轨: {target_row['boll_upper']:.2f}")
        print(f"  BOLL下轨: {target_row['boll_lower']:.2f}")
        print(f"  MA192 < BOLL中轨: {target_row['ma_below_mid']}")
        print(f"  MA192 > BOLL中轨: {target_row['ma_above_mid']}")
        print(f"  收盘价向上穿越MA192: {target_row['cross_above_ma']}")
        print(f"  收盘价向下穿越MA192: {target_row['cross_below_ma']}")
        print(f"  有效压缩状态: {target_row['valid_compression']}")
        print(f"  压缩bars: {target_row['compression_bars']}")
        print(f"  BOLL宽度百分比: {target_row['boll_width_pct']:.4f}")
        
        # 检查信号
        if target_row['signal'] == 1:
            print(f"  信号: 做多 (+1) [YES]")
        elif target_row['signal'] == -1:
            print(f"  信号: 做空 (-1) [YES]")
        else:
            print(f"  信号: 无信号 (0) [NO]")
        
        # 检查前一根K线的情况
        if idx > 0:
            prev_row = df.iloc[idx - 1]
            print(f"\n  前一根K线:")
            print(f"    价格: {prev_row['close']:.2f}")
            print(f"    MA192: {prev_row['ma192']:.2f}")
            if target_time == "2026-06-23 12:30:00":
                print(f"    收盘价 >= MA192: {prev_row['close'] >= prev_row['ma192']}")
                print(f"    收盘价 < MA192: {prev_row['close'] < prev_row['ma192']}")
            else:
                print(f"    收盘价 <= MA192: {prev_row['close'] <= prev_row['ma192']}")
                print(f"    收盘价 > MA192: {prev_row['close'] > prev_row['ma192']}")
        
        # 分析为什么有或没有信号
        print(f"\n  信号分析:")
        if target_row['signal'] == 0:
            if target_time == "2026-06-23 12:30:00":
                # 预期做空信号
                if not target_row['ma_below_mid']:
                    print(f"    原因: MA192不小于BOLL中轨 (MA192={target_row['ma192']:.2f}, BOLL中轨={target_row['boll_mid']:.2f})")
                elif not target_row['cross_below_ma']:
                    print(f"    原因: 没有发生向下穿越")
                elif not target_row['valid_compression']:
                    print(f"    原因: 不在有效压缩状态")
                else:
                    print(f"    原因: 其他条件不满足")
            else:
                # 预期做多信号
                if not target_row['ma_above_mid']:
                    print(f"    原因: MA192不大于BOLL中轨 (MA192={target_row['ma192']:.2f}, BOLL中轨={target_row['boll_mid']:.2f})")
                elif not target_row['cross_above_ma']:
                    print(f"    原因: 没有发生向上穿越")
                elif not target_row['valid_compression']:
                    print(f"    原因: 不在有效压缩状态")
                else:
                    print(f"    原因: 其他条件不满足")
        else:
            print(f"    信号已生成，符合策略逻辑")

def main():
    """主函数"""
    check_signals_corrected()

if __name__ == "__main__":
    main()