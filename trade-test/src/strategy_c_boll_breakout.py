#!/usr/bin/env python3
"""
方案C：BOLL压缩突破策略
策略逻辑：MA192与BOLL(100,2.0)中轨关系 + 穿越信号 + 压缩时间窗口
数据：30m K线
"""

import pandas as pd
import numpy as np
import json
import os
from datetime import datetime, timedelta
from typing import Dict, List, Tuple, Optional
import warnings
warnings.filterwarnings('ignore')

class BollingerBand:
    """布林带指标计算"""
    
    @staticmethod
    def calculate(df: pd.DataFrame, period: int = 100, std_dev: float = 2.0) -> pd.DataFrame:
        """
        计算布林带指标
        
        Args:
            df: 包含close列的DataFrame
            period: 布林带周期
            std_dev: 标准差倍数
            
        Returns:
            添加了BOLL指标的DataFrame
        """
        df = df.copy()
        
        # 计算中轨（SMA）
        df['boll_mid'] = df['close'].rolling(window=period).mean()
        
        # 计算标准差
        df['boll_std'] = df['close'].rolling(window=period).std()
        
        # 计算上轨和下轨
        df['boll_upper'] = df['boll_mid'] + (std_dev * df['boll_std'])
        df['boll_lower'] = df['boll_mid'] - (std_dev * df['boll_std'])
        
        # 计算带宽（标准化）
        df['boll_width'] = (df['boll_upper'] - df['boll_lower']) / df['boll_mid']
        
        # 计算带宽百分比（相对于历史）
        df['boll_width_pct'] = df['boll_width'].rolling(window=200).apply(
            lambda x: (x.iloc[-1] - x.min()) / (x.max() - x.min()) if x.max() != x.min() else 0.5
        )
        
        return df

class CompressionDetector:
    """BOLL压缩检测器"""
    
    def __init__(self, compression_threshold: float = 0.3, min_bars: int = 10):
        """
        初始化压缩检测器
        
        Args:
            compression_threshold: 压缩阈值（带宽百分比低于此值认为是压缩）
            min_bars: 最小压缩持续时间（bar数）
        """
        self.compression_threshold = compression_threshold
        self.min_bars = min_bars
    
    def detect_compression(self, boll_width_pct: pd.Series) -> pd.DataFrame:
        """
        检测BOLL压缩状态
        
        Args:
            boll_width_pct: BOLL带宽百分比序列
            
        Returns:
            包含压缩状态的DataFrame
        """
        df = pd.DataFrame()
        df['boll_width_pct'] = boll_width_pct
        
        # 标记压缩状态
        df['is_compressed'] = df['boll_width_pct'] < self.compression_threshold
        
        # 计算连续压缩的bar数
        compression_groups = (~df['is_compressed']).cumsum()
        df['compression_bars'] = df.groupby(compression_groups)['is_compressed'].cumsum()
        
        # 标记有效压缩（持续足够长时间）
        df['valid_compression'] = (df['is_compressed']) & (df['compression_bars'] >= self.min_bars)
        
        # 计算压缩强度（0-1，越低越压缩）
        df['compression_strength'] = 1 - df['boll_width_pct']
        
        # 计算压缩时间窗口（滑动窗口内的压缩程度）
        window_sizes = [5, 10, 20, 50]
        for window in window_sizes:
            df[f'compression_{window}'] = df['is_compressed'].rolling(window=window).mean()
        
        return df

class StrategyC:
    """方案C：BOLL压缩突破策略"""
    
    def __init__(self, 
                 ma_period: int = 192,
                 boll_period: int = 100,
                 boll_std: float = 2.0,
                 compression_threshold: float = 0.3,
                 min_compression_bars: int = 10,
                 hard_stop_pct: float = 2.0,
                 boll_stop_enabled: bool = True,
                 trailing_enabled: bool = False,
                 trailing_activate_pct: float = 4.0,
                 trailing_callback_pct: float = 1.0):
        """
        初始化策略参数
        
        Args:
            ma_period: MA周期
            boll_period: BOLL周期
            boll_std: BOLL标准差倍数
            compression_threshold: 压缩阈值
            min_compression_bars: 最小压缩持续时间
            hard_stop_pct: 硬止损百分比
            boll_stop_enabled: 是否启用BOLL中轨止损
            trailing_enabled: 是否启用移动止盈
            trailing_activate_pct: 移动止盈激活百分比
            trailing_callback_pct: 移动止盈回撤百分比
        """
        self.ma_period = ma_period
        self.boll_period = boll_period
        self.boll_std = boll_std
        self.compression_threshold = compression_threshold
        self.min_compression_bars = min_compression_bars
        self.hard_stop_pct = hard_stop_pct
        self.boll_stop_enabled = boll_stop_enabled
        self.trailing_enabled = trailing_enabled
        self.trailing_activate_pct = trailing_activate_pct
        self.trailing_callback_pct = trailing_callback_pct
        
        # 初始化指标计算器
        self.boll = BollingerBand()
        self.compression_detector = CompressionDetector(
            compression_threshold=compression_threshold,
            min_bars=min_compression_bars
        )
    
    def calculate_indicators(self, df: pd.DataFrame) -> pd.DataFrame:
        """
        计算所有技术指标
        
        Args:
            df: 原始K线数据
            
        Returns:
            添加了所有指标的DataFrame
        """
        df = df.copy()
        
        # 计算MA192
        df['ma192'] = df['close'].rolling(window=self.ma_period).mean()
        
        # 计算BOLL指标
        df = self.boll.calculate(df, period=self.boll_period, std_dev=self.boll_std)
        
        # 计算压缩状态
        compression_df = self.compression_detector.detect_compression(df['boll_width_pct'])
        df = pd.concat([df, compression_df], axis=1)
        
        # 计算MA192与BOLL中轨的关系
        df['ma_above_mid'] = df['ma192'] > df['boll_mid']
        df['ma_below_mid'] = df['ma192'] < df['boll_mid']
        
        # 计算穿越信号
        df['cross_above_ma'] = (df['close'] > df['ma192']) & (df['close'].shift(1) <= df['ma192'].shift(1))
        df['cross_below_ma'] = (df['close'] < df['ma192']) & (df['close'].shift(1) >= df['ma192'].shift(1))
        
        return df
    
    def generate_signals(self, df: pd.DataFrame) -> pd.DataFrame:
        """
        生成交易信号
        
        Args:
            df: 包含指标的DataFrame
            
        Returns:
            添加了信号的DataFrame
        """
        df = df.copy()
        
        # 初始化信号列
        df['signal'] = 0  # 0: 无信号, 1: 做多, -1: 做空
        df['signal_type'] = ''  # 信号类型描述
        
        # 做多信号：MA192 > BOLL中轨 AND 收盘价向上穿越MA192 AND 处于压缩状态
        long_condition = (
            df['ma_above_mid'] &  # MA192在中轨之上
            df['cross_above_ma'] &  # 收盘价向上穿越MA192
            df['valid_compression']  # 处于有效压缩状态
        )
        
        # 做空信号：MA192 < BOLL中轨 AND 收盘价向下穿越MA192 AND 处于压缩状态
        short_condition = (
            df['ma_below_mid'] &  # MA192在中轨之下
            df['cross_below_ma'] &  # 收盘价向下穿越MA192
            df['valid_compression']  # 处于有效压缩状态
        )
        
        # 生成信号
        df.loc[long_condition, 'signal'] = 1
        df.loc[long_condition, 'signal_type'] = 'long_breakout'
        
        df.loc[short_condition, 'signal'] = -1
        df.loc[short_condition, 'signal_type'] = 'short_breakout'
        
        return df
    
    def backtest(self, df: pd.DataFrame, initial_capital: float = 10000.0) -> Dict:
        """
        运行回测
        
        Args:
            df: 包含信号的DataFrame
            initial_capital: 初始资金
            
        Returns:
            回测结果字典
        """
        df = df.copy()
        
        # 初始化回测变量
        capital = initial_capital
        position = 0  # 当前持仓：0=空仓，1=多头，-1=空头
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
                if position == 1:  # 多头
                    current_profit_pct = (current_bar['close'] - entry_price) / entry_price * 100
                else:  # 空头
                    current_profit_pct = (entry_price - current_bar['close']) / entry_price * 100
                
                max_profit_pct = max(max_profit_pct, current_profit_pct)
                
                # 检查出场条件
                exit_reason = None
                exit_price = current_bar['close']
                
                # 1. 硬止损
                if current_profit_pct <= -self.hard_stop_pct:
                    exit_reason = 'hard_stop'
                
                # 2. BOLL中轨止损
                elif self.boll_stop_enabled:
                    if position == 1 and current_bar['close'] < current_bar['boll_mid']:
                        exit_reason = 'boll_mid_stop'
                    elif position == -1 and current_bar['close'] > current_bar['boll_mid']:
                        exit_reason = 'boll_mid_stop'
                
                # 3. 移动止盈（如果启用）
                elif self.trailing_enabled and max_profit_pct >= self.trailing_activate_pct:
                    # 计算从最高点的回撤
                    if position == 1:
                        drawdown = (max_profit_pct - current_profit_pct)
                    else:
                        drawdown = (max_profit_pct - current_profit_pct)
                    
                    if drawdown >= self.trailing_callback_pct:
                        exit_reason = 'trailing_stop'
                
                # 执行出场
                if exit_reason:
                    # 计算盈亏
                    if position == 1:
                        pnl_pct = (exit_price - entry_price) / entry_price * 100
                    else:
                        pnl_pct = (entry_price - exit_price) / entry_price * 100
                    
                    pnl_amount = capital * (pnl_pct / 100)
                    capital += pnl_amount
                    
                    # 记录交易
                    trade = {
                        'entry_time': entry_time,
                        'exit_time': current_bar.name if hasattr(current_bar, 'name') else i,
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
                    
                    # 重置持仓
                    position = 0
                    entry_price = 0.0
                    entry_time = None
                    max_profit_pct = 0.0
            
            # 如果没有持仓，检查入场信号
            if position == 0 and current_bar['signal'] != 0:
                signal = current_bar['signal']
                entry_price = current_bar['close']
                entry_time = current_bar.name if hasattr(current_bar, 'name') else i
                position = signal
                max_profit_pct = 0.0
        
        # 计算回测统计
        if not trades:
            return {
                'trades': [],
                'stats': {
                    'total_trades': 0,
                    'win_rate': 0,
                    'total_pnl_pct': 0,
                    'final_capital': initial_capital,
                    'max_drawdown_pct': 0,
                    'avg_trade_pnl_pct': 0,
                    'profit_factor': 0
                }
            }
        
        trades_df = pd.DataFrame(trades)
        
        # 计算统计指标
        total_trades = len(trades_df)
        winning_trades = len(trades_df[trades_df['pnl_pct'] > 0])
        losing_trades = len(trades_df[trades_df['pnl_pct'] <= 0])
        
        win_rate = winning_trades / total_trades * 100 if total_trades > 0 else 0
        
        total_pnl_pct = trades_df['pnl_pct'].sum()
        final_capital = capital
        
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
        
        # 计算复利收益
        compound_return = (final_capital - initial_capital) / initial_capital * 100
        
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

def load_data(file_path: str) -> pd.DataFrame:
    """
    加载K线数据
    
    Args:
        file_path: CSV文件路径
        
    Returns:
        DataFrame
    """
    if not os.path.exists(file_path):
        raise FileNotFoundError(f"数据文件不存在: {file_path}")
    
    df = pd.read_csv(file_path)
    
    # 确保时间列正确
    if 'open_time' in df.columns:
        df['open_time'] = pd.to_datetime(df['open_time'])
        df.set_index('open_time', inplace=True)
    elif 'timestamp' in df.columns:
        df['timestamp'] = pd.to_datetime(df['timestamp'])
        df.set_index('timestamp', inplace=True)
    
    # 确保数值列正确
    numeric_cols = ['open', 'high', 'low', 'close', 'volume']
    for col in numeric_cols:
        if col in df.columns:
            df[col] = pd.to_numeric(df[col], errors='coerce')
    
    return df

def run_strategy_c_backtest(data_dir: str, 
                           symbol: str,
                           ma_period: int = 192,
                           boll_period: int = 100,
                           boll_std: float = 2.0,
                           compression_threshold: float = 0.3,
                           min_compression_bars: int = 10,
                           hard_stop_pct: float = 2.0) -> Dict:
    """
    运行方案C策略回测
    
    Args:
        data_dir: 数据目录
        symbol: 交易对符号
        ma_period: MA周期
        boll_period: BOLL周期
        boll_std: BOLL标准差倍数
        compression_threshold: 压缩阈值
        min_compression_bars: 最小压缩持续时间
        hard_stop_pct: 硬止损百分比
        
    Returns:
        回测结果字典
    """
    # 加载数据
    file_path = os.path.join(data_dir, f'kline_30m_{symbol}.csv')
    if not os.path.exists(file_path):
        raise FileNotFoundError(f"数据文件不存在: {file_path}")
    
    df = load_data(file_path)
    print(f"加载 {symbol} 数据: {len(df)} 根K线")
    
    # 初始化策略
    strategy = StrategyC(
        ma_period=ma_period,
        boll_period=boll_period,
        boll_std=boll_std,
        compression_threshold=compression_threshold,
        min_compression_bars=min_compression_bars,
        hard_stop_pct=hard_stop_pct,
        boll_stop_enabled=True,
        trailing_enabled=False
    )
    
    # 计算指标
    df = strategy.calculate_indicators(df)
    
    # 生成信号
    df = strategy.generate_signals(df)
    
    # 运行回测
    result = strategy.backtest(df)
    
    return result

def main():
    """主函数"""
    # 数据目录（使用示例数据）
    data_dir = "F:\\rust-projects\\rust-trade\\trade-test\\src\\sample_data"
    
    # 测试币种
    symbols = ['BTC', 'ETH', 'SOL', 'BNB', 'SUI', 'HYPE']
    
    # 策略参数
    params = {
        'ma_period': 192,
        'boll_period': 100,
        'boll_std': 2.0,
        'compression_threshold': 0.3,
        'min_compression_bars': 10,
        'hard_stop_pct': 2.0
    }
    
    print("=" * 80)
    print("方案C：BOLL压缩突破策略 - 初步回测")
    print("=" * 80)
    print(f"参数: {params}")
    print("=" * 80)
    
    results = {}
    
    for symbol in symbols:
        try:
            print(f"\n回测 {symbol}...")
            result = run_strategy_c_backtest(data_dir, symbol, **params)
            results[symbol] = result
            
            stats = result['stats']
            print(f"{symbol}: 交易数={stats['total_trades']}, 胜率={stats['win_rate']:.1f}%, "
                  f"总收益={stats['total_pnl_pct']:.2f}%, 复利={stats['compound_return_pct']:.2f}%, "
                  f"最大回撤={stats['max_drawdown_pct']:.2f}%")
            
        except Exception as e:
            print(f"{symbol} 回测失败: {e}")
            results[symbol] = {'error': str(e)}
    
    # 保存结果
    output_dir = "F:\\rust-projects\\rust-trade\\trade-test\\src\\feature_report"
    os.makedirs(output_dir, exist_ok=True)
    
    output_file = os.path.join(output_dir, "strategy_c_initial_results.json")
    with open(output_file, 'w', encoding='utf-8') as f:
        # 将结果转换为可序列化的格式
        serializable_results = {}
        for symbol, result in results.items():
            if 'error' in result:
                serializable_results[symbol] = result
            else:
                serializable_result = {
                    'stats': result['stats'],
                    'trades_count': len(result['trades'])
                }
                serializable_results[symbol] = serializable_result
        
        json.dump(serializable_results, f, indent=2, ensure_ascii=False)
    
    print(f"\n结果已保存到: {output_file}")
    
    # 生成报告
    generate_report(results, params, output_dir)

def generate_report(results: Dict, params: Dict, output_dir: str):
    """
    生成策略研究报告
    
    Args:
        results: 回测结果
        params: 策略参数
        output_dir: 输出目录
    """
    report_file = os.path.join(output_dir, "strategy_c_initial_report.md")
    
    with open(report_file, 'w', encoding='utf-8') as f:
        f.write("# 方案C：BOLL压缩突破策略 - 初步回测报告\n\n")
        f.write(f"> 生成时间：{datetime.now().strftime('%Y-%m-%d %H:%M')}\n")
        f.write("> 数据：30m K线\n")
        f.write("> 策略：MA192 + BOLL(100,2.0) 压缩突破\n")
        f.write("> 未计手续费/滑点\n\n")
        
        f.write("## 一、策略参数\n\n")
        f.write("| 参数 | 值 | 说明 |\n")
        f.write("|------|-----|------|\n")
        for key, value in params.items():
            f.write(f"| {key} | {value} | - |\n")
        f.write("\n")
        
        f.write("## 二、全币种汇总\n\n")
        f.write("| 币种 | 交易数 | 胜率 | 总收益 | 复利收益 | 最大回撤 | 盈亏比 | 平均压缩bars |\n")
        f.write("|------|--------|------|--------|----------|----------|--------|--------------|\n")
        
        for symbol, result in results.items():
            if 'error' in result:
                f.write(f"| {symbol} | 错误 | - | - | - | - | - | - |\n")
            else:
                stats = result['stats']
                f.write(f"| {symbol} | {stats['total_trades']} | {stats['win_rate']:.1f}% | "
                       f"{stats['total_pnl_pct']:.2f}% | {stats['compound_return_pct']:.2f}% | "
                       f"{stats['max_drawdown_pct']:.2f}% | {stats['profit_factor']:.2f} | "
                       f"{stats['avg_compression_bars']:.1f} |\n")
        
        f.write("\n## 三、初步分析\n\n")
        
        # 计算平均表现
        valid_results = {k: v for k, v in results.items() if 'error' not in v}
        if valid_results:
            avg_trades = np.mean([r['stats']['total_trades'] for r in valid_results.values()])
            avg_win_rate = np.mean([r['stats']['win_rate'] for r in valid_results.values()])
            avg_return = np.mean([r['stats']['compound_return_pct'] for r in valid_results.values()])
            
            f.write(f"### 3.1 平均表现\n\n")
            f.write(f"- 平均交易数: {avg_trades:.1f}\n")
            f.write(f"- 平均胜率: {avg_win_rate:.1f}%\n")
            f.write(f"- 平均复利收益: {avg_return:.2f}%\n\n")
        
        f.write("### 3.2 策略特点\n\n")
        f.write("1. **入场逻辑**：BOLL压缩 + MA192穿越\n")
        f.write("2. **出场逻辑**：BOLL中轨止损\n")
        f.write("3. **压缩检测**：基于BOLL带宽百分比\n\n")
        
        f.write("## 四、下一步优化方向\n\n")
        f.write("1. **参数优化**：调整压缩阈值、最小压缩bars、止损参数\n")
        f.write("2. **止盈优化**：添加移动止盈或其他止盈方式\n")
        f.write("3. **压缩时间窗口**：优化压缩持续时间的判断\n")
        f.write("4. **多时间框架**：结合更高时间框架确认突破\n")
        f.write("5. **手续费测试**：评估手续费对策略的影响\n\n")
        
        f.write("## 五、相关文件\n\n")
        f.write("- 策略脚本: `src/strategy_c_boll_breakout.py`\n")
        f.write("- 结果文件: `src/feature_report/strategy_c_initial_results.json`\n")
        f.write("- 本报告: `src/feature_report/strategy_c_initial_report.md`\n")
    
    print(f"报告已生成: {report_file}")

if __name__ == "__main__":
    main()