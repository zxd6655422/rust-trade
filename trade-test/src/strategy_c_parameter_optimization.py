#!/usr/bin/env python3
"""
方案C参数优化
"""

import pandas as pd
import numpy as np
import os
from itertools import product
import json
from datetime import datetime

class StrategyCBacktester:
    """策略C回测器"""
    
    def __init__(self, data_dir: str):
        """
        初始化回测器
        
        Args:
            data_dir: 数据目录
        """
        self.data_dir = data_dir
        self.results = {}
    
    def load_data(self, symbol: str) -> pd.DataFrame:
        """加载数据"""
        file_path = os.path.join(self.data_dir, f"kline_30m_{symbol}.csv")
        if not os.path.exists(file_path):
            raise FileNotFoundError(f"数据文件不存在: {file_path}")
        
        df = pd.read_csv(file_path)
        df['open_time'] = pd.to_datetime(df['open_time'])
        return df
    
    def calculate_indicators(self, df: pd.DataFrame, params: dict) -> pd.DataFrame:
        """
        计算技术指标
        
        Args:
            df: 原始数据
            params: 参数字典
            
        Returns:
            添加了指标的DataFrame
        """
        df = df.copy()
        
        # 提取参数
        ma_period = params.get('ma_period', 192)
        boll_period = params.get('boll_period', 100)
        boll_std = params.get('boll_std', 2.0)
        compression_threshold = params.get('compression_threshold', 0.3)
        min_compression_bars = params.get('min_compression_bars', 10)
        
        # 计算MA
        df['ma192'] = df['close'].rolling(window=ma_period).mean()
        
        # 计算BOLL
        df['boll_mid'] = df['close'].rolling(window=boll_period).mean()
        df['boll_std'] = df['close'].rolling(window=boll_period).std()
        df['boll_upper'] = df['boll_mid'] + (boll_std * df['boll_std'])
        df['boll_lower'] = df['boll_mid'] - (boll_std * df['boll_std'])
        
        # 计算BOLL带宽
        df['boll_width'] = (df['boll_upper'] - df['boll_lower']) / df['boll_mid']
        
        # 计算带宽百分比
        df['boll_width_pct'] = df['boll_width'].rolling(window=200).apply(
            lambda x: (x.iloc[-1] - x.min()) / (x.max() - x.min()) if x.max() != x.min() else 0.5
        )
        
        # 压缩检测
        df['is_compressed'] = df['boll_width_pct'] < compression_threshold
        
        # 计算连续压缩的bar数
        compression_groups = (~df['is_compressed']).cumsum()
        df['compression_bars'] = df.groupby(compression_groups)['is_compressed'].cumsum()
        
        # 有效压缩
        df['valid_compression'] = (df['is_compressed']) & (df['compression_bars'] >= min_compression_bars)
        
        # MA192与BOLL中轨关系
        df['ma_above_mid'] = df['ma192'] > df['boll_mid']
        df['ma_below_mid'] = df['ma192'] < df['boll_mid']
        
        # 穿越信号
        df['cross_above_ma'] = (df['close'] > df['ma192']) & (df['close'].shift(1) <= df['ma192'].shift(1))
        df['cross_below_ma'] = (df['close'] < df['ma192']) & (df['close'].shift(1) >= df['ma192'].shift(1))
        
        return df
    
    def generate_signals(self, df: pd.DataFrame) -> pd.DataFrame:
        """生成交易信号"""
        df = df.copy()
        
        # 做多信号
        long_condition = (
            df['ma_above_mid'] &
            df['cross_above_ma'] &
            df['valid_compression']
        )
        
        # 做空信号
        short_condition = (
            df['ma_below_mid'] &
            df['cross_below_ma'] &
            df['valid_compression']
        )
        
        df['signal'] = 0
        df.loc[long_condition, 'signal'] = 1
        df.loc[short_condition, 'signal'] = -1
        
        return df
    
    def run_backtest(self, df: pd.DataFrame, params: dict) -> dict:
        """
        运行回测
        
        Args:
            df: 包含信号的DataFrame
            params: 策略参数
            
        Returns:
            回测结果
        """
        df = df.copy()
        
        # 提取参数
        hard_stop_pct = params.get('hard_stop_pct', 2.0)
        boll_stop_enabled = params.get('boll_stop_enabled', True)
        trailing_enabled = params.get('trailing_enabled', False)
        trailing_activate_pct = params.get('trailing_activate_pct', 4.0)
        trailing_callback_pct = params.get('trailing_callback_pct', 1.0)
        
        # 初始化回测变量
        initial_capital = 10000.0
        capital = initial_capital
        position = 0
        entry_price = 0.0
        entry_time = None
        max_profit_pct = 0.0
        trades = []
        
        # 遍历每个bar
        for i in range(1, len(df)):
            current_bar = df.iloc[i]
            prev_bar = df.iloc[i-1]
            
            # 如果有持仓，检查出场条件
            if position != 0:
                # 更新最大浮盈
                if position == 1:
                    current_profit_pct = (current_bar['close'] - entry_price) / entry_price * 100
                else:
                    current_profit_pct = (entry_price - current_bar['close']) / entry_price * 100
                
                max_profit_pct = max(max_profit_pct, current_profit_pct)
                
                # 检查出场条件
                exit_reason = None
                exit_price = current_bar['close']
                
                # 1. 硬止损
                if current_profit_pct <= -hard_stop_pct:
                    exit_reason = 'hard_stop'
                
                # 2. BOLL中轨止损
                elif boll_stop_enabled:
                    if position == 1 and current_bar['close'] < current_bar['boll_mid']:
                        exit_reason = 'boll_mid_stop'
                    elif position == -1 and current_bar['close'] > current_bar['boll_mid']:
                        exit_reason = 'boll_mid_stop'
                
                # 3. 移动止盈
                elif trailing_enabled and max_profit_pct >= trailing_activate_pct:
                    if position == 1:
                        drawdown = max_profit_pct - current_profit_pct
                    else:
                        drawdown = max_profit_pct - current_profit_pct
                    
                    if drawdown >= trailing_callback_pct:
                        exit_reason = 'trailing_stop'
                
                # 执行出场
                if exit_reason:
                    if position == 1:
                        pnl_pct = (exit_price - entry_price) / entry_price * 100
                    else:
                        pnl_pct = (entry_price - exit_price) / entry_price * 100
                    
                    pnl_amount = capital * (pnl_pct / 100)
                    capital += pnl_amount
                    
                    trade = {
                        'entry_time': entry_time,
                        'exit_time': current_bar['open_time'] if 'open_time' in current_bar.index else i,
                        'direction': 'LONG' if position == 1 else 'SHORT',
                        'entry_price': entry_price,
                        'exit_price': exit_price,
                        'pnl_pct': pnl_pct,
                        'pnl_amount': pnl_amount,
                        'exit_reason': exit_reason,
                        'max_profit_pct': max_profit_pct,
                        'compression_bars_at_entry': prev_bar.get('compression_bars', 0),
                        'boll_width_pct_at_entry': prev_bar.get('boll_width_pct', 0)
                    }
                    trades.append(trade)
                    
                    position = 0
                    entry_price = 0.0
                    entry_time = None
                    max_profit_pct = 0.0
            
            # 如果没有持仓，检查入场信号
            if position == 0 and current_bar['signal'] != 0:
                signal = current_bar['signal']
                entry_price = current_bar['close']
                entry_time = current_bar['open_time'] if 'open_time' in current_bar.index else i
                position = signal
                max_profit_pct = 0.0
        
        # 计算统计
        if not trades:
            return {
                'trades': [],
                'stats': {
                    'total_trades': 0,
                    'win_rate': 0,
                    'total_pnl_pct': 0,
                    'compound_return_pct': 0,
                    'final_capital': initial_capital,
                    'max_drawdown_pct': 0,
                    'profit_factor': 0
                }
            }
        
        trades_df = pd.DataFrame(trades)
        
        total_trades = len(trades_df)
        winning_trades = len(trades_df[trades_df['pnl_pct'] > 0])
        losing_trades = len(trades_df[trades_df['pnl_pct'] <= 0])
        
        win_rate = winning_trades / total_trades * 100 if total_trades > 0 else 0
        
        total_pnl_pct = trades_df['pnl_pct'].sum()
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
        avg_loss = abs(trades_df[trades_df['pnl_pct'] <= 0]['pnl_pct'].mean()) if losing_trades > 0 else 0
        profit_factor = avg_win / avg_loss if avg_loss > 0 else float('inf')
        
        return {
            'trades': trades,
            'stats': {
                'total_trades': total_trades,
                'winning_trades': winning_trades,
                'losing_trades': losing_trades,
                'win_rate': win_rate,
                'total_pnl_pct': total_pnl_pct,
                'compound_return_pct': compound_return,
                'final_capital': final_capital,
                'max_drawdown_pct': max_drawdown_pct,
                'avg_win_pct': avg_win,
                'avg_loss_pct': avg_loss,
                'avg_trade_pnl_pct': trades_df['pnl_pct'].mean(),
                'profit_factor': profit_factor,
                'avg_compression_bars': trades_df['compression_bars_at_entry'].mean(),
                'avg_boll_width_pct': trades_df['boll_width_pct_at_entry'].mean()
            }
        }
    
    def optimize_parameters(self, symbol: str, param_grid: dict) -> pd.DataFrame:
        """
        优化参数
        
        Args:
            symbol: 交易对符号
            param_grid: 参数网格
            
        Returns:
            优化结果DataFrame
        """
        print(f"优化 {symbol} 参数...")
        
        # 加载数据
        df = self.load_data(symbol)
        
        # 生成参数组合
        param_names = list(param_grid.keys())
        param_values = list(param_grid.values())
        param_combinations = list(product(*param_values))
        
        results = []
        
        for i, params_values in enumerate(param_combinations):
            params = dict(zip(param_names, params_values))
            
            # 计算指标
            df_with_indicators = self.calculate_indicators(df, params)
            
            # 生成信号
            df_with_signals = self.generate_signals(df_with_indicators)
            
            # 运行回测
            backtest_result = self.run_backtest(df_with_signals, params)
            
            # 记录结果
            result = {
                'symbol': symbol,
                'params': params,
                'stats': backtest_result['stats']
            }
            results.append(result)
            
            if (i + 1) % 10 == 0:
                print(f"  完成 {i + 1}/{len(param_combinations)} 个参数组合")
        
        # 转换为DataFrame
        results_df = pd.DataFrame(results)
        
        # 添加参数列
        for param_name in param_names:
            results_df[param_name] = results_df['params'].apply(lambda x: x[param_name])
        
        # 添加统计列
        stats_keys = ['total_trades', 'win_rate', 'compound_return_pct', 'max_drawdown_pct', 'profit_factor']
        for key in stats_keys:
            results_df[key] = results_df['stats'].apply(lambda x: x.get(key, 0))
        
        return results_df

def main():
    """主函数"""
    # 数据目录
    data_dir = "F:\\rust-projects\\rust-trade\\trade-test\\src\\sample_data"
    
    # 初始化回测器
    backtester = StrategyCBacktester(data_dir)
    
    # 定义参数网格
    param_grid = {
        'ma_period': [192],  # 固定MA192
        'boll_period': [100],  # 固定BOLL周期
        'boll_std': [2.0],  # 固定标准差倍数
        'compression_threshold': [0.2, 0.3, 0.4],  # 压缩阈值
        'min_compression_bars': [5, 10, 15],  # 最小压缩bars
        'hard_stop_pct': [1.5, 2.0, 2.5],  # 硬止损百分比
        'boll_stop_enabled': [True],  # 固定启用BOLL止损
        'trailing_enabled': [False],  # 暂不启用移动止盈
        'trailing_activate_pct': [4.0],  # 固定值
        'trailing_callback_pct': [1.0]  # 固定值
    }
    
    # 测试币种
    symbols = ['BTC', 'ETH', 'SOL']
    
    all_results = []
    
    for symbol in symbols:
        try:
            results_df = backtester.optimize_parameters(symbol, param_grid)
            all_results.append(results_df)
            
            # 显示最佳结果
            best_result = results_df.sort_values('compound_return_pct', ascending=False).iloc[0]
            print(f"\n{symbol} 最佳参数:")
            print(f"  参数: {best_result['params']}")
            print(f"  复利收益: {best_result['compound_return_pct']:.2f}%")
            print(f"  胜率: {best_result['win_rate']:.1f}%")
            print(f"  最大回撤: {best_result['max_drawdown_pct']:.2f}%")
            print(f"  盈亏比: {best_result['profit_factor']:.2f}")
            
        except Exception as e:
            print(f"{symbol} 优化失败: {e}")
    
    # 保存结果
    if all_results:
        combined_results = pd.concat(all_results, ignore_index=True)
        
        output_dir = "F:\\rust-projects\\rust-trade\\trade-test\\src\\feature_report"
        os.makedirs(output_dir, exist_ok=True)
        
        output_file = os.path.join(output_dir, "strategy_c_parameter_optimization.csv")
        combined_results.to_csv(output_file, index=False)
        
        print(f"\n优化结果已保存到: {output_file}")
        
        # 显示全局最佳结果
        global_best = combined_results.sort_values('compound_return_pct', ascending=False).iloc[0]
        print(f"\n全局最佳结果:")
        print(f"  币种: {global_best['symbol']}")
        print(f"  参数: {global_best['params']}")
        print(f"  复利收益: {global_best['compound_return_pct']:.2f}%")

if __name__ == "__main__":
    print("=" * 80)
    print("方案C参数优化")
    print("=" * 80)
    
    main()