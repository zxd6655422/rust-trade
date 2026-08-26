#!/usr/bin/env python3
"""
方案D E6 + MA48斜率方向过滤（纯标准库）

背景：D2b 分析发现"盈利单入场时 MA48 斜率上升、亏损单走平/向下"。
此处验证：E6 入场叠加斜率方向过滤（做多 slope24>0 / 做空 slope24<0），
并扫阈值 0 / 0.05 / 0.1，另加"排除窄带(zone宽度<0.5%)"对照。

变体：
  BASE      E6 基线（无过滤）
  SLOPE_D   slope24 > 0(多) / < 0(空)
  SLOPE_05  slope24 > 0.05 / < -0.05
  SLOPE_10  slope24 > 0.1 / < -0.1
  SLD_NB    SLOPE_D + 排除窄带(zw_pct >= 0.5)
"""
import os
import sys
import csv

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from strategy_d_feasibility import load_1h, indicators, compute_trend_flags
from strategy_d_entry_state_analysis import precompute


def backtest(bars, variant, window=10):
    n = len(bars)
    initial = 10000.0
    capital = initial
    position = 0
    entry_price = 0.0
    entry_time = None
    entry_idx = 0
    trades = []
    recent = []

    thresh = {'SLOPE_D': 0.0, 'SLOPE_05': 0.05, 'SLOPE_10': 0.10}.get(variant, 0.0)
    use_slope = variant != 'BASE'
    use_nb = variant == 'SLD_NB'

    for i in range(1, n):
        cur = bars[i]
        recent.append(1 if (cur['ztop'] is not None and cur['l'] <= cur['ztop'] and cur['h'] >= cur['zbottom']) else 0)
        if len(recent) > window:
            recent.pop(0)

        if position != 0:
            exit_reason = None
            exit_price = cur['c']
            exit_time = cur['t']
            bars_held = i - entry_idx
            if len(recent) == window and sum(recent) / window >= 0.5:
                exit_reason = 'congestion'
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
                    'pnl_pct': pnl_pct, 'pnl_amount': pnl_amount, 'bars_held': bars_held,
                    'year': int(entry_time[:4]),
                })
                position = 0
                entry_price = 0.0
                entry_time = None

        if position == 0:
            prev = bars[i - 1]
            if prev['ztop'] is None or cur['ztop'] is None:
                sig = 0
            else:
                sig = 0
                slope = cur['slope24']
                zw = cur['zw_pct']
                ok_slope = True
                if use_slope and slope is not None:
                    ok_slope = slope > thresh if (prev['trend_up']) else (slope < -thresh if prev['trend_down'] else True)
                if use_nb and zw is not None and zw < 0.5:
                    ok_slope = False
                if ok_slope:
                    if prev['trend_up'] and prev['c'] > prev['ztop'] and cur['c'] < cur['ztop'] and cur['c'] > cur['zbottom']:
                        sig = 1
                    elif prev['trend_down'] and prev['c'] < prev['zbottom'] and cur['c'] > cur['zbottom'] and cur['c'] < cur['ztop']:
                        sig = -1
            if sig != 0:
                entry_price = cur['c']
                entry_time = cur['t']
                entry_idx = i
                position = sig

    return trades, initial, capital


def stats(trades, initial):
    if not trades:
        return None
    n = len(trades)
    wins = sum(1 for t in trades if t['pnl_pct'] > 0)
    cap = initial
    curve = [cap]
    for t in trades:
        cap += t['pnl_amount']
        curve.append(cap)
    peak = curve[0]
    max_dd = 0.0
    for c in curve:
        peak = max(peak, c)
        dd = (c - peak) / peak * 100
        if dd < max_dd:
            max_dd = dd
    return {'n': n, 'win': wins / n * 100, 'compound': (cap - initial) / initial * 100, 'dd': max_dd,
            'avg_hold': sum(t['bars_held'] for t in trades) / n}


def yearly(trades, start=2024):
    out = {}
    for t in trades:
        if t['year'] >= start:
            out.setdefault(t['year'], []).append(t)
    res = {}
    for y, ts in out.items():
        cap = 10000.0
        for t in ts:
            cap += t['pnl_amount']
        res[y] = (cap - 10000.0) / 10000.0 * 100
    return res


def with_fee(trades, fee=0.10):
    cap = 10000.0
    for t in trades:
        cap += t['pnl_amount'] - cap * (fee / 100)
    return (cap - 10000.0) / 10000.0 * 100


def main():
    symbols = ['BTC', 'ETH', 'SOL', 'BNB', 'SUI', 'HYPE']
    variants = ['BASE', 'SLOPE_D', 'SLOPE_05', 'SLOPE_10', 'SLD_NB']
    windows = [10, 20]
    rows = []

    for sym in symbols:
        bars = load_1h(sym)
        bars = indicators(bars)
        bars = compute_trend_flags(bars)
        bars = precompute(bars)
        print(f"\n{'='*100}\n{sym} | {len(bars)} 根1hK线\n{'='*100}")
        print(f"{'变体':<9}{'窗口':>5}{'笔数':>7}{'胜率':>8}{'复利':>10}{'回撤':>9}{'持仓':>7}{'taker后':>10} | 2024 / 2025 / 2026")
        for v in variants:
            for w in windows:
                trades, init, _ = backtest(bars, v, w)
                st = stats(trades, init)
                if st is None:
                    print(f"{v:<9}{w:>5} 无交易")
                    continue
                yrs = yearly(trades)
                fee = with_fee(trades)
                c = " / ".join(f"{y}:{yrs.get(y, 0):.0f}%" for y in (2024, 2025, 2026))
                print(f"{v:<9}{w:>5}{st['n']:>7}{st['win']:>7.1f}%{st['compound']:>9.1f}%{st['dd']:>8.1f}%"
                      f"{st['avg_hold']:>6.1f}根{fee:>9.1f}% | {c}")
                rows.append({'symbol': sym, 'variant': v, 'window': w,
                             'trades': st['n'], 'win_rate': round(st['win'], 1),
                             'compound': round(st['compound'], 1), 'max_dd': round(st['dd'], 1),
                             'avg_hold': round(st['avg_hold'], 1), 'fee': round(fee, 1),
                             'y2024': round(yrs.get(2024, 0), 1), 'y2025': round(yrs.get(2025, 0), 1),
                             'y2026': round(yrs.get(2026, 0), 1)})

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'feature_report',
                       'strategy_d_slope_filter_results.csv')
    with open(out, 'w', encoding='utf-8-sig', newline='') as f:
        writer = csv.DictWriter(f, fieldnames=list(rows[0].keys()))
        writer.writeheader()
        writer.writerows(rows)
    print(f"\n结果已保存: {out}")


if __name__ == '__main__':
    main()
