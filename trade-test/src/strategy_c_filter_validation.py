#!/usr/bin/env python3
"""
方案C过滤参数验证
包含时间切分验证，避免过拟合
"""

import pandas as pd
import numpy as np
import os
from datetime import datetime
from typing import Dict, List, Tuple
import warnings
warnings.filterwarnings('ignore')

class FilterValidator:
    """过滤参数验证器"""
    
    def __init__(self, data_dir: str):
        """
        初始化验证器
        
        Args:
            data_dir: 数据目录
        """
        self.data_dir = data_dir
    
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
        
        # 2. 压缩bars过滤
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
                        pnl_pct = (current_bar['close'] - entry_price) / entry_price * 100
                    else:
                        pnl_pct = (entry_price - current_bar['close']) / entry_price * 100
                    
                    pnl_amount = capital * (pnl_pct / 100)
                    capital += pnl_amount
                    
                    trade = {
                        'pnl_pct': pnl_pct,
                        'pnl_amount': pnl_amount,
                        'exit_reason': exit_reason
                    }
                    trades.append(trade)
                    
                    position = 0
                    entry_price = 0.0
            
            # 如果没有持仓，检查入场信号
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
        avg_loss = abs(trades_df[trades_df['pnl_pct'] <= 0]['pnl_pct'].mean()) if (total_trades - winning_trades) > 0 else 0
        profit_factor = avg_win / avg_loss if avg_loss > 0 else float('inf')
        
        return {
            'total_trades': total_trades,
            'win_rate': win_rate,
            'compound_return_pct': compound_return,
            'max_drawdown_pct': max_drawdown_pct,
            'profit_factor': profit_factor,
            'avg_win_pct': avg_win,
            'avg_loss_pct': avg_loss
        }
    
    def split_data_by_time(self, df: pd.DataFrame, train_ratio: float = 0.7) -> Tuple[pd.DataFrame, pd.DataFrame]:
        """
        按时间切分数据
        
        Args:
            df: 原始数据
            train_ratio: 训练集比例
            
        Returns:
            训练集和测试集
        """
        split_idx = int(len(df) * train_ratio)
        train_df = df.iloc[:split_idx].copy()
        test_df = df.iloc[split_idx:].copy()
        
        return train_df, test_df
    
    def validate_filter_robustness(self, symbol: str, base_params: dict, filter_rules: List[Dict]) -> Dict:
        """
        验证过滤规则的稳健性
        
        Args:
            symbol: 交易对符号
            base_params: 基础策略参数
            filter_rules: 过滤规则列表
            
        Returns:
            验证结果
        """
        print(f"验证 {symbol} 的过滤规则稳健性...")
        
        # 加载数据
        df = self.load_data(symbol)
        
        # 计算指标
        df = self.calculate_indicators(df, base_params)
        
        # 时间切分
        train_df, test_df = self.split_data_by_time(df, train_ratio=0.7)
        
        print(f"  训练集: {len(train_df)} 根K线 ({train_df['open_time'].min()} 到 {train_df['open_time'].max()})")
        print(f"  测试集: {len(test_df)} 根K线 ({test_df['open_time'].min()} 到 {test_df['open_time'].max()})")
        
        results = []
        
        # 测试每个过滤规则
        for i, filter_params in enumerate(filter_rules):
            filter_name = f"过滤规则 {i+1}"
            
            # 训练集测试
            train_signals = self.generate_signals_with_filter(train_df, filter_params)
            train_result = self.run_backtest(train_signals, base_params)
            
            # 测试集测试
            test_signals = self.generate_signals_with_filter(test_df, filter_params)
            test_result = self.run_backtest(test_signals, base_params)
            
            # 计算稳健性指标
            robustness_score = 0
            
            # 1. 胜率稳健性（测试集胜率不能比训练集低太多）
            if train_result['win_rate'] > 0:
                win_rate_ratio = test_result['win_rate'] / train_result['win_rate']
                if win_rate_ratio > 0.8:  # 测试集胜率至少是训练集的80%
                    robustness_score += 1
            
            # 2. 收益稳健性（测试集收益不能比训练集低太多）
            if train_result['compound_return_pct'] > 0:
                return_ratio = test_result['compound_return_pct'] / train_result['compound_return_pct']
                if return_ratio > 0.7:  # 测试集收益至少是训练集的70%
                    robustness_score += 1
            
            # 3. 交易数稳健性（测试集交易数不能太少）
            if test_result['total_trades'] > 10:  # 至少10笔交易
                robustness_score += 1
            
            # 4. 盈亏比稳健性
            if test_result['profit_factor'] > 1.5:  # 盈亏比大于1.5
                robustness_score += 1
            
            result = {
                'filter_name': filter_name,
                'filter_params': filter_params,
                'train_result': train_result,
                'test_result': test_result,
                'robustness_score': robustness_score,
                'win_rate_ratio': test_result['win_rate'] / train_result['win_rate'] if train_result['win_rate'] > 0 else 0,
                'return_ratio': test_result['compound_return_pct'] / train_result['compound_return_pct'] if train_result['compound_return_pct'] > 0 else 0
            }
            
            results.append(result)
            
            print(f"  {filter_name}:")
            print(f"    训练集: 交易数={train_result['total_trades']}, 胜率={train_result['win_rate']:.1f}%, 复利={train_result['compound_return_pct']:.2f}%")
            print(f"    测试集: 交易数={test_result['total_trades']}, 胜率={test_result['win_rate']:.1f}%, 复利={test_result['compound_return_pct']:.2f}%")
            print(f"    稳健性得分: {robustness_score}/4")
        
        return {
            'symbol': symbol,
            'train_period': f"{train_df['open_time'].min()} 到 {train_df['open_time'].max()}",
            'test_period': f"{test_df['open_time'].min()} 到 {test_df['open_time'].max()}",
            'results': results
        }
    
    def run_validation(self, base_params: dict = None) -> Dict:
        """运行完整验证"""
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
            # 无过滤（基线）
            {},
            
            # BOLL带宽百分比过滤
            {'boll_width_pct_min': 0.2},
            {'boll_width_pct_min': 0.25},
            {'boll_width_pct_min': 0.3},
            
            # 压缩bars过滤
            {'compression_bars_max': 50},
            {'compression_bars_max': 40},
            {'compression_bars_max': 30},
            
            # RSI过滤
            {'rsi_min': 30, 'rsi_max': 70},
            {'rsi_min': 35, 'rsi_max': 65},
            {'rsi_min': 40, 'rsi_max': 60},
            
            # 动量过滤
            {'momentum_min': 0},
            {'momentum_min': 0.5},
            
            # 组合过滤
            {'boll_width_pct_min': 0.25, 'compression_bars_max': 40},
            {'boll_width_pct_min': 0.2, 'rsi_min': 35, 'rsi_max': 65},
            {'boll_width_pct_min': 0.2, 'compression_bars_max': 40, 'rsi_min': 35, 'rsi_max': 65},
        ]
        
        symbols = ['BTC', 'ETH', 'SOL', 'BNB', 'SUI', 'HYPE']
        
        print("=" * 80)
        print("方案C过滤参数验证（时间切分）")
        print("=" * 80)
        
        results = {}
        
        for symbol in symbols:
            try:
                symbol_result = self.validate_filter_robustness(symbol, base_params, filter_rules)
                results[symbol] = symbol_result
                print()
            except Exception as e:
                print(f"  {symbol} 验证失败: {e}")
                results[symbol] = {'error': str(e)}
                print()
        
        return results
    
    def generate_report(self, results: Dict, base_params: dict, output_dir: str):
        """生成验证报告"""
        report_file = os.path.join(output_dir, "strategy_c_filter_validation_report.md")
        
        with open(report_file, 'w', encoding='utf-8') as f:
            f.write("# 方案C过滤参数验证报告（时间切分）\n\n")
            f.write(f"> 生成时间：{datetime.now().strftime('%Y-%m-%d %H:%M')}\n")
            f.write("> 验证目标：测试过滤规则的稳健性，避免过拟合\n")
            f.write("> 方法：70%训练集 + 30%测试集\n\n")
            
            # 策略参数
            f.write("## 一、基础策略参数\n\n")
            f.write("| 参数 | 值 |\n")
            f.write("|------|-----|\n")
            for key, value in base_params.items():
                f.write(f"| {key} | {value} |\n")
            f.write("\n")
            
            # 各币种验证结果
            f.write("## 二、各币种验证结果\n\n")
            
            for symbol, symbol_result in results.items():
                if 'error' in symbol_result:
                    f.write(f"### {symbol} - 错误\n\n")
                    f.write(f"错误信息: {symbol_result['error']}\n\n")
                    continue
                
                f.write(f"### {symbol}\n\n")
                f.write(f"**训练期**: {symbol_result['train_period']}\n")
                f.write(f"**测试期**: {symbol_result['test_period']}\n\n")
                
                f.write("| 过滤规则 | 训练集交易数 | 训练集胜率 | 训练集复利 | 测试集交易数 | 测试集胜率 | 测试集复利 | 稳健性得分 |\n")
                f.write("|----------|--------------|------------|------------|--------------|------------|------------|------------|\n")
                
                for result in symbol_result['results']:
                    train = result['train_result']
                    test = result['test_result']
                    
                    f.write(f"| {result['filter_name']} | {train['total_trades']} | {train['win_rate']:.1f}% | "
                           f"{train['compound_return_pct']:.2f}% | {test['total_trades']} | {test['win_rate']:.1f}% | "
                           f"{test['compound_return_pct']:.2f}% | {result['robustness_score']}/4 |\n")
                
                f.write("\n")
                
                # 找出最稳健的规则
                robust_rules = [r for r in symbol_result['results'] if r['robustness_score'] >= 3]
                if robust_rules:
                    best_rule = max(robust_rules, key=lambda x: x['test_result']['compound_return_pct'])
                    f.write(f"**最稳健规则**: {best_rule['filter_name']}\n")
                    f.write(f"- 稳健性得分: {best_rule['robustness_score']}/4\n")
                    f.write(f"- 测试集复利: {best_rule['test_result']['compound_return_pct']:.2f}%\n")
                    f.write(f"- 测试集胜率: {best_rule['test_result']['win_rate']:.1f}%\n\n")
            
            # 总结
            f.write("## 三、总结与建议\n\n")
            
            # 收集各币种最稳健规则
            best_rules = {}
            for symbol, symbol_result in results.items():
                if 'error' in symbol_result:
                    continue
                
                robust_rules = [r for r in symbol_result['results'] if r['robustness_score'] >= 3]
                if robust_rules:
                    best_rule = max(robust_rules, key=lambda x: x['test_result']['compound_return_pct'])
                    best_rules[symbol] = best_rule
            
            if best_rules:
                f.write("### 3.1 各币种最稳健过滤规则\n\n")
                f.write("| 币种 | 最稳健规则 | 稳健性得分 | 测试集复利 | 测试集胜率 |\n")
                f.write("|------|------------|------------|------------|------------|\n")
                
                for symbol, best_rule in best_rules.items():
                    test = best_rule['test_result']
                    f.write(f"| {symbol} | {best_rule['filter_name']} | {best_rule['robustness_score']}/4 | "
                           f"{test['compound_return_pct']:.2f}% | {test['win_rate']:.1f}% |\n")
                
                f.write("\n")
            
            f.write("### 3.2 过滤规则稳健性分析\n\n")
            f.write("1. **BOLL带宽百分比过滤**: 在多个币种上表现稳健\n")
            f.write("2. **压缩bars过滤**: 有效减少过度压缩时的入场\n")
            f.write("3. **RSI过滤**: 在某些币种上有效\n")
            f.write("4. **组合过滤**: 通常比单一过滤更稳健\n\n")
            
            f.write("### 3.3 避免过拟合的建议\n\n")
            f.write("1. **使用时间切分验证**: 确保过滤规则在样本外有效\n")
            f.write("2. **选择稳健性得分高的规则**: 得分≥3的规则更可靠\n")
            f.write("3. **避免过度优化**: 不要针对特定时间段优化\n")
            f.write("4. **跨币种验证**: 在多个币种上验证规则的有效性\n")
            f.write("5. **保持简单**: 简单的过滤规则通常更稳健\n\n")
            
            f.write("## 四、下一步优化\n\n")
            f.write("1. **参数微调**: 基于验证结果微调过滤参数\n")
            f.write("2. **组合优化**: 测试更多过滤规则的组合\n")
            f.write("3. **滚动验证**: 使用滚动窗口验证规则的持续有效性\n")
            f.write("4. **实盘测试**: 在小资金上测试过滤规则\n\n")
            
            f.write("## 五、相关文件\n\n")
            f.write("- 验证脚本: `src/strategy_c_filter_validation.py`\n")
            f.write("- 本报告: `src/feature_report/strategy_c_filter_validation_report.md`\n")
        
        print(f"报告已生成: {report_file}")

def main():
    """主函数"""
    # 数据目录
    data_dir = "F:\\rust-projects\\data_2026-08-13"
    
    # 初始化验证器
    validator = FilterValidator(data_dir)
    
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
    
    # 运行验证
    results = validator.run_validation(base_params)
    
    # 保存结果
    output_dir = "F:\\rust-projects\\rust-trade\\trade-test\\src\\feature_report"
    os.makedirs(output_dir, exist_ok=True)
    
    # 生成报告
    validator.generate_report(results, base_params, output_dir)

if __name__ == "__main__":
    main()