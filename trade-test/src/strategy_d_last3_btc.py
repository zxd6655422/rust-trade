#!/usr/bin/env python3
"""BTC 方案D E6 最近3笔持仓明细（BASE 与 SLOPE_D W10）"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from strategy_d_feasibility import load_1h, indicators, compute_trend_flags
from strategy_d_entry_state_analysis import precompute
from strategy_d_slope_filter import backtest


def show(symbol, variant, window=10):
    bars = load_1h(symbol)
    bars = indicators(bars)
    bars = compute_trend_flags(bars)
    bars = precompute(bars)
    trades, _, _ = backtest(bars, variant, window)
    print(f"\n===== {symbol} | {variant} W{window} | 总 {len(trades)} 笔 | 数据止于 {bars[-1]['t'][:16]} =====")
    print(f"{'#':>2} {'方向':<5} {'入场时间':<17} {'入场价':>10} {'出场时间':<17} {'出场价':>10} {'盈亏%':>8} {'持仓':>5}  入场时MA48/中轨/斜率")
    for k in range(1, 4):
        t = trades[-k]
        entry = next(b for b in bars if b['t'] == t['entry_time'])
        print(f"{k:>2} {t['direction']:<5} {t['entry_time'][:16]:<17} {t['entry_price']:>10.2f} "
              f"{t['exit_time'][:16]:<17} {t['exit_price']:>10.2f} {t['pnl_pct']:>+7.2f}% {t['bars_held']:>4}根  "
              f"MA48={entry['ma48']:.2f} 中轨={entry['mid']:.2f} 斜率24={entry['slope24']:+.3f}%")


if __name__ == '__main__':
    show('BTC', 'BASE', 10)
    show('BTC', 'SLOPE_D', 10)
