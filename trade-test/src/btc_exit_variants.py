#!/usr/bin/env python3
"""
方案C 出场逻辑变体测试（6币种）

变体：
  ORIG      原始：BOLL中轨止损 + 2%硬止损
  BUF5      中轨止损延迟5根K线（前5根只吃硬止损）
  BUF10     中轨止损延迟10根K线
  BOLL_LOW  动态止损改为 BOLL下轨(做多)/BOLL上轨(做空) + 2%硬止损

输出：交易数 / 胜率 / 复利 / 最大回撤 / 离场原因分布 / 平均持仓根数
"""
import os
import sys
import pandas as pd
import numpy as np
import warnings
warnings.filterwarnings('ignore')

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from strategy_c_final_backtest import FinalBacktester
from btc_ma48_filter_analysis import summarize, DATA_DIR, PARAMS


def backtest_with_exit(df, params, exit_mode='ORIG'):
    df = df.copy()
    hard_stop_pct = params.get('hard_stop_pct', 2.0)
    buffer_bars = {'BUF5': 5, 'BUF10': 10}.get(exit_mode, 0)
    use_boll_lower = exit_mode == 'BOLL_LOW'

    initial_capital = 10000.0
    capital = initial_capital
    position = 0
    entry_price = 0.0
    entry_time = None
    entry_idx = 0
    trades = []

    for i in range(1, len(df)):
        bar = df.iloc[i]

        if position != 0:
            if position == 1:
                cur_profit = (bar['close'] - entry_price) / entry_price * 100
            else:
                cur_profit = (entry_price - bar['close']) / entry_price * 100

            exit_reason = None
            exit_price = bar['close']
            exit_time = bar['open_time']
            bars_held = i - entry_idx

            # 1. 硬止损（始终有效）
            if cur_profit <= -hard_stop_pct:
                exit_reason = 'hard_stop'
            else:
                if use_boll_lower:
                    # BOLL 下轨/上轨动态止损
                    if position == 1 and bar['close'] < bar['boll_lower']:
                        exit_reason = 'boll_lower_stop'
                    elif position == -1 and bar['close'] > bar['boll_upper']:
                        exit_reason = 'boll_lower_stop'
                else:
                    # BOLL 中轨止损（可带时间缓冲）
                    if bars_held >= buffer_bars:
                        if position == 1 and bar['close'] < bar['boll_mid']:
                            exit_reason = 'boll_mid_stop'
                        elif position == -1 and bar['close'] > bar['boll_mid']:
                            exit_reason = 'boll_mid_stop'

            if exit_reason:
                if position == 1:
                    pnl_pct = (exit_price - entry_price) / entry_price * 100
                else:
                    pnl_pct = (entry_price - exit_price) / entry_price * 100
                pnl_amount = capital * (pnl_pct / 100)
                capital += pnl_amount
                trades.append({
                    'entry_time': entry_time, 'exit_time': exit_time,
                    'direction': 'LONG' if position == 1 else 'SHORT',
                    'entry_price': entry_price, 'exit_price': exit_price,
                    'pnl_pct': pnl_pct, 'pnl_amount': pnl_amount,
                    'exit_reason': exit_reason, 'bars_held': bars_held,
                })
                position = 0
                entry_price = 0.0
                entry_time = None

        if position == 0 and bar['signal'] != 0:
            entry_price = bar['close']
            entry_time = bar['open_time']
            entry_idx = i
            position = bar['signal']

    return trades, initial_capital, capital


def main():
    symbols = ['BTC', 'ETH', 'SOL', 'BNB', 'SUI', 'HYPE']
    modes = ['ORIG', 'BUF5', 'BUF10', 'BOLL_LOW']
    rows = []

    for sym in symbols:
        bt = FinalBacktester(DATA_DIR)
        df = bt.load_data(sym)
        df = bt.calculate_indicators(df, PARAMS)
        df = bt.generate_signals(df)

        print(f"\n{'='*72}\n{sym}\n{'='*72}")
        print(f"{'模式':<9}{'交易':>6}{'胜率':>8}{'复利':>9}{'回撤':>9}{'平均持仓':>9}  离场分布")
        for m in modes:
            trades, init_cap, _ = backtest_with_exit(df, PARAMS, m)
            st = summarize(trades, init_cap)
            tdf = pd.DataFrame(trades)
            reason_counts = tdf['exit_reason'].value_counts().to_dict()
            reasons = ", ".join(f"{k}:{v}" for k, v in sorted(reason_counts.items()))
            avg_hold = tdf['bars_held'].mean()
            print(f"{m:<9}{st['trades']:>6}{st['win_rate']:>7.1f}%{st['compound_return']:>8.1f}%"
                  f"{st['max_drawdown']:>8.1f}%{avg_hold:>8.1f}根  {reasons}")
            rows.append({'symbol': sym, 'mode': m, 'trades': st['trades'],
                         'win_rate': round(st['win_rate'], 1),
                         'compound': round(st['compound_return'], 1),
                         'max_dd': round(st['max_drawdown'], 1),
                         'avg_hold': round(avg_hold, 1),
                         'exit_reasons': reasons})

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'feature_report',
                       'strategy_c_exit_variants_results.csv')
    pd.DataFrame(rows).to_csv(out, index=False, encoding='utf-8-sig')
    print(f"\n结果已保存: {out}")


if __name__ == '__main__':
    main()
