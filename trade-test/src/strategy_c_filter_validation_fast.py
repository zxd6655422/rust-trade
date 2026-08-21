#!/usr/bin/env python3
"""
方案C过滤参数快速验证
"""

import pandas as pd
import numpy as np
import os
from datetime import datetime

def fast_validation():
    """快速验证过滤规则"""
    
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
    
    # 关键过滤规则
    filter_rules = [
        {'name': '无过滤（基线）', 'params': {}},
        {'name': 'BOLL带宽>0.2', 'params': {'boll_width_pct_min': 0.2}},
        {'name': 'BOLL带宽>0.25', 'params': {'boll_width_pct_min': 0.25}},
        {'name': '压缩bars<50', 'params': {'compression_bars_max': 50}},
        {'name': '压缩bars<40', 'params': {'compression_bars_max': 40}},
        {'name': 'RSI 35-65', 'params': {'rsi_min': 35, 'rsi_max': 65}},
        {'name': '组合1', 'params': {'boll_width_pct_min': 0.25, 'compression_bars_max': 40}},
        {'name': '组合2', 'params': {'boll_width_pct_min': 0.2, 'rsi_min': 35, 'rsi_max': 65}},
    ]
    
    results = {}
    
    for symbol in symbols:
        print(f"\n验证 {symbol}...")
        
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
        
        # 时间切分
        split_idx = int(len(df) * 0.7)
        train_df = df.iloc[:split_idx].copy()
        test_df = df.iloc[split_idx:].copy()
        
        print(f"  训练集: {len(train_df)} 根K线")
        print(f"  测试集: {len(test_df)} 根K线")
        
        symbol_results = []
        
        for rule in filter_rules:
            # 训练集测试
            train_signals = generate_signals(train_df, rule['params'])
            train_result = run_backtest(train_signals, base_params)
            
            # 测试集测试
            test_signals = generate_signals(test_df, rule['params'])
            test_result = run_backtest(test_signals, base_params)
            
            # 计算稳健性得分
            robustness_score = 0
            
            # 胜率稳健性
            if train_result['win_rate'] > 0:
                win_rate_ratio = test_result['win_rate'] / train_result['win_rate']
                if win_rate_ratio > 0.8:
                    robustness_score += 1
            
            # 收益稳健性
            if train_result['compound_return_pct'] > 0:
                return_ratio = test_result['compound_return_pct'] / train_result['compound_return_pct']
                if return_ratio > 0.7:
                    robustness_score += 1
            
            # 交易数稳健性
            if test_result['total_trades'] > 10:
                robustness_score += 1
            
            # 盈亏比稳健性
            if test_result['profit_factor'] > 1.5:
                robustness_score += 1
            
            symbol_results.append({
                'filter_name': rule['name'],
                'filter_params': rule['params'],
                'train_result': train_result,
                'test_result': test_result,
                'robustness_score': robustness_score
            })
            
            print(f"  {rule['name']}:")
            print(f"    训练集: 交易数={train_result['total_trades']}, 胜率={train_result['win_rate']:.1f}%, 复利={train_result['compound_return_pct']:.2f}%")
            print(f"    测试集: 交易数={test_result['total_trades']}, 胜率={test_result['win_rate']:.1f}%, 复利={test_result['compound_return_pct']:.2f}%")
            print(f"    稳健性得分: {robustness_score}/4")
        
        results[symbol] = symbol_results
    
    return results

def generate_signals(df, filter_params):
    """生成交易信号"""
    df = df.copy()
    
    # 基础信号条件
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
    
    # 生成信号
    df['signal'] = 0
    df.loc[long_condition, 'signal'] = 1
    df.loc[short_condition, 'signal'] = -1
    
    return df

def run_backtest(df, params):
    """运行回测"""
    df = df.copy()
    
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
            if current_profit_pct <= -params['hard_stop_pct']:
                exit_reason = 'hard_stop'
            
            # BOLL中轨止损
            elif params['boll_stop_enabled']:
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
    if not trades:
        return {
            'total_trades': 0,
            'win_rate': 0,
            'compound_return_pct': 0,
            'max_drawdown_pct': 0,
            'profit_factor': 0
        }
    
    trades_df = pd.DataFrame(trades)
    
    total_trades = len(trades_df)
    winning_trades = len(trades_df[trades_df['pnl_pct'] > 0])
    
    win_rate = winning_trades / total_trades * 100 if total_trades > 0 else 0
    
    final_capital = capital
    compound_return = (final_capital - initial_capital) / initial_capital * 100
    
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
    
    return {
        'total_trades': total_trades,
        'win_rate': win_rate,
        'compound_return_pct': compound_return,
        'max_drawdown_pct': max_drawdown_pct,
        'profit_factor': profit_factor
    }

def main():
    """主函数"""
    print("=" * 80)
    print("方案C过滤参数快速验证（时间切分）")
    print("=" * 80)
    
    results = fast_validation()
    
    # 生成报告
    output_dir = "F:\\rust-projects\\rust-trade\\trade-test\\src\\feature_report"
    os.makedirs(output_dir, exist_ok=True)
    
    report_file = os.path.join(output_dir, "strategy_c_filter_validation_fast_report.md")
    
    with open(report_file, 'w', encoding='utf-8') as f:
        f.write("# 方案C过滤参数快速验证报告\n\n")
        f.write(f"> 生成时间：{datetime.now().strftime('%Y-%m-%d %H:%M')}\n")
        f.write("> 验证目标：测试过滤规则的稳健性\n")
        f.write("> 方法：70%训练集 + 30%测试集\n\n")
        
        f.write("## 一、各币种验证结果\n\n")
        
        for symbol, symbol_results in results.items():
            f.write(f"### {symbol}\n\n")
            f.write("| 过滤规则 | 训练集交易数 | 训练集胜率 | 训练集复利 | 测试集交易数 | 测试集胜率 | 测试集复利 | 稳健性得分 |\n")
            f.write("|----------|--------------|------------|------------|--------------|------------|------------|------------|\n")
            
            for result in symbol_results:
                train = result['train_result']
                test = result['test_result']
                
                f.write(f"| {result['filter_name']} | {train['total_trades']} | {train['win_rate']:.1f}% | "
                       f"{train['compound_return_pct']:.2f}% | {test['total_trades']} | {test['win_rate']:.1f}% | "
                       f"{test['compound_return_pct']:.2f}% | {result['robustness_score']}/4 |\n")
            
            f.write("\n")
            
            # 找出最稳健的规则
            robust_rules = [r for r in symbol_results if r['robustness_score'] >= 3]
            if robust_rules:
                best_rule = max(robust_rules, key=lambda x: x['test_result']['compound_return_pct'])
                f.write(f"**最稳健规则**: {best_rule['filter_name']}\n")
                f.write(f"- 稳健性得分: {best_rule['robustness_score']}/4\n")
                f.write(f"- 测试集复利: {best_rule['test_result']['compound_return_pct']:.2f}%\n")
                f.write(f"- 测试集胜率: {best_rule['test_result']['win_rate']:.1f}%\n\n")
        
        f.write("## 二、总结\n\n")
        
        # 收集各币种最稳健规则
        best_rules = {}
        for symbol, symbol_results in results.items():
            robust_rules = [r for r in symbol_results if r['robustness_score'] >= 3]
            if robust_rules:
                best_rule = max(robust_rules, key=lambda x: x['test_result']['compound_return_pct'])
                best_rules[symbol] = best_rule
        
        if best_rules:
            f.write("### 2.1 各币种最稳健过滤规则\n\n")
            f.write("| 币种 | 最稳健规则 | 稳健性得分 | 测试集复利 | 测试集胜率 |\n")
            f.write("|------|------------|------------|------------|------------|\n")
            
            for symbol, best_rule in best_rules.items():
                test = best_rule['test_result']
                f.write(f"| {symbol} | {best_rule['filter_name']} | {best_rule['robustness_score']}/4 | "
                       f"{test['compound_return_pct']:.2f}% | {test['win_rate']:.1f}% |\n")
            
            f.write("\n")
        
        f.write("### 2.2 避免过拟合的建议\n\n")
        f.write("1. **选择稳健性得分≥3的规则**: 这些规则在样本外表现稳定\n")
        f.write("2. **避免过度优化**: 不要针对特定时间段优化\n")
        f.write("3. **跨币种验证**: 在多个币种上验证规则的有效性\n")
        f.write("4. **保持简单**: 简单的过滤规则通常更稳健\n\n")
        
        f.write("## 三、相关文件\n\n")
        f.write("- 验证脚本: `src/strategy_c_filter_validation_fast.py`\n")
        f.write("- 本报告: `src/feature_report/strategy_c_filter_validation_fast_report.md`\n")
    
    print(f"\n报告已生成: {report_file}")

if __name__ == "__main__":
    main()