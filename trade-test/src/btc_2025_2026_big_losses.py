#!/usr/bin/env python3
"""
提取方案C策略在BTC上2025-2026年的逐笔交易明细，
按亏损金额排序，列出最大亏损单（入场时间/出场时间/入场价/止损价/盈亏）。
回测逻辑与 src/strategy_c_final_backtest.py 完全一致。
"""
import os
import sys
import json
import pandas as pd
import numpy as np
import warnings
warnings.filterwarnings('ignore')

# 复用最终回测器的逻辑
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from strategy_c_final_backtest import FinalBacktester

DATA_DIR = r"D:\dev-projects\data_2026-08-13"

PARAMS = {
    'ma_period': 192,
    'boll_period': 100,
    'boll_std': 2.0,
    'compression_threshold': 0.3,
    'min_compression_bars': 10,
    'hard_stop_pct': 2.0,
    'boll_stop_enabled': True,
}


def run_with_trade_details():
    """运行回测，并为每笔交易补充止损价信息"""
    backtester = FinalBacktester(DATA_DIR)

    # 重写 run_backtest，记录止损价（boll_mid / 硬止损价）
    df = backtester.load_data('BTC')
    df = backtester.calculate_indicators(df, PARAMS)
    df = backtester.generate_signals(df)
    df = df.copy()

    hard_stop_pct = PARAMS.get('hard_stop_pct', 2.0)
    initial_capital = 10000.0
    capital = initial_capital
    position = 0
    entry_price = 0.0
    entry_time = None
    entry_boll_mid = None
    trades = []

    for i in range(1, len(df)):
        current_bar = df.iloc[i]

        if position != 0:
            if position == 1:
                current_profit_pct = (current_bar['close'] - entry_price) / entry_price * 100
            else:
                current_profit_pct = (entry_price - current_bar['close']) / entry_price * 100

            exit_reason = None
            exit_price = current_bar['close']
            exit_time = current_bar['open_time']

            if current_profit_pct <= -hard_stop_pct:
                exit_reason = 'hard_stop'
            elif PARAMS['boll_stop_enabled']:
                if position == 1 and current_bar['close'] < current_bar['boll_mid']:
                    exit_reason = 'boll_mid_stop'
                elif position == -1 and current_bar['close'] > current_bar['boll_mid']:
                    exit_reason = 'boll_mid_stop'

            if exit_reason:
                if position == 1:
                    pnl_pct = (exit_price - entry_price) / entry_price * 100
                else:
                    pnl_pct = (entry_price - exit_price) / entry_price * 100
                pnl_amount = capital * (pnl_pct / 100)
                capital += pnl_amount

                # 止损价：boll_mid_stop = 出场bar的BOLL中轨；hard_stop = 入场价±2%
                if exit_reason == 'hard_stop':
                    stop_price = entry_price * (1 - hard_stop_pct / 100) if position == 1 \
                        else entry_price * (1 + hard_stop_pct / 100)
                else:
                    stop_price = current_bar['boll_mid']

                trade = {
                    'entry_time': entry_time,
                    'exit_time': exit_time,
                    'direction': 'LONG' if position == 1 else 'SHORT',
                    'entry_price': entry_price,
                    'exit_price': exit_price,
                    'stop_price': stop_price,
                    'entry_boll_mid': entry_boll_mid,
                    'bars_held': i - entry_idx if 'entry_idx' in dir() else 0,
                    'pnl_pct': pnl_pct,
                    'pnl_amount': pnl_amount,
                    'exit_reason': exit_reason,
                }
                trades.append(trade)

                position = 0
                entry_price = 0.0
                entry_time = None
                entry_boll_mid = None

        if position == 0 and current_bar['signal'] != 0:
            signal = current_bar['signal']
            entry_price = current_bar['close']
            entry_time = current_bar['open_time']
            entry_boll_mid = current_bar['boll_mid']
            entry_idx = i
            position = signal

    trades_df = pd.DataFrame(trades)
    trades_df['entry_time'] = pd.to_datetime(trades_df['entry_time'])
    trades_df['exit_time'] = pd.to_datetime(trades_df['exit_time'])
    trades_df['year'] = trades_df['entry_time'].dt.year

    print(f"BTC 总交易数: {len(trades_df)}")
    print(f"年度分布: {trades_df['year'].value_counts().sort_index().to_dict()}")
    print(f"总复利收益: {(capital - initial_capital) / initial_capital * 100:.2f}%")
    print(f"胜率: {(trades_df['pnl_pct'] > 0).mean() * 100:.1f}%")

    # 25-26年
    recent = trades_df[trades_df['year'] >= 2025].copy()
    recent = recent.sort_values('pnl_pct', ascending=True).reset_index(drop=True)
    print(f"\n2025-2026 交易数: {len(recent)}, 亏损单数: {(recent['pnl_pct'] < 0).sum()}")

    return trades_df, recent


def fmt_time(t):
    return t.strftime('%Y-%m-%d %H:%M')


def main():
    trades_df, recent = run_with_trade_details()

    print("\n===== 2025-2026 最大亏损单 TOP 15 =====")
    print(f"{'#':>2} {'方向':<5} {'入场时间':<17} {'出场时间':<17} {'入场价':>10} "
          f"{'止损价':>10} {'出场价':>10} {'盈亏%':>8} {'盈亏金额':>10} {'离场原因'}")
    for idx, row in recent.head(15).iterrows():
        print(f"{idx+1:>2} {row['direction']:<5} {fmt_time(row['entry_time']):<17} {fmt_time(row['exit_time']):<17} "
              f"{row['entry_price']:>10.1f} {row['stop_price']:>10.1f} {row['exit_price']:>10.1f} "
              f"{row['pnl_pct']:>8.2f} {row['pnl_amount']:>10.2f} {row['exit_reason']}")

    # 保存明细
    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'feature_report',
                       'strategy_c_btc_2025_2026_trades.csv')
    recent.to_csv(out, index=False, encoding='utf-8-sig')
    print(f"\n明细已保存: {out}")

    # 全期最大亏损单（供对照）
    all_sorted = trades_df.sort_values('pnl_pct', ascending=True)
    print("\n===== 全期最大亏损单 TOP 5 =====")
    for idx, row in all_sorted.head(5).iterrows():
        print(f"{row['direction']:<5} {fmt_time(row['entry_time']):<17} {fmt_time(row['exit_time']):<17} "
              f"{row['entry_price']:>10.1f} {row['stop_price']:>10.1f} {row['exit_price']:>10.1f} "
              f"{row['pnl_pct']:>8.2f} {row['exit_reason']}")


if __name__ == "__main__":
    main()
