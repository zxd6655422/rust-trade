#!/usr/bin/env python3
"""严格双底/双顶形态过滤（收窄容差 + 强制中间反弹）在 BTC 上的快速验证"""
import sys
sys.path.insert(0, '.')
from strategy_d_feasibility import load_1h, indicators
from strategy_d_entry_state_analysis import precompute
from strategy_d_double_pattern import compute_swings


def has_pattern_strict(bars, swings, i, LB, tol, recent, min_sep, rebound, use_low):
    start = max(0, i - LB)
    pts = [j for j in range(start, i) if swings[j]]
    for a in range(len(pts)):
        for b in range(a + 1, len(pts)):
            j1, j2 = pts[a], pts[b]
            if i - j2 > recent or j2 - j1 < min_sep:
                continue
            v1 = bars[j1]['l'] if use_low else bars[j1]['h']
            v2 = bars[j2]['l'] if use_low else bars[j2]['h']
            if abs(v1 - v2) / max(v1, v2) > tol:
                continue
            if use_low:
                peak = max(bars[j]['h'] for j in range(j1, j2 + 1))
                if peak >= min(v1, v2) * (1 + rebound / 100):
                    return True
            else:
                trough = min(bars[j]['l'] for j in range(j1, j2 + 1))
                if trough <= max(v1, v2) * (1 - rebound / 100):
                    return True
    return False


def run(tol, rebound, LB=72, recent=24, min_sep=8, w=10):
    bars = load_1h('BTC')
    bars = indicators(bars)
    bars = precompute(bars)
    sw_low, sw_high = compute_swings(bars, 2)
    n = len(bars)
    init = 10000.0
    cap = init
    pos = 0
    ep = 0.0
    trades = []
    recent_win = []
    for i in range(1, n):
        cur = bars[i]
        inter = (cur['l'] <= cur['ztop'] and cur['h'] >= cur['zbottom']) if cur['ztop'] is not None else False
        recent_win.append(1 if inter else 0)
        if len(recent_win) > w:
            recent_win.pop(0)
        if pos != 0:
            if len(recent_win) == w and sum(recent_win) / w >= 0.5:
                pnl = (cur['c'] - ep) / ep * 100 if pos == 1 else (ep - cur['c']) / ep * 100
                cap += cap * (pnl / 100)
                trades.append(pnl)
                pos = 0
        if pos == 0:
            prev = bars[i - 1]
            if prev['mid'] is not None and cur['mid'] is not None:
                sig = 0
                if prev['c'] < prev['mid'] and cur['c'] > cur['mid']:
                    sig = 1
                elif prev['c'] > prev['mid'] and cur['c'] < cur['mid']:
                    sig = -1
                if sig == 1 and not has_pattern_strict(bars, sw_low, i, LB, tol, recent, min_sep, rebound, True):
                    sig = 0
                if sig == -1 and not has_pattern_strict(bars, sw_high, i, LB, tol, recent, min_sep, rebound, False):
                    sig = 0
                if sig != 0:
                    ep = cur['c']
                    pos = sig
    if not trades:
        return None
    wr = sum(1 for t in trades if t > 0) / len(trades) * 100
    return {'n': len(trades), 'wr': wr, 'comp': (cap - init) / init * 100}


if __name__ == '__main__':
    print("BTC 严格形态过滤（含中间反弹）对比：")
    for tol, reb in [(0.3, 0.0), (0.3, 1.5), (0.3, 2.0), (0.5, 2.0), (0.2, 1.5)]:
        r = run(tol, reb)
        if r:
            print(f"  tol={tol}% 反弹>={reb}%: 笔数 {r['n']:>5} | 胜率 {r['wr']:.1f}% | 复利 {r['comp']:+.1f}%")
