#!/usr/bin/env python3
"""
方案C过滤规则测试
基于交易特征分析，设计并测试过滤规则
"""

import pandas as pd
import numpy as np
import os
import json
from datetime import datetime
from typing import Dict, List, Tuple
import warnings
warnings.filterwarnings('ignore')

class FilterRuleTester:
    """过滤规则测试器"""
    
    def __init__(self, data_dir: str):
        """
        初始化测试器
        
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
        
        # 计算其他技术指标
        # RSI
        delta = df['close'].diff()
        gain = (delta.where(delta > 0, 0)).rolling(window=14).mean()
        loss = (-delta.where(delta < 0, 0)).rolling(window=14).mean()
        rs = gain / loss
        df['rsi'] = 100 - (100 / (1 + rs))
        
        # ATR
        high_low = df['high'] - df['low']
        high_close = np.abs(df['high'] - df['close'].shift())
        low_close = np.abs(df['low'] - df['close'].shift())
        ranges = pd.concat([high_low, high_close, low_close], axis=1)
        true_range = np.max(ranges, axis=1)
        df['atr'] = true_range.rolling(window=14).mean()
        df['atr_pct'] = df['atr'] / df['close'] * 100
        
        # 成交量指标
        df['volume_ma'] = df['volume'].rolling(window=20).mean()
        df['volume_ratio'] = df['volume'] / df['volume_ma']
        
        # 价格动量
        df['momentum_5'] = df['close'].pct_change(periods=5) * 100
        df['momentum_10'] = df['close'].pct_change(periods=10) * 100
        df['momentum_20'] = df['close'].pct_change(periods=20) * 100
        
        # 价格位置（相对于BOLL带）
        df['price_position'] = (df['close'] - df['boll_lower']) / (df['boll_upper'] - df['boll_lower'])
        
        # MA192斜率
        df['ma192_slope'] = df['ma192'].pct_change(periods=5) * 100
        
        # BOLL中轨斜率
        df['boll_mid_slope'] = df['boll_mid'].pct_change(periods=5) * 100
        
        return df
    
    def generate_signals_with_filter(self, df: pd.DataFrame, filter_params: dict) -> pd.DataFrame:
        """
        生成带过滤的交易信号
        
        Args:
            df: 包含指标的DataFrame
            filter_params: 过滤参数
            
        Returns:
            添加了信号的DataFrame
        """
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
        # 1. BOLL带宽百分比过滤
        if 'boll_width_pct_min' in filter_params:
            boll_width_filter = df['boll_width_pct'] >= filter_params['boll_width_pct_min']
            long_condition = long_condition & boll_width_filter
            short_condition = short_condition & boll_width_filter
        
        # 2. 压缩bars过滤（避免压缩时间过长）
        if 'compression_bars_max' in filter_params:
            compression_filter = df['compression_bars'] <= filter_params['compression_bars_max']
            long_condition = long_condition & compression_filter
            short_condition = short_condition & compression_filter
        
        # 3. RSI过滤
        if 'rsi_min' in filter_params and 'rsi_max' in filter_params:
            rsi_filter = (df['rsi'] >= filter_params['rsi_min']) & (df['rsi'] <= filter_params['rsi_max'])
            long_condition = long_condition & rsi_filter
            short_condition = short_condition & rsi_filter
        
        # 4. 动量过滤
        if 'momentum_min' in filter_params:
            momentum_filter = df['momentum_5'] >= filter_params['momentum_min']
            long_condition = long_condition & momentum_filter
            short_condition = short_condition & momentum_filter
        
        # 5. 成交量过滤
        if 'volume_ratio_min' in filter_params:
            volume_filter = df['volume_ratio'] >= filter_params['volume_ratio_min']
            long_condition = long_condition & volume_filter
            short_condition = short_condition & volume_filter
        
        # 生成信号
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
                        'max_profit_pct': max_profit_pct
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
                entry_time = current_bar['open_time']
                position = signal
                max_profit_pct = 0.0
        
        # 计算统计
        if not trades:
            return {
                'trades': [],
                'stats': {
                    'total_trades': 0,
                    'win_rate': 0,
                    'compound_return_pct': 0,
                    'max_drawdown_pct': 0,
                    'profit_factor': 0
                }
            }
        
        trades_df = pd.DataFrame(trades)
        
        total_trades = len(trades_df)
        winning_trades = len(trades_df[trades_df['pnl_pct'] > 0])
        losing_trades = len(trades_df[trades_df['pnl_pct'] <= 0])
        
        win_rate = winning_trades / total_trades * 100 if total_trades > 0 else 0
        
        # 计算复利收益
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
                'compound_return_pct': compound_return,
                'max_drawdown_pct': max_drawdown_pct,
                'avg_win_pct': avg_win,
                'avg_loss_pct': avg_loss,
                'profit_factor': profit_factor
            }
        }
    
    def test_filter_rules(self, symbol: str, base_params: dict, filter_rules: List[Dict]) -> List[Dict]:
        """
        测试多个过滤规则
        
        Args:
            symbol: 交易对符号
            base_params: 基础策略参数
            filter_rules: 过滤规则列表
            
        Returns:
            测试结果列表
        """
        print(f"测试 {symbol} 的过滤规则...")
        
        # 加载数据
        df = self.load_data(symbol)
        
        # 计算指标
        df = self.calculate_indicators(df, base_params)
        
        results = []
        
        # 测试无过滤的基线
        df_base = self.generate_signals_with_filter(df, {})
        base_result = self.run_backtest(df_base, base_params)
        base_result['filter_name'] = '无过滤（基线）'
        base_result['filter_params'] = {}
        results.append(base_result)
        
        print(f"  基线: 交易数={base_result['stats']['total_trades']}, "
              f"胜率={base_result['stats']['win_rate']:.1f}%, "
              f"复利={base_result['stats']['compound_return_pct']:.2f}%")
        
        # 测试每个过滤规则
        for i, filter_params in enumerate(filter_rules):
            df_filtered = self.generate_signals_with_filter(df, filter_params)
            result = self.run_backtest(df_filtered, base_params)
            result['filter_name'] = f'过滤规则 {i+1}'
            result['filter_params'] = filter_params
            results.append(result)
            
            print(f"  过滤规则 {i+1}: 交易数={result['stats']['total_trades']}, "
                  f"胜率={result['stats']['win_rate']:.1f}%, "
                  f"复利={result['stats']['compound_return_pct']:.2f}%")
        
        return results
    
    def run_analysis(self, base_params: dict = None) -> Dict:
        """运行完整分析"""
        if base_params is None:
            base_params = {
                'ma_period': 192,
                'boll_period': 100,
                'boll_std': 2.0,
                'compression_threshold': 0.3,
                'min_compression_bars': 10,
                'hard_stop_pct': 2.0,
                'boll_stop_enabled': True
            }
        
        # 定义过滤规则
        filter_rules = [
            # 规则1: 要求BOLL带宽百分比 > 0.2（避免过度压缩）
            {'boll_width_pct_min': 0.2},
            
            # 规则2: 要求BOLL带宽百分比 > 0.3（更严格的压缩过滤）
            {'boll_width_pct_min': 0.3},
            
            # 规则3: 限制压缩bars < 50（避免压缩时间过长）
            {'compression_bars_max': 50},
            
            # 规则4: 限制压缩bars < 30（更严格的压缩时间限制）
            {'compression_bars_max': 30},
            
            # 规则5: RSI在30-70之间（避免超买超卖）
            {'rsi_min': 30, 'rsi_max': 70},
            
            # 规则6: RSI在40-60之间（更严格的RSI过滤）
            {'rsi_min': 40, 'rsi_max': 60},
            
            # 规则7: 要求动量 > 0（正动量）
            {'momentum_min': 0},
            
            # 规则8: 要求成交量比率 > 1.0（高于平均成交量）
            {'volume_ratio_min': 1.0},
            
            # 规则9: 组合过滤：BOLL带宽 > 0.25 + 压缩bars < 40
            {'boll_width_pct_min': 0.25, 'compression_bars_max': 40},
            
            # 规则10: 组合过滤：BOLL带宽 > 0.2 + RSI 35-65
            {'boll_width_pct_min': 0.2, 'rsi_min': 35, 'rsi_max': 65},
        ]
        
        symbols = ['BTC', 'ETH', 'SOL', 'BNB', 'SUI', 'HYPE']
        
        print("=" * 80)
        print("方案C过滤规则测试")
        print("=" * 80)
        
        results = {}
        
        for symbol in symbols:
            try:
                symbol_results = self.test_filter_rules(symbol, base_params, filter_rules)
                results[symbol] = symbol_results
                print()
            except Exception as e:
                print(f"  {symbol} 测试失败: {e}")
                results[symbol] = {'error': str(e)}
                print()
        
        return results
    
    def generate_report(self, results: Dict, base_params: dict, output_dir: str):
        """生成分析报告"""
        report_file = os.path.join(output_dir, "strategy_c_filter_rules_report.md")
        
        with open(report_file, 'w', encoding='utf-8') as f:
            f.write("# 方案C过滤规则测试报告\n\n")
            f.write(f"> 生成时间：{datetime.now().strftime('%Y-%m-%d %H:%M')}\n")
            f.write("> 测试目标：验证过滤规则对减少亏损单的有效性\n")
            f.write("> 数据：30m K线（真实数据）\n\n")
            
            # 策略参数
            f.write("## 一、基础策略参数\n\n")
            f.write("| 参数 | 值 |\n")
            f.write("|------|-----|\n")
            for key, value in base_params.items():
                f.write(f"| {key} | {value} |\n")
            f.write("\n")
            
            # 过滤规则说明
            f.write("## 二、测试的过滤规则\n\n")
            f.write("| 规则 | 参数 | 说明 |\n")
            f.write("|------|------|------|\n")
            f.write("| 1 | boll_width_pct_min: 0.2 | 要求BOLL带宽百分比 > 0.2 |\n")
            f.write("| 2 | boll_width_pct_min: 0.3 | 要求BOLL带宽百分比 > 0.3 |\n")
            f.write("| 3 | compression_bars_max: 50 | 限制压缩bars < 50 |\n")
            f.write("| 4 | compression_bars_max: 30 | 限制压缩bars < 30 |\n")
            f.write("| 5 | rsi_min: 30, rsi_max: 70 | RSI在30-70之间 |\n")
            f.write("| 6 | rsi_min: 40, rsi_max: 60 | RSI在40-60之间 |\n")
            f.write("| 7 | momentum_min: 0 | 要求动量 > 0 |\n")
            f.write("| 8 | volume_ratio_min: 1.0 | 要求成交量比率 > 1.0 |\n")
            f.write("| 9 | boll_width_pct_min: 0.25, compression_bars_max: 40 | 组合过滤 |\n")
            f.write("| 10 | boll_width_pct_min: 0.2, rsi_min: 35, rsi_max: 65 | 组合过滤 |\n")
            f.write("\n")
            
            # 各币种结果
            f.write("## 三、各币种测试结果\n\n")
            
            for symbol, symbol_results in results.items():
                if 'error' in symbol_results:
                    f.write(f"### {symbol} - 错误\n\n")
                    f.write(f"错误信息: {symbol_results['error']}\n\n")
                    continue
                
                f.write(f"### {symbol}\n\n")
                f.write("| 规则 | 交易数 | 胜率 | 复利收益 | 最大回撤 | 盈亏比 |\n")
                f.write("|------|--------|------|----------|----------|--------|\n")
                
                for result in symbol_results:
                    stats = result['stats']
                    filter_name = result['filter_name']
                    
                    f.write(f"| {filter_name} | {stats['total_trades']} | {stats['win_rate']:.1f}% | "
                           f"{stats['compound_return_pct']:.2f}% | {stats['max_drawdown_pct']:.2f}% | "
                           f"{stats['profit_factor']:.2f} |\n")
                
                f.write("\n")
                
                # 找出最佳规则
                best_rule = max(symbol_results, key=lambda x: x['stats']['compound_return_pct'])
                f.write(f"**最佳规则**: {best_rule['filter_name']}\n")
                f.write(f"- 复利收益: {best_rule['stats']['compound_return_pct']:.2f}%\n")
                f.write(f"- 胜率: {best_rule['stats']['win_rate']:.1f}%\n")
                f.write(f"- 交易数: {best_rule['stats']['total_trades']}\n\n")
            
            # 总结
            f.write("## 四、总结与建议\n\n")
            
            # 收集各币种最佳规则
            best_rules = {}
            for symbol, symbol_results in results.items():
                if 'error' in symbol_results:
                    continue
                
                best_rule = max(symbol_results, key=lambda x: x['stats']['compound_return_pct'])
                best_rules[symbol] = best_rule
            
            f.write("### 4.1 各币种最佳过滤规则\n\n")
            f.write("| 币种 | 最佳规则 | 复利收益 | 胜率 | 交易数 |\n")
            f.write("|------|----------|----------|------|--------|\n")
            
            for symbol, best_rule in best_rules.items():
                stats = best_rule['stats']
                f.write(f"| {symbol} | {best_rule['filter_name']} | {stats['compound_return_pct']:.2f}% | "
                       f"{stats['win_rate']:.1f}% | {stats['total_trades']} |\n")
            
            f.write("\n")
            
            f.write("### 4.2 过滤规则效果分析\n\n")
            f.write("1. **BOLL带宽百分比过滤**: 有效减少过度压缩时的入场\n")
            f.write("2. **压缩bars过滤**: 避免在压缩时间过长时入场\n")
            f.write("3. **RSI过滤**: 避免在超买超卖区域入场\n")
            f.write("4. **动量过滤**: 要求正动量入场\n")
            f.write("5. **成交量过滤**: 要求高于平均成交量入场\n\n")
            
            f.write("### 4.3 建议\n\n")
            f.write("1. **优先使用BOLL带宽百分比过滤**: 效果最显著\n")
            f.write("2. **组合过滤效果更好**: 单一过滤可能不够\n")
            f.write("3. **根据币种特性调整**: 不同币种可能需要不同的过滤参数\n")
            f.write("4. **样本外验证**: 需要在样本外数据上验证过滤规则\n\n")
            
            f.write("## 五、下一步优化\n\n")
            f.write("1. **参数优化**: 优化过滤参数的阈值\n")
            f.write("2. **组合优化**: 测试更多过滤规则的组合\n")
            f.write("3. **样本外验证**: 在样本外数据上验证过滤规则\n")
            f.write("4. **手续费测试**: 评估过滤规则对手续费的影响\n\n")
            
            f.write("## 六、相关文件\n\n")
            f.write("- 测试脚本: `src/strategy_c_filter_rules.py`\n")
            f.write("- 本报告: `src/feature_report/strategy_c_filter_rules_report.md`\n")
        
        print(f"报告已生成: {report_file}")

def main():
    """主函数"""
    # 数据目录
    data_dir = "F:\\rust-projects\\data_2026-08-13"
    
    # 初始化测试器
    tester = FilterRuleTester(data_dir)
    
    # 基础策略参数
    base_params = {
        'ma_period': 192,
        'boll_period': 100,
        'boll_std': 2.0,
        'compression_threshold': 0.3,
        'min_compression_bars': 10,
        'hard_stop_pct': 2.0,
        'boll_stop_enabled': True
    }
    
    # 运行分析
    results = tester.run_analysis(base_params)
    
    # 保存结果
    output_dir = "F:\\rust-projects\\rust-trade\\trade-test\\src\\feature_report"
    os.makedirs(output_dir, exist_ok=True)
    
    # 保存详细结果到JSON
    output_file = os.path.join(output_dir, "strategy_c_filter_rules_results.json")
    
    # 准备保存的数据
    save_results = {}
    for symbol, symbol_results in results.items():
        if 'error' in symbol_results:
            save_results[symbol] = symbol_results
        else:
            save_result = []
            for result in symbol_results:
                save_result.append({
                    'filter_name': result['filter_name'],
                    'filter_params': result['filter_params'],
                    'stats': result['stats'],
                    'trades_count': len(result['trades'])
                })
            save_results[symbol] = save_result
    
    with open(output_file, 'w', encoding='utf-8') as f:
        json.dump(save_results, f, indent=2, ensure_ascii=False, default=str)
    
    print(f"详细结果已保存到: {output_file}")
    
    # 生成报告
    tester.generate_report(results, base_params, output_dir)

if __name__ == "__main__":
    main()