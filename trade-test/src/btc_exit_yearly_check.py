#!/usr/bin/env python3
"""
出场变体分年度验证：重点看 2025/2026 年（ORIG vs BUF5 vs BOLL_LOW）
"""
import os
import sys
import pandas as pd
import warnings
warnings.filterwarnings('ignore')

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from strategy_c_final_backtest import FinalBacktester
from btc_ma48_filter_analysis import DATA_DIR, PARAMS
from btc_exit_variants import backtest_with_exit


def yearly_stats(trades):
    df = pd.DataFrame(trades)
    df['entry_time'] = pd.to_datetime(df['entry_time'])
    df['year'] = df['entry_time'].dt.year
    out = {}
    for year, g in df.groupby('year'):
        wins = int((g['pnl_pct'] > 0).sum())
        cap = 10000.0
        for t in g.to_dict('records'):
            cap += t['pnl_amount']
        out[year] = {
            'n': len(g),
            'win_rate': wins / len(g) * 100,
            'compound': (cap - 10000.0) / 10000.0 * 100,
        }
    return out


def main():
    symbols = ['BTC', 'ETH', 'SOL', 'BNB', 'SUI', 'HYPE']
    modes = ['ORIG', 'BUF5', 'BOLL_LOW']

    for sym in symbols:
        bt = FinalBacktester(DATA_DIR)
        df = bt.load_data(sym)
        df = bt.calculate_indicators(df, PARAMS)
        df = bt.generate_signals(df)

        rows = {}
        for m in modes:
            trades, _, _ = backtest_with_exit(df, PARAMS, m)
            rows[m] = yearly_stats(trades)

        years = sorted(set().union(*[set(r.keys()) for r in rows.values()]))
        recent = [y for y in years if y >= 2024]

        print(f"\n===== {sym}（近三年 2024-2026） =====")
        print(f"{'模式':<9}" + "".join(f"  {y} 笔数/胜率/复利" for y in recent))
        for m in modes:
            cells = []
            for y in recent:
                s = rows[m].get(y, {'n': 0, 'win_rate': 0, 'compound': 0})
                cells.append(f"  {s['n']:>3} / {s['win_rate']:>4.1f}% / {s['compound']:>6.1f}%")
            print(f"{m:<9}" + "".join(cells))


if __name__ == '__main__':
    main()
