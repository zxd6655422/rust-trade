#!/usr/bin/env python3
"""
方案C交易分析 - 亏损单与盈利单特征对比
"""

import pandas as pd
import numpy as np
import os
import json
from datetime import datetime
from typing import Dict, List, Tuple
import warnings
warnings.filterwarnings('ignore')

class TradeAnalyzer:
    """交易分析器"""
    
    def __init__(self, data_dir: str):
        """
        初始化分析器
        
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
            # 处理带时区的格式
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
    
    def run_backtest_with_features(self, df: pd.DataFrame, params: dict) -> List[Dict]:
        """
        运行回测并记录每笔交易的特征
        
        Args:
            df: 包含信号的DataFrame
            params: 策略参数
            
        Returns:
            交易列表（包含特征）
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
        entry_idx = 0
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
                    
                    # 记录交易特征
                    trade = {
                        'entry_time': entry_time,
                        'exit_time': exit_time,
                        'direction': 'LONG' if position == 1 else 'SHORT',
                        'entry_price': entry_price,
                        'exit_price': exit_price,
                        'pnl_pct': pnl_pct,
                        'pnl_amount': pnl_amount,
                        'exit_reason': exit_reason,
                        'max_profit_pct': max_profit_pct,
                        'hold_bars': i - entry_idx,
                        
                        # 入场时的特征
                        'entry_boll_width_pct': prev_bar.get('boll_width_pct', 0),
                        'entry_compression_bars': prev_bar.get('compression_bars', 0),
                        'entry_rsi': prev_bar.get('rsi', 50),
                        'entry_atr_pct': prev_bar.get('atr_pct', 0),
                        'entry_volume_ratio': prev_bar.get('volume_ratio', 1),
                        'entry_momentum_5': prev_bar.get('momentum_5', 0),
                        'entry_momentum_10': prev_bar.get('momentum_10', 0),
                        'entry_momentum_20': prev_bar.get('momentum_20', 0),
                        'entry_price_position': prev_bar.get('price_position', 0.5),
                        'entry_ma192_slope': prev_bar.get('ma192_slope', 0),
                        'entry_boll_mid_slope': prev_bar.get('boll_mid_slope', 0),
                        'entry_ma_above_mid': prev_bar.get('ma_above_mid', False),
                        'entry_boll_width': prev_bar.get('boll_width', 0),
                        'entry_close': prev_bar.get('close', 0),
                        'entry_ma192': prev_bar.get('ma192', 0),
                        'entry_boll_mid': prev_bar.get('boll_mid', 0),
                        
                        # 出场时的特征
                        'exit_rsi': current_bar.get('rsi', 50),
                        'exit_atr_pct': current_bar.get('atr_pct', 0),
                        'exit_volume_ratio': current_bar.get('volume_ratio', 1),
                        'exit_price_position': current_bar.get('price_position', 0.5),
                        'exit_boll_width': current_bar.get('boll_width', 0),
                    }
                    trades.append(trade)
                    
                    position = 0
                    entry_price = 0.0
                    entry_time = None
                    entry_idx = 0
                    max_profit_pct = 0.0
            
            # 如果没有持仓，检查入场信号
            if position == 0 and current_bar['signal'] != 0:
                signal = current_bar['signal']
                entry_price = current_bar['close']
                entry_time = current_bar['open_time']
                entry_idx = i
                position = signal
                max_profit_pct = 0.0
        
        return trades
    
    def analyze_trade_features(self, trades: List[Dict]) -> Dict:
        """
        分析交易特征
        
        Args:
            trades: 交易列表
            
        Returns:
            分析结果
        """
        if not trades:
            return {}
        
        trades_df = pd.DataFrame(trades)
        
        # 分离盈利单和亏损单
        winning_trades = trades_df[trades_df['pnl_pct'] > 0]
        losing_trades = trades_df[trades_df['pnl_pct'] <= 0]
        
        # 计算特征统计
        feature_columns = [
            'entry_boll_width_pct', 'entry_compression_bars', 'entry_rsi',
            'entry_atr_pct', 'entry_volume_ratio', 'entry_momentum_5',
            'entry_momentum_10', 'entry_momentum_20', 'entry_price_position',
            'entry_ma192_slope', 'entry_boll_mid_slope', 'entry_boll_width'
        ]
        
        analysis = {
            'total_trades': len(trades_df),
            'winning_trades': len(winning_trades),
            'losing_trades': len(losing_trades),
            'win_rate': len(winning_trades) / len(trades_df) * 100,
            'avg_pnl': trades_df['pnl_pct'].mean(),
            'avg_win': winning_trades['pnl_pct'].mean() if len(winning_trades) > 0 else 0,
            'avg_loss': losing_trades['pnl_pct'].mean() if len(losing_trades) > 0 else 0,
            'feature_comparison': {}
        }
        
        # 比较每个特征
        for feature in feature_columns:
            if feature in trades_df.columns:
                win_values = winning_trades[feature].dropna()
                lose_values = losing_trades[feature].dropna()
                
                if len(win_values) > 0 and len(lose_values) > 0:
                    comparison = {
                        'win_mean': win_values.mean(),
                        'lose_mean': lose_values.mean(),
                        'win_median': win_values.median(),
                        'lose_median': lose_values.median(),
                        'win_std': win_values.std(),
                        'lose_std': lose_values.std(),
                        'difference': win_values.mean() - lose_values.mean(),
                        'difference_pct': (win_values.mean() - lose_values.mean()) / lose_values.mean() * 100 if lose_values.mean() != 0 else 0
                    }
                    analysis['feature_comparison'][feature] = comparison
        
        return analysis
    
    def analyze_symbol(self, symbol: str, params: dict) -> Dict:
        """分析单个币种"""
        print(f"分析 {symbol}...")
        
        # 加载数据
        df = self.load_data(symbol)
        print(f"  数据量: {len(df)} 根K线")
        
        # 计算指标
        df = self.calculate_indicators(df, params)
        
        # 生成信号
        df = self.generate_signals(df)
        
        # 运行回测并记录特征
        trades = self.run_backtest_with_features(df, params)
        
        # 分析交易特征
        analysis = self.analyze_trade_features(trades)
        
        # 显示结果
        print(f"  总交易数: {analysis['total_trades']}")
        print(f"  胜率: {analysis['win_rate']:.1f}%")
        print(f"  平均收益: {analysis['avg_pnl']:.2f}%")
        print(f"  盈利单平均: {analysis['avg_win']:.2f}%")
        print(f"  亏损单平均: {analysis['avg_loss']:.2f}%")
        
        return {
            'symbol': symbol,
            'analysis': analysis,
            'trades': trades
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
        print("方案C交易特征分析 - 亏损单 vs 盈利单")
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
        """生成分析报告"""
        report_file = os.path.join(output_dir, "strategy_c_trade_analysis.md")
        
        with open(report_file, 'w', encoding='utf-8') as f:
            f.write("# 方案C交易特征分析报告\n\n")
            f.write(f"> 生成时间：{datetime.now().strftime('%Y-%m-%d %H:%M')}\n")
            f.write("> 分析目标：亏损单 vs 盈利单特征对比\n")
            f.write("> 数据：30m K线（真实数据）\n\n")
            
            # 策略参数
            f.write("## 一、策略参数\n\n")
            f.write("| 参数 | 值 |\n")
            f.write("|------|-----|\n")
            for key, value in params.items():
                f.write(f"| {key} | {value} |\n")
            f.write("\n")
            
            # 各币种分析
            f.write("## 二、各币种特征分析\n\n")
            
            for symbol, result in results.items():
                if 'error' in result:
                    f.write(f"### {symbol} - 错误\n\n")
                    f.write(f"错误信息: {result['error']}\n\n")
                    continue
                
                analysis = result['analysis']
                
                f.write(f"### {symbol}\n\n")
                
                # 基本统计
                f.write("**基本统计**:\n\n")
                f.write(f"- 总交易数: {analysis['total_trades']}\n")
                f.write(f"- 盈利单: {analysis['winning_trades']} ({analysis['win_rate']:.1f}%)\n")
                f.write(f"- 亏损单: {analysis['losing_trades']} ({100-analysis['win_rate']:.1f}%)\n")
                f.write(f"- 平均收益: {analysis['avg_pnl']:.2f}%\n")
                f.write(f"- 盈利单平均: {analysis['avg_win']:.2f}%\n")
                f.write(f"- 亏损单平均: {analysis['avg_loss']:.2f}%\n\n")
                
                # 特征对比
                if 'feature_comparison' in analysis and analysis['feature_comparison']:
                    f.write("**特征对比（盈利单 vs 亏损单）**:\n\n")
                    f.write("| 特征 | 盈利单均值 | 亏损单均值 | 差异 | 差异百分比 |\n")
                    f.write("|------|------------|------------|------|------------|\n")
                    
                    for feature, comp in analysis['feature_comparison'].items():
                        # 格式化特征名称
                        feature_name = feature.replace('entry_', '').replace('_', ' ').title()
                        
                        f.write(f"| {feature_name} | {comp['win_mean']:.4f} | {comp['lose_mean']:.4f} | "
                               f"{comp['difference']:.4f} | {comp['difference_pct']:.1f}% |\n")
                    
                    f.write("\n")
                    
                    # 找出差异最大的特征
                    significant_features = []
                    for feature, comp in analysis['feature_comparison'].items():
                        if abs(comp['difference_pct']) > 10:  # 差异超过10%
                            significant_features.append((feature, comp))
                    
                    if significant_features:
                        f.write("**显著差异特征（差异>10%）**:\n\n")
                        for feature, comp in significant_features:
                            feature_name = feature.replace('entry_', '').replace('_', ' ').title()
                            direction = "更高" if comp['difference'] > 0 else "更低"
                            f.write(f"- **{feature_name}**: 盈利单比亏损单{direction} {abs(comp['difference_pct']):.1f}%\n")
                        f.write("\n")
                
                f.write("---\n\n")
            
            # 总结与过滤建议
            f.write("## 三、总结与过滤建议\n\n")
            
            # 收集所有币种的显著特征
            all_significant_features = {}
            for symbol, result in results.items():
                if 'error' in result:
                    continue
                
                analysis = result['analysis']
                if 'feature_comparison' in analysis:
                    for feature, comp in analysis['feature_comparison'].items():
                        if abs(comp['difference_pct']) > 10:
                            if feature not in all_significant_features:
                                all_significant_features[feature] = []
                            all_significant_features[feature].append({
                                'symbol': symbol,
                                'difference_pct': comp['difference_pct'],
                                'win_mean': comp['win_mean'],
                                'lose_mean': comp['lose_mean']
                            })
            
            if all_significant_features:
                f.write("### 3.1 跨币种显著特征\n\n")
                f.write("| 特征 | 出现币种数 | 平均差异 | 方向 |\n")
                f.write("|------|------------|----------|------|\n")
                
                for feature, occurrences in all_significant_features.items():
                    feature_name = feature.replace('entry_', '').replace('_', ' ').title()
                    avg_diff = np.mean([occ['difference_pct'] for occ in occurrences])
                    direction = "盈利单更高" if avg_diff > 0 else "盈利单更低"
                    
                    f.write(f"| {feature_name} | {len(occurrences)} | {avg_diff:.1f}% | {direction} |\n")
                
                f.write("\n")
            
            f.write("### 3.2 过滤规则建议\n\n")
            f.write("基于特征分析，建议以下过滤规则：\n\n")
            f.write("1. **BOLL带宽百分比过滤**: 在特定压缩程度下入场\n")
            f.write("2. **RSI过滤**: 避免在超买/超卖区域入场\n")
            f.write("3. **动量过滤**: 避免在极端动量下入场\n")
            f.write("4. **成交量过滤**: 在成交量异常时谨慎入场\n")
            f.write("5. **价格位置过滤**: 避免在BOLL带极端位置入场\n\n")
            
            f.write("## 四、下一步优化\n\n")
            f.write("1. **参数优化**: 基于特征分析优化策略参数\n")
            f.write("2. **过滤规则实现**: 实现基于特征的过滤规则\n")
            f.write("3. **回测验证**: 验证过滤规则的有效性\n")
            f.write("4. **样本外测试**: 进行时间切分验证\n\n")
            
            f.write("## 五、相关文件\n\n")
            f.write("- 分析脚本: `src/strategy_c_trade_analysis.py`\n")
            f.write("- 本报告: `src/feature_report/strategy_c_trade_analysis.md`\n")
        
        print(f"报告已生成: {report_file}")

def main():
    """主函数"""
    # 数据目录
    data_dir = "F:\\rust-projects\\data_2026-08-13"
    
    # 初始化分析器
    analyzer = TradeAnalyzer(data_dir)
    
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
    results = analyzer.run_analysis(params)
    
    # 保存结果
    output_dir = "F:\\rust-projects\\rust-trade\\trade-test\\src\\feature_report"
    os.makedirs(output_dir, exist_ok=True)
    
    # 保存详细结果到JSON
    output_file = os.path.join(output_dir, "strategy_c_trade_analysis_results.json")
    
    # 准备保存的数据
    save_results = {}
    for symbol, result in results.items():
        if 'error' in result:
            save_results[symbol] = result
        else:
            save_result = {
                'analysis': result['analysis'],
                'trades_count': len(result['trades'])
            }
            save_results[symbol] = save_result
    
    with open(output_file, 'w', encoding='utf-8') as f:
        json.dump(save_results, f, indent=2, ensure_ascii=False, default=str)
    
    print(f"详细结果已保存到: {output_file}")
    
    # 生成报告
    analyzer.generate_report(results, params, output_dir)

if __name__ == "__main__":
    main()