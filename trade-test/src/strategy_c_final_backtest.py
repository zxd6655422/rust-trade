#!/usr/bin/env python3
"""
方案C最终策略回测
基于修正后的逻辑进行完整回测
"""

import pandas as pd
import numpy as np
import os
from datetime import datetime
from typing import Dict, List, Tuple
import warnings
warnings.filterwarnings('ignore')

class FinalBacktester:
    """最终策略回测器"""
    
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
        df['boll_std_val'] = df['close'].rolling(window=boll_period).std()
        df['boll_upper'] = df['boll_mid'] + (boll_std * df['boll_std_val'])
        df['boll_lower'] = df['boll_mid'] - (boll_std * df['boll_std_val'])
        
        # 计算BOLL带宽
        df['boll_width'] = (df['boll_upper'] - df['boll_lower']) / df['boll_mid']
        
        # 计算带宽百分比（滚动窗口）
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
    
    def run_backtest(self, df: pd.DataFrame, params: dict) -> Dict:
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
        
        # 初始化回测变量
        initial_capital = 10000.0
        capital = initial_capital
        position = 0
        entry_price = 0.0
        entry_time = None
        trades = []
        
        # 遍历每个bar
        for i in range(1, len(df)):
            current_bar = df.iloc[i]
            
            # 如果有持仓，检查出场条件
            if position != 0:
                # 更新最大浮盈
                if position == 1:
                    current_profit_pct = (current_bar['close'] - entry_price) / entry_price * 100
                else:
                    current_profit_pct = (entry_price - current_bar['close']) / entry_price * 100
                
                # 检查出场条件
                exit_reason = None
                exit_price = current_bar['close']
                exit_time = current_bar['open_time']
                
                # 1. 硬止损
                if current_profit_pct <= -hard_stop_pct:
                    exit_reason = 'hard_stop'
                
                # 2. BOLL中轨止损
                elif boll_stop_enabled:
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
                    
                    trade = {
                        'entry_time': entry_time,
                        'exit_time': exit_time,
                        'direction': 'LONG' if position == 1 else 'SHORT',
                        'entry_price': entry_price,
                        'exit_price': exit_price,
                        'pnl_pct': pnl_pct,
                        'pnl_amount': pnl_amount,
                        'exit_reason': exit_reason,
                        'year': entry_time.year if hasattr(entry_time, 'year') else 0
                    }
                    trades.append(trade)
                    
                    position = 0
                    entry_price = 0.0
                    entry_time = None
            
            # 如果没有持仓，检查入场信号
            if position == 0 and current_bar['signal'] != 0:
                signal = current_bar['signal']
                entry_price = current_bar['close']
                entry_time = current_bar['open_time']
                position = signal
        
        return {
            'trades': trades,
            'final_capital': capital,
            'initial_capital': initial_capital
        }
    
    def calculate_statistics(self, trades: List[Dict], initial_capital: float = 10000.0) -> Dict:
        """计算交易统计"""
        if not trades:
            return {
                'total_trades': 0,
                'winning_trades': 0,
                'losing_trades': 0,
                'win_rate': 0,
                'total_pnl_pct': 0,
                'compound_return_pct': 0,
                'final_capital': initial_capital,
                'max_drawdown_pct': 0,
                'avg_win_pct': 0,
                'avg_loss_pct': 0,
                'profit_factor': 0
            }
        
        trades_df = pd.DataFrame(trades)
        
        total_trades = len(trades_df)
        winning_trades = len(trades_df[trades_df['pnl_pct'] > 0])
        losing_trades = len(trades_df[trades_df['pnl_pct'] <= 0])
        
        win_rate = winning_trades / total_trades * 100 if total_trades > 0 else 0
        
        total_pnl_pct = trades_df['pnl_pct'].sum()
        
        # 计算复利收益
        capital = initial_capital
        for trade in trades:
            capital += trade['pnl_amount']
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
            'profit_factor': profit_factor
        }
    
    def analyze_by_year(self, trades: List[Dict]) -> Dict:
        """按年度分析交易表现"""
        if not trades:
            return {}
        
        trades_df = pd.DataFrame(trades)
        trades_df['entry_time'] = pd.to_datetime(trades_df['entry_time'])
        trades_df['year'] = trades_df['entry_time'].dt.year
        
        yearly_stats = {}
        
        for year in sorted(trades_df['year'].unique()):
            year_trades = trades_df[trades_df['year'] == year]
            
            if len(year_trades) == 0:
                continue
            
            # 计算年度统计
            total_trades = len(year_trades)
            winning_trades = len(year_trades[year_trades['pnl_pct'] > 0])
            losing_trades = len(year_trades[year_trades['pnl_pct'] <= 0])
            
            win_rate = winning_trades / total_trades * 100 if total_trades > 0 else 0
            total_pnl_pct = year_trades['pnl_pct'].sum()
            
            # 计算年度复利
            year_initial_capital = 10000.0
            year_capital = year_initial_capital
            for _, trade in year_trades.iterrows():
                year_capital += trade['pnl_amount']
            year_compound_return = (year_capital - year_initial_capital) / year_initial_capital * 100
            
            # 计算年度最大回撤
            capital_curve = [year_initial_capital]
            current_capital = year_initial_capital
            for _, trade in year_trades.iterrows():
                current_capital += trade['pnl_amount']
                capital_curve.append(current_capital)
            
            capital_series = pd.Series(capital_curve)
            rolling_max = capital_series.expanding().max()
            drawdowns = (capital_series - rolling_max) / rolling_max * 100
            max_drawdown_pct = drawdowns.min()
            
            # 计算盈亏比
            avg_win = year_trades[year_trades['pnl_pct'] > 0]['pnl_pct'].mean() if winning_trades > 0 else 0
            avg_loss = abs(year_trades[year_trades['pnl_pct'] <= 0]['pnl_pct'].mean()) if losing_trades > 0 else 0
            profit_factor = avg_win / avg_loss if avg_loss > 0 else float('inf')
            
            # 离场原因统计
            exit_reasons = year_trades['exit_reason'].value_counts().to_dict()
            
            yearly_stats[year] = {
                'total_trades': total_trades,
                'winning_trades': winning_trades,
                'losing_trades': losing_trades,
                'win_rate': win_rate,
                'total_pnl_pct': total_pnl_pct,
                'compound_return_pct': year_compound_return,
                'max_drawdown_pct': max_drawdown_pct,
                'avg_win_pct': avg_win,
                'avg_loss_pct': avg_loss,
                'profit_factor': profit_factor,
                'exit_reasons': exit_reasons
            }
        
        return yearly_stats
    
    def analyze_symbol(self, symbol: str, params: dict) -> Dict:
        """分析单个币种"""
        print(f"分析 {symbol}...")
        
        # 加载数据
        df = self.load_data(symbol)
        print(f"  数据量: {len(df)} 根K线")
        print(f"  时间范围: {df['open_time'].min()} 到 {df['open_time'].max()}")
        
        # 计算指标
        df = self.calculate_indicators(df, params)
        
        # 生成信号
        df = self.generate_signals(df)
        
        # 统计信号
        long_signals = (df['signal'] == 1).sum()
        short_signals = (df['signal'] == -1).sum()
        print(f"  做多信号: {long_signals}, 做空信号: {short_signals}")
        
        # 运行回测
        backtest_result = self.run_backtest(df, params)
        trades = backtest_result['trades']
        
        # 计算总体统计
        overall_stats = self.calculate_statistics(trades)
        
        # 按年度分析
        yearly_stats = self.analyze_by_year(trades)
        
        # 显示结果
        print(f"  总交易数: {overall_stats['total_trades']}")
        print(f"  胜率: {overall_stats['win_rate']:.1f}%")
        print(f"  复利收益: {overall_stats['compound_return_pct']:.2f}%")
        print(f"  最大回撤: {overall_stats['max_drawdown_pct']:.2f}%")
        print(f"  盈亏比: {overall_stats['profit_factor']:.2f}")
        
        return {
            'symbol': symbol,
            'overall_stats': overall_stats,
            'yearly_stats': yearly_stats,
            'trades': trades,
            'signal_count': {'long': long_signals, 'short': short_signals}
        }
    
    def run_analysis(self, params: dict = None) -> Dict:
        """运行完整分析"""
        if params is None:
            params = {
                'ma_period': 192,
                'boll_period': 100,
                'boll_std': 2.0,
                'compression_threshold': 0.3,
                'min_compression_bars': 10,
                'hard_stop_pct': 2.0,
                'boll_stop_enabled': True
            }
        
        symbols = ['BTC', 'ETH', 'SOL', 'BNB', 'SUI', 'HYPE']
        
        print("=" * 80)
        print("方案C最终策略回测")
        print("=" * 80)
        print(f"参数: {params}")
        print("=" * 80)
        
        results = {}
        
        for symbol in symbols:
            try:
                result = self.analyze_symbol(symbol, params)
                results[symbol] = result
                print()
            except Exception as e:
                print(f"  {symbol} 分析失败: {e}")
                results[symbol] = {'error': str(e)}
                print()
        
        return results
    
    def generate_report(self, results: Dict, params: dict, output_dir: str):
        """生成详细报告"""
        report_file = os.path.join(output_dir, "strategy_c_final_backtest_report.md")
        
        with open(report_file, 'w', encoding='utf-8') as f:
            f.write("# 方案C最终策略回测报告\n\n")
            f.write(f"> 生成时间：{datetime.now().strftime('%Y-%m-%d %H:%M')}\n")
            f.write("> 数据：30m K线（真实数据）\n")
            f.write("> 策略：MA192 + BOLL(100,2.0) 压缩突破\n")
            f.write("> 未计手续费/滑点\n\n")
            
            # 策略参数
            f.write("## 一、策略参数\n\n")
            f.write("| 参数 | 值 | 说明 |\n")
            f.write("|------|-----|------|\n")
            for key, value in params.items():
                f.write(f"| {key} | {value} | - |\n")
            f.write("\n")
            
            # 全币种汇总
            f.write("## 二、全币种汇总\n\n")
            f.write("| 币种 | 交易数 | 胜率 | 简单收益 | 复利收益 | 最大回撤 | 盈亏比 |\n")
            f.write("|------|--------|------|----------|----------|----------|--------|\n")
            
            for symbol, result in results.items():
                if 'error' in result:
                    f.write(f"| {symbol} | 错误 | - | - | - | - | - |\n")
                else:
                    stats = result['overall_stats']
                    f.write(f"| {symbol} | {stats['total_trades']} | {stats['win_rate']:.1f}% | "
                           f"{stats['total_pnl_pct']:.2f}% | {stats['compound_return_pct']:.2f}% | "
                           f"{stats['max_drawdown_pct']:.2f}% | {stats['profit_factor']:.2f} |\n")
            
            f.write("\n")
            
            # 各币种年度详细分析
            f.write("## 三、各币种年度详细分析\n\n")
            
            for symbol, result in results.items():
                if 'error' in result:
                    f.write(f"### {symbol} - 错误\n\n")
                    f.write(f"错误信息: {result['error']}\n\n")
                    continue
                
                f.write(f"### {symbol}\n\n")
                
                # 信号统计
                signal_count = result['signal_count']
                f.write(f"**信号统计**: 做多 {signal_count['long']}，做空 {signal_count['short']}\n\n")
                
                # 年度统计表
                yearly_stats = result['yearly_stats']
                if yearly_stats:
                    f.write("| 年份 | 交易数 | 胜率 | 简单收益 | 复利收益 | 最大回撤 | 盈亏比 | 离场原因 |\n")
                    f.write("|------|--------|------|----------|----------|----------|--------|----------|\n")
                    
                    for year in sorted(yearly_stats.keys()):
                        stats = yearly_stats[year]
                        exit_reasons = stats['exit_reasons']
                        exit_reasons_str = ", ".join([f"{k}:{v}" for k, v in exit_reasons.items()])
                        
                        f.write(f"| {year} | {stats['total_trades']} | {stats['win_rate']:.1f}% | "
                               f"{stats['total_pnl_pct']:.2f}% | {stats['compound_return_pct']:.2f}% | "
                               f"{stats['max_drawdown_pct']:.2f}% | {stats['profit_factor']:.2f} | "
                               f"{exit_reasons_str} |\n")
                    
                    f.write("\n")
                
                # 关键发现
                f.write("**关键发现**:\n\n")
                
                # 找出最佳和最差年份
                if yearly_stats:
                    best_year = max(yearly_stats.items(), key=lambda x: x[1]['compound_return_pct'])
                    worst_year = min(yearly_stats.items(), key=lambda x: x[1]['compound_return_pct'])
                    
                    f.write(f"- 最佳年份: {best_year[0]} (复利 {best_year[1]['compound_return_pct']:.2f}%)\n")
                    f.write(f"- 最差年份: {worst_year[0]} (复利 {worst_year[1]['compound_return_pct']:.2f}%)\n")
                    
                    # 胜率分析
                    avg_win_rate = np.mean([s['win_rate'] for s in yearly_stats.values()])
                    f.write(f"- 平均年度胜率: {avg_win_rate:.1f}%\n")
                    
                    # 交易频率分析
                    total_years = len(yearly_stats)
                    total_trades = sum(s['total_trades'] for s in yearly_stats.values())
                    avg_trades_per_year = total_trades / total_years if total_years > 0 else 0
                    f.write(f"- 平均每年交易数: {avg_trades_per_year:.1f}\n")
                
                f.write("\n")
            
            # 总结与建议
            f.write("## 四、总结与建议\n\n")
            
            # 计算平均表现
            valid_results = {k: v for k, v in results.items() if 'error' not in v}
            if valid_results:
                avg_trades = np.mean([r['overall_stats']['total_trades'] for r in valid_results.values()])
                avg_win_rate = np.mean([r['overall_stats']['win_rate'] for r in valid_results.values()])
                avg_return = np.mean([r['overall_stats']['compound_return_pct'] for r in valid_results.values()])
                
                f.write("### 4.1 平均表现\n\n")
                f.write(f"- 平均交易数: {avg_trades:.1f}\n")
                f.write(f"- 平均胜率: {avg_win_rate:.1f}%\n")
                f.write(f"- 平均复利收益: {avg_return:.2f}%\n\n")
            
            f.write("### 4.2 策略特点\n\n")
            f.write("1. **入场逻辑**：BOLL压缩 + MA192穿越\n")
            f.write("2. **出场逻辑**：BOLL中轨止损\n")
            f.write("3. **压缩检测**：基于BOLL带宽百分比\n\n")
            
            f.write("### 4.3 下一步优化方向\n\n")
            f.write("1. **参数优化**：调整压缩阈值、最小压缩bars、止损参数\n")
            f.write("2. **止盈优化**：添加移动止盈或其他止盈方式\n")
            f.write("3. **压缩时间窗口**：优化压缩持续时间的判断\n")
            f.write("4. **多时间框架**：结合更高时间框架确认突破\n")
            f.write("5. **手续费测试**：评估手续费对策略的影响\n\n")
            
            f.write("## 五、相关文件\n\n")
            f.write("- 策略脚本: `src/strategy_c_final_backtest.py`\n")
            f.write("- 本报告: `src/feature_report/strategy_c_final_backtest_report.md`\n")
        
        print(f"报告已生成: {report_file}")

def main():
    """主函数"""
    # 数据目录
    data_dir = "F:\\rust-projects\\data_2026-08-13"
    
    # 初始化回测器
    backtester = FinalBacktester(data_dir)
    
    # 策略参数
    params = {
        'ma_period': 192,
        'boll_period': 100,
        'boll_std': 2.0,
        'compression_threshold': 0.3,
        'min_compression_bars': 10,
        'hard_stop_pct': 2.0,
        'boll_stop_enabled': True
    }
    
    # 运行分析
    results = backtester.run_analysis(params)
    
    # 保存结果
    output_dir = "F:\\rust-projects\\rust-trade\\trade-test\\src\\feature_report"
    os.makedirs(output_dir, exist_ok=True)
    
    # 生成报告
    backtester.generate_report(results, params, output_dir)

if __name__ == "__main__":
    main()