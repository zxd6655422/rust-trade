#!/usr/bin/env python3
"""
生成示例数据用于测试策略
"""

import pandas as pd
import numpy as np
import os
from datetime import datetime, timedelta

def generate_sample_data(symbol: str, start_date: str = "2020-01-01", end_date: str = "2026-08-01"):
    """
    生成示例K线数据
    
    Args:
        symbol: 交易对符号
        start_date: 开始日期
        end_date: 结束日期
    """
    # 生成时间序列（30分钟间隔）
    start = pd.to_datetime(start_date)
    end = pd.to_datetime(end_date)
    
    # 计算30分钟间隔的数量
    intervals = int((end - start).total_seconds() / 1800)  # 1800秒 = 30分钟
    
    # 生成时间戳
    timestamps = [start + timedelta(minutes=30*i) for i in range(intervals)]
    
    # 生成价格数据（随机游走 + 趋势）
    np.random.seed(42)  # 固定随机种子以便复现
    
    # 初始价格
    initial_prices = {
        'BTC': 7000,
        'ETH': 150,
        'SOL': 1.5,
        'BNB': 15,
        'SUI': 0.1,
        'HYPE': 10
    }
    
    initial_price = initial_prices.get(symbol, 100)
    
    # 生成价格序列
    prices = [initial_price]
    for i in range(1, intervals):
        # 添加趋势和波动
        trend = 0.0001  # 微小上升趋势
        volatility = 0.02  # 2%波动率
        
        # 随机价格变化
        change = np.random.normal(trend, volatility)
        new_price = prices[-1] * (1 + change)
        prices.append(max(new_price, 0.01))  # 确保价格为正
    
    # 生成OHLCV数据
    data = []
    for i in range(intervals):
        timestamp = timestamps[i]
        close = prices[i]
        
        # 生成开盘、最高、最低价
        open_price = close * (1 + np.random.uniform(-0.005, 0.005))
        high = max(open_price, close) * (1 + np.random.uniform(0, 0.01))
        low = min(open_price, close) * (1 - np.random.uniform(0, 0.01))
        
        # 生成成交量
        volume = np.random.uniform(1000, 10000) * (1 + np.random.uniform(-0.5, 0.5))
        
        data.append({
            'open_time': timestamp,
            'open': open_price,
            'high': high,
            'low': low,
            'close': close,
            'volume': volume
        })
    
    # 创建DataFrame
    df = pd.DataFrame(data)
    
    # 保存到CSV
    output_dir = "F:\\rust-projects\\rust-trade\\trade-test\\src\\sample_data"
    os.makedirs(output_dir, exist_ok=True)
    
    file_path = os.path.join(output_dir, f"kline_30m_{symbol}.csv")
    df.to_csv(file_path, index=False)
    
    print(f"生成 {symbol} 数据: {len(df)} 根K线")
    print(f"保存到: {file_path}")
    
    return df

def main():
    """主函数"""
    symbols = ['BTC', 'ETH', 'SOL', 'BNB', 'SUI', 'HYPE']
    
    print("生成示例数据用于策略测试...")
    
    for symbol in symbols:
        try:
            generate_sample_data(symbol)
        except Exception as e:
            print(f"生成 {symbol} 数据失败: {e}")
    
    print("\n示例数据生成完成！")

if __name__ == "__main__":
    main()