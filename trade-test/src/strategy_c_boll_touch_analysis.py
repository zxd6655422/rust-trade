#!/usr/bin/env python3
"""
方案C BOLL接触分析
分析BOLL收窄期间频繁接触上下轨的情况
"""

import pandas as pd
import numpy as np
import os
from datetime import datetime

def analyze_boll_touches():
    """分析BOLL接触情况"""
    
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
    
    # 滑动窗口参数
    window_sizes = [20, 30, 50]  # 不同的窗口大小
    touch_thresholds = [3, 5, 7]  # 不同的接触次数阈值
    
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
        
        # 计算BOLL接触
        # 定义接触：价格接近上下轨（在1%范围内）
        touch_threshold_pct = 0.01  # 1%范围内算接触
        
        df['touch_upper'] = (df['high'] >= df['boll_upper'] * (1 - touch_threshold_pct)) | \
                           (df['close'] >= df['boll_upper'] * (1 - touch_threshold_pct))
        df['touch_lower'] = (df['low'] <= df['boll_lower'] * (1 + touch_threshold_pct)) | \
                           (df['close'] <= df['boll_lower'] * (1 + touch_threshold_pct))
        
        # 计算滑动窗口内的接触次数
        for window in window_sizes:
            df[f'touch_upper_{window}'] = df['touch_upper'].rolling(window=window).sum()
            df[f'touch_lower_{window}'] = df['touch_lower'].rolling(window=window).sum()
        
        # 分析压缩期间的接触情况
        compressed_df = df[df['valid_compression']].copy()
        
        print(f"  压缩期间bar数: {len(compressed_df)}")
        
        # 分析不同窗口和阈值的过滤效果
        symbol_results = {}
        
        for window in window_sizes:
            for threshold in touch_thresholds:
                # 过滤条件：频繁接触下轨不做多，频繁接触上轨不做空
                long_condition = (
                    df['ma_above_mid'] &
                    df['cross_above_ma'] &
                    df['valid_compression'] &
                    (df[f'touch_lower_{window}'] < threshold)  # 不频繁接触下轨
                )
                
                short_condition = (
                    df['ma_below_mid'] &
                    df['cross_below_ma'] &
                    df['valid_compression'] &
                    (df[f'touch_upper_{window}'] < threshold)  # 不频繁接触上轨
                )
                
                df['signal'] = 0
                df.loc[long_condition, 'signal'] = 1
                df.loc[short_condition, 'signal'] = -1
                
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
                    
                    win_rate = winning_trades / total_trades * 100 if total_trades > 0 else 0
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
                    avg_loss = abs(trades_df[trades_df['pnl_pct'] <= 0]['pnl_pct'].mean()) if (total_trades - winning_trades) > 0 else 0
                    profit_factor = avg_win / avg_loss if avg_loss > 0 else float('inf')
                    
                    result_key = f"window_{window}_threshold_{threshold}"
                    symbol_results[result_key] = {
                        'window': window,
                        'threshold': threshold,
                        'total_trades': total_trades,
                        'win_rate': win_rate,
                        'compound_return': compound_return,
                        'max_drawdown': max_drawdown_pct,
                        'profit_factor': profit_factor
                    }
                    
                    print(f"  窗口{window}, 阈值{threshold}: 交易数={total_trades}, 胜率={win_rate:.1f}%, 复利={compound_return:.2f}%")
                else:
                    print(f"  窗口{window}, 阈值{threshold}: 无交易")
        
        results[symbol] = symbol_results
    
    return results

def main():
    """主函数"""
    print("=" * 80)
    print("方案C BOLL接触分析")
    print("=" * 80)
    
    results = analyze_boll_touches()
    
    # 生成报告
    output_dir = "F:\\rust-projects\\rust-trade\\trade-test\\src\\feature_report"
    os.makedirs(output_dir, exist_ok=True)
    
    report_file = os.path.join(output_dir, "strategy_c_boll_touch_report.md")
    
    with open(report_file, 'w', encoding='utf-8') as f:
        f.write("# 方案C BOLL接触分析报告\n\n")
        f.write(f"> 生成时间：{datetime.now().strftime('%Y-%m-%d %H:%M')}\n")
        f.write("> 分析目标：分析BOLL收窄期间频繁接触上下轨的情况\n\n")
        
        f.write("## 一、分析逻辑\n\n")
        f.write("### 1.1 BOLL接触定义\n\n")
        f.write("- **接触上轨**：价格高点或收盘价在上轨1%范围内\n")
        f.write("- **接触下轨**：价格低点或收盘价在下轨1%范围内\n\n")
        
        f.write("### 1.2 过滤规则\n\n")
        f.write("- **频繁接触下轨不做多**：在滑动窗口内，如果接触下轨次数超过阈值，则不做多\n")
        f.write("- **频繁接触上轨不做空**：在滑动窗口内，如果接触上轨次数超过阈值，则不做空\n\n")
        
        f.write("## 二、各币种分析结果\n\n")
        
        for symbol, symbol_results in results.items():
            f.write(f"### {symbol}\n\n")
            f.write("| 窗口大小 | 接触阈值 | 交易数 | 胜率 | 复利收益 | 最大回撤 | 盈亏比 |\n")
            f.write("|----------|----------|--------|------|----------|----------|--------|\n")
            
            for key, result in symbol_results.items():
                f.write(f"| {result['window']} | {result['threshold']} | {result['total_trades']} | "
                       f"{result['win_rate']:.1f}% | {result['compound_return']:.2f}% | "
                       f"{result['max_drawdown']:.2f}% | {result['profit_factor']:.2f} |\n")
            
            f.write("\n")
        
        f.write("## 三、关键发现\n\n")
        
        # 找出最佳参数
        best_results = {}
        for symbol, symbol_results in results.items():
            if symbol_results:
                best_key = max(symbol_results.keys(), key=lambda x: symbol_results[x]['compound_return'])
                best_results[symbol] = symbol_results[best_key]
        
        f.write("### 3.1 各币种最佳参数\n\n")
        f.write("| 币种 | 最佳窗口 | 最佳阈值 | 交易数 | 胜率 | 复利收益 |\n")
        f.write("|------|----------|----------|--------|------|----------|\n")
        
        for symbol, result in best_results.items():
            f.write(f"| {symbol} | {result['window']} | {result['threshold']} | {result['total_trades']} | "
                   f"{result['win_rate']:.1f}% | {result['compound_return']:.2f}% |\n")
        
        f.write("\n")
        
        f.write("### 3.2 过滤效果分析\n\n")
        f.write("1. **减少频繁接触时的入场**：过滤掉在压缩期间频繁接触上下轨的信号\n")
        f.write("2. **提高信号质量**：只在价格突破时入场，避免在震荡中频繁交易\n")
        f.write("3. **减少假突破**：频繁接触上下轨可能是假突破的信号\n\n")
        
        f.write("## 四、优化建议\n\n")
        f.write("1. **参数优化**：测试更多窗口大小和阈值组合\n")
        f.write("2. **动态阈值**：根据市场波动率调整阈值\n")
        f.write("3. **结合其他过滤**：与RSI、动量等指标结合\n")
        f.write("4. **样本外验证**：在样本外数据上验证过滤效果\n\n")
        
        f.write("## 五、相关文件\n\n")
        f.write("- 分析脚本: `src/strategy_c_boll_touch_analysis.py`\n")
        f.write("- 本报告: `src/feature_report/strategy_c_boll_touch_report.md`\n")
    
    print(f"\n报告已生成: {report_file}")

if __name__ == "__main__":
    main()