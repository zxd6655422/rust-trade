#!/usr/bin/env python3
"""
噪音消除验证（基于 C2 = BOLL_LOW + V3）：
  E1 ATR过滤:   atr_pct <= 该币信号bar中位数（低波动入场，6币同向判别）
  E2 方向过滤:  SUI 只做空 / HYPE 停用（其余保持）
  E3 组合:      E1 + E2
输出每币: 全样本(复利/回撤/taker后) + 2025-2026(复利/回撤/taker后)
"""
import os
import sys
import pandas as pd
import numpy as np
import warnings
warnings.filterwarnings('ignore')

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from strategy_c_final_backtest import FinalBacktester
from btc_ma48_filter_analysis import DATA_DIR, PARAMS
from btc_filter_variants import build_reject_mask

DIR_RULES = {'SUI': 'SHORT', 'HYPE': 'NONE'}  # SUI 只做空；HYPE 全部禁用


def prep_df(symbol):
    bt = FinalBacktester(DATA_DIR)
    df = bt.load_data(symbol)
    df = bt.calculate_indicators(df, PARAMS)
    # ATR14（与 strategy_c_trade_analysis 一致）
    high_low = df['high'] - df['low']
    high_close = np.abs(df['high'] - df['close'].shift())
    low_close = np.abs(df['low'] - df['close'].shift())
    tr = pd.concat([high_low, high_close, low_close], axis=1).max(axis=1)
    df['atr'] = tr.rolling(window=14).mean()
    df['atr_pct'] = df['atr'] / df['close'] * 100
    df = bt.generate_signals(df)
    return df


def run(df, params, use_atr=False, dir_rule=None):
    df = df.copy()
    mask = build_reject_mask(df, 'V3')
    if use_atr:
        # 该币所有信号bar的 atr_pct 中位数（样本内，注明）
        sig_atr = df.loc[df['signal'] != 0, 'atr_pct']
        med = sig_atr.median()
        mask = mask | ((df['signal'] != 0) & (df['atr_pct'] > med))
    if dir_rule == 'SHORT':
        mask = mask | (df['signal'] == 1)
    elif dir_rule == 'LONG':
        mask = mask | (df['signal'] == -1)
    elif dir_rule == 'NONE':
        mask = mask | (df['signal'] != 0)
    df.loc[mask, 'signal'] = 0

    hard_stop_pct = params.get('hard_stop_pct', 2.0)
    capital = 10000.0
    position = 0
    entry_price = 0.0
    entry_time = None
    entry_idx = 0
    trades = []

    for i in range(1, len(df)):
        bar = df.iloc[i]
        if position != 0:
            if position == 1:
                cur = (bar['close'] - entry_price) / entry_price * 100
            else:
                cur = (entry_price - bar['close']) / entry_price * 100
            reason = None
            if cur <= -hard_stop_pct:
                reason = 'hard_stop'
            elif position == 1 and bar['close'] < bar['boll_lower']:
                reason = 'boll_lower_stop'
            elif position == -1 and bar['close'] > bar['boll_upper']:
                reason = 'boll_lower_stop'
            if reason:
                pnl = (bar['close'] - entry_price) / entry_price * 100 if position == 1 \
                    else (entry_price - bar['close']) / entry_price * 100
                pnl_amount = capital * (pnl / 100)
                capital += pnl_amount
                trades.append({'entry_time': entry_time, 'pnl_pct': pnl,
                               'pnl_amount': pnl_amount})
                position = 0
        if position == 0 and bar['signal'] != 0:
            entry_price = bar['close']
            entry_time = bar['open_time']
            entry_idx = i
            position = bar['signal']

    return trades, capital


def stats(trades, initial=10000.0, fee=0.0):
    if not trades:
        return {'n': 0, 'compound': 0, 'dd': 0}
    cap = initial
    curve = [cap]
    for t in trades:
        pnl = t['pnl_amount']
        fee_amt = cap * (fee / 100)
        cap += pnl - fee_amt
        curve.append(cap)
    s = pd.Series(curve)
    dd = ((s - s.expanding().max()) / s.expanding().max() * 100).min()
    return {'n': len(trades), 'compound': (cap - initial) / initial * 100, 'dd': dd}


def recent_trades(trades):
    return [t for t in trades if pd.to_datetime(t['entry_time']).year >= 2025]


def main():
    symbols = ['BTC', 'ETH', 'SOL', 'BNB', 'SUI', 'HYPE']
    modes = [('C2', False, None), ('C2+ATR', True, None),
             ('C2+DIR', False, None), ('C2+ATR+DIR', True, None)]
    print(f"{'币种':<5}{'模式':<12}{'全样本笔数':>8}{'复利':>9}{'回撤':>8}{'taker后':>9} | "
          f"{'25-26笔数':>8}{'复利':>9}{'回撤':>8}{'taker后':>9}")
    for sym in symbols:
        df = prep_df(sym)
        dir_rule = DIR_RULES.get(sym)
        for name, use_atr, _ in modes:
            trades, _ = run(df, PARAMS, use_atr=use_atr, dir_rule=dir_rule)
            fs = stats(trades)
            fs_t = stats(trades, fee=0.10)
            rt = recent_trades(trades)
            rs = stats(rt)
            rs_t = stats(rt, fee=0.10)
            print(f"{sym:<5}{name:<12}{fs['n']:>8}{fs['compound']:>8.1f}%{fs['dd']:>7.1f}%"
                  f"{fs_t['compound']:>8.1f}% | {rs['n']:>8}{rs['compound']:>8.1f}%"
                  f"{rs['dd']:>7.1f}%{rs_t['compound']:>8.1f}%")


if __name__ == '__main__':
    main()
