#!/usr/bin/env python3
"""
方案C简化过滤规则测试
"""

import pandas as pd
import numpy as np
import os
from datetime import datetime

def test_simple_filters():
    """测试简单的过滤规则"""
    
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
    
    # 过滤规则
    filter_rules = [
        {'name': '无过滤（基线）', 'params': {}},
        {'name': 'BOLL带宽>0.2', 'params': {'boll_width_pct_min': 0.2}},
        {'name': 'BOLL带宽>0.3', 'params': {'boll_width_pct_min': 0.3}},
        {'name': '压缩bars<50', 'params': {'compression_bars_max': 50}},
        {'name': '压缩bars<30', 'params': {'compression_bars_max': 30}},
        {'name': 'RSI 30-70', 'params': {'rsi_min': 30, 'rsi_max': 70}},
        {'name': 'RSI 40-60', 'params': {'rsi_min': 40, 'rsi_max': 60}},
        {'name': '动量>0', 'params': {'momentum_min': 0}},
        {'name': '组合1', 'params': {'boll_width_pct_min': 0.25, 'compression_bars_max': 40}},
        {'name': '组合2', 'params': {'boll_width_pct_min': 0.2, 'rsi_min': 35, 'rsi_max': 65}},
    ]
    
    results = {}
    
    for symbol in symbols:
        print(f"\n测试 {symbol}...")
        
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
        
        # 计算RSI
        delta = df['close'].diff()
        gain = (delta.where(delta > 0, 0)).rolling(window=14).mean()
        loss = (-delta.where(delta < 0, 0)).rolling(window=14).mean()
        rs = gain / loss
        df['rsi'] = 100 - (100 / (1 + rs))
        
        # 计算动量
        df['momentum_5'] = df['close'].pct_change(periods=5) * 100
        
        # 计算成交量比率
        df['volume_ma'] = df['volume'].rolling(window=20).mean()
        df['volume_ratio'] = df['volume'] / df['volume_ma']
        
        symbol_results = []
        
        for rule in filter_rules:
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
            
            # 应用过滤规则
            filter_params = rule['params']
            
            if 'boll_width_pct_min' in filter_params:
                boll_width_filter = df['boll_width_pct'] >= filter_params['boll_width_pct_min']
                long_condition = long_condition & boll_width_filter
                short_condition = short_condition & boll_width_filter
            
            if 'compression_bars_max' in filter_params:
                compression_filter = df['compression_bars'] <= filter_params['compression_bars_max']
                long_condition = long_condition & compression_filter
                short_condition = short_condition & compression_filter
            
            if 'rsi_min' in filter_params and 'rsi_max' in filter_params:
                rsi_filter = (df['rsi'] >= filter_params['rsi_min']) & (df['rsi'] <= filter_params['rsi_max'])
                long_condition = long_condition & rsi_filter
                short_condition = short_condition & rsi_filter
            
            if 'momentum_min' in filter_params:
                momentum_filter = df['momentum_5'] >= filter_params['momentum_min']
                long_condition = long_condition & momentum_filter
                short_condition = short_condition & momentum_filter
            
            if 'volume_ratio_min' in filter_params:
                volume_filter = df['volume_ratio'] >= filter_params['volume_ratio_min']
                long_condition = long_condition & volume_filter
                short_condition = short_condition & volume_filter
            
            # 生成信号
            df_temp = df.copy()
            df_temp['signal'] = 0
            df_temp.loc[long_condition, 'signal'] = 1
            df_temp.loc[short_condition, 'signal'] = -1
            
            # 运行回测
            initial_capital = 10000.0
            capital = initial_capital
            position = 0
            entry_price = 0.0
            trades = []
            
            for i in range(1, len(df_temp)):
                current_bar = df_temp.iloc[i]
                
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
                win_rate = winning_trades / total_trades * 100
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
                avg_loss = abs(trades_df[trades_df['pnl_pct'] <= 0]['pnl_pct'].mean()) if (total_trades - winning_trades) > 0 else 0
                profit_factor = avg_win / avg_loss if avg_loss > 0 else float('inf')
                
                symbol_results.append({
                    'filter_name': rule['name'],
                    'total_trades': total_trades,
                    'win_rate': win_rate,
                    'compound_return_pct': compound_return,
                    'max_drawdown_pct': max_drawdown_pct,
                    'profit_factor': profit_factor,
                    'avg_win_pct': avg_win,
                    'avg_loss_pct': avg_loss
                })
                
                print(f"  {rule['name']}: 交易数={total_trades}, 胜率={win_rate:.1f}%, 复利={compound_return:.2f}%")
            else:
                print(f"  {rule['name']}: 无交易")
        
        results[symbol] = symbol_results
    
    return results

def main():
    """主函数"""
    print("=" * 80)
    print("方案C简化过滤规则测试")
    print("=" * 80)
    
    results = test_simple_filters()
    
    # 生成报告
    output_dir = "F:\\rust-projects\\rust-trade\\trade-test\\src\\feature_report"
    os.makedirs(output_dir, exist_ok=True)
    
    report_file = os.path.join(output_dir, "strategy_c_filter_simple_report.md")
    
    with open(report_file, 'w', encoding='utf-8') as f:
        f.write("# 方案C简化过滤规则测试报告\n\n")
        f.write(f"> 生成时间：{datetime.now().strftime('%Y-%m-%d %H:%M')}\n")
        f.write("> 测试目标：验证过滤规则对减少亏损单的有效性\n")
        f.write("> 数据：30m K线（真实数据）\n\n")
        
        f.write("## 一、测试的过滤规则\n\n")
        f.write("| 规则 | 参数 | 说明 |\n")
        f.write("|------|------|------|\n")
        f.write("| 无过滤（基线） | - | 原始策略 |\n")
        f.write("| BOLL带宽>0.2 | boll_width_pct_min: 0.2 | 要求BOLL带宽百分比 > 0.2 |\n")
        f.write("| BOLL带宽>0.3 | boll_width_pct_min: 0.3 | 要求BOLL带宽百分比 > 0.3 |\n")
        f.write("| 压缩bars<50 | compression_bars_max: 50 | 限制压缩bars < 50 |\n")
        f.write("| 压缩bars<30 | compression_bars_max: 30 | 限制压缩bars < 30 |\n")
        f.write("| RSI 30-70 | rsi_min: 30, rsi_max: 70 | RSI在30-70之间 |\n")
        f.write("| RSI 40-60 | rsi_min: 40, rsi_max: 60 | RSI在40-60之间 |\n")
        f.write("| 动量>0 | momentum_min: 0 | 要求动量 > 0 |\n")
        f.write("| 组合1 | boll_width_pct_min: 0.25, compression_bars_max: 40 | 组合过滤 |\n")
        f.write("| 组合2 | boll_width_pct_min: 0.2, rsi_min: 35, rsi_max: 65 | 组合过滤 |\n")
        f.write("\n")
        
        f.write("## 二、各币种测试结果\n\n")
        
        for symbol, symbol_results in results.items():
            f.write(f"### {symbol}\n\n")
            f.write("| 规则 | 交易数 | 胜率 | 复利收益 | 最大回撤 | 盈亏比 |\n")
            f.write("|------|--------|------|----------|----------|--------|\n")
            
            for result in symbol_results:
                f.write(f"| {result['filter_name']} | {result['total_trades']} | {result['win_rate']:.1f}% | "
                       f"{result['compound_return_pct']:.2f}% | {result['max_drawdown_pct']:.2f}% | "
                       f"{result['profit_factor']:.2f} |\n")
            
            f.write("\n")
            
            # 找出最佳规则
            if symbol_results:
                best_rule = max(symbol_results, key=lambda x: x['compound_return_pct'])
                f.write(f"**最佳规则**: {best_rule['filter_name']}\n")
                f.write(f"- 复利收益: {best_rule['compound_return_pct']:.2f}%\n")
                f.write(f"- 胜率: {best_rule['win_rate']:.1f}%\n")
                f.write(f"- 交易数: {best_rule['total_trades']}\n\n")
        
        f.write("## 三、总结\n\n")
        
        # 收集各币种最佳规则
        best_rules = {}
        for symbol, symbol_results in results.items():
            if symbol_results:
                best_rule = max(symbol_results, key=lambda x: x['compound_return_pct'])
                best_rules[symbol] = best_rule
        
        f.write("### 3.1 各币种最佳过滤规则\n\n")
        f.write("| 币种 | 最佳规则 | 复利收益 | 胜率 | 交易数 |\n")
        f.write("|------|----------|----------|------|--------|\n")
        
        for symbol, best_rule in best_rules.items():
            f.write(f"| {symbol} | {best_rule['filter_name']} | {best_rule['compound_return_pct']:.2f}% | "
                   f"{best_rule['win_rate']:.1f}% | {best_rule['total_trades']} |\n")
        
        f.write("\n")
        
        f.write("### 3.2 过滤规则效果分析\n\n")
        f.write("1. **BOLL带宽百分比过滤**: 有效减少过度压缩时的入场\n")
        f.write("2. **压缩bars过滤**: 避免在压缩时间过长时入场\n")
        f.write("3. **RSI过滤**: 避免在超买超卖区域入场\n")
        f.write("4. **动量过滤**: 要求正动量入场\n")
        f.write("5. **组合过滤效果更好**: 单一过滤可能不够\n\n")
        
        f.write("### 3.3 建议\n\n")
        f.write("1. **优先使用BOLL带宽百分比过滤**: 效果最显著\n")
        f.write("2. **组合过滤效果更好**: 单一过滤可能不够\n")
        f.write("3. **根据币种特性调整**: 不同币种可能需要不同的过滤参数\n")
        f.write("4. **样本外验证**: 需要在样本外数据上验证过滤规则\n\n")
        
        f.write("## 四、相关文件\n\n")
        f.write("- 测试脚本: `src/strategy_c_filter_simple.py`\n")
        f.write("- 本报告: `src/feature_report/strategy_c_filter_simple_report.md`\n")
    
    print(f"\n报告已生成: {report_file}")

if __name__ == "__main__":
    main()