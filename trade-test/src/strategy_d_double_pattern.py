#!/usr/bin/env python3
"""
验证：收盘穿越中轨(规则A) + 双底/双顶形态确认 是否有效

双底(做多前提)：回看窗口内存在 >=2 个摆动低点，高低点差 <= tol，最后一个低点在近期
双顶(做空前提)：对称（摆动高点）
摆动点：±k 根邻域内的极值
"""
import os
import sys
import csv

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from strategy_d_feasibility import load_1h, indicators
from strategy_d_entry_state_analysis import precompute


def compute_swings(bars, k=2):
    n = len(bars)
    sw_low = [False] * n
    sw_high = [False] * n
    for i in range(k, n - k):
        b = bars[i]
        if all(b['l'] <= bars[j]['l'] for j in range(i - k, i + k + 1)):
            sw_low[i] = True
        if all(b['h'] >= bars[j]['h'] for j in range(i - k, i + k + 1)):
            sw_high[i] = True
    return sw_low, sw_high


def has_pattern(bars, swings, i, LB, tol, recent, min_sep, use_low):
    """swings: 摆动点布尔数组（use_low=True 用低点=双底，False 用高点=双顶）"""
    start = max(0, i - LB)
    pts = [j for j in range(start, i) if swings[j]]
    if len(pts) < 2:
        return False
    for a in range(len(pts)):
        for b in range(a + 1, len(pts)):
            j1, j2 = pts[a], pts[b]
            if i - j2 > recent:
                continue
            if j2 - j1 < min_sep:
                continue
            v1 = bars[j1]['l'] if use_low else bars[j1]['h']
            v2 = bars[j2]['l'] if use_low else bars[j2]['h']
            if abs(v1 - v2) / max(v1, v2) <= tol:
                return True
    return False


def backtest(bars, mode, window=10, LB=72, tol=0.005, recent=24, min_sep=8, k=2):
    sw_low, sw_high = compute_swings(bars, k)
    n = len(bars)
    initial = 10000.0
    capital = initial
    position = 0
    entry_price = 0.0
    entry_time = None
    entry_idx = 0
    trades = []
    recent_win = []

    for i in range(1, n):
        cur = bars[i]
        if cur['ztop'] is not None:
            inter = cur['l'] <= cur['ztop'] and cur['h'] >= cur['zbottom']
        else:
            inter = False
        recent_win.append(1 if inter else 0)
        if len(recent_win) > window:
            recent_win.pop(0)

        if position != 0:
            exit_reason = None
            exit_price = cur['c']
            exit_time = cur['t']
            bars_held = i - entry_idx
            if len(recent_win) == window and sum(recent_win) / window >= 0.5:
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
            if prev['mid'] is not None and cur['mid'] is not None:
                sig = 0
                if prev['c'] < prev['mid'] and cur['c'] > cur['mid']:
                    sig = 1
                elif prev['c'] > prev['mid'] and cur['c'] < cur['mid']:
                    sig = -1
                if sig != 0 and mode == 'DB':
                    # 做多需双底，做空需双顶
                    if sig == 1 and not has_pattern(bars, sw_low, i, LB, tol, recent, min_sep, True):
                        sig = 0
                    if sig == -1 and not has_pattern(bars, sw_high, i, LB, tol, recent, min_sep, False):
                        sig = 0
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
    dd = 0.0
    for c in curve:
        peak = max(peak, c)
        dd = min(dd, (c - peak) / peak * 100)
    return {'n': n, 'win': wins / n * 100, 'compound': (cap - initial) / initial * 100, 'dd': dd,
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


def verify_examples(bars, sw_low, sw_high):
    """检查用户3个示例信号是否被双底/双顶检测捕获"""
    tg = [('2026-08-03 22:00', '做多', '双底'),
          ('2026-08-10 21:00', '做空', '双顶'),
          ('2026-08-17 10:00', '做多', '双底')]
    print("\n===== 用户示例验证（tol=0.5%, LB=72, recent=24） =====")
    for t, direc, pat in tg:
        i = next((j for j, b in enumerate(bars) if b['t'].startswith(t)), None)
        if i is None:
            print(f"  {t}: 未找到")
            continue
        up = bars[i-1]['c'] < bars[i-1]['mid'] and bars[i]['c'] > bars[i]['mid']
        dn = bars[i-1]['c'] > bars[i-1]['mid'] and bars[i]['c'] < bars[i]['mid']
        use_low = (pat == '双底')
        hit = has_pattern(bars, sw_low if use_low else sw_high, i, 72, 0.005, 24, 8, use_low)
        print(f"  {t} {direc}: 穿越={up or dn} | {pat}检测={hit}")


def main():
    bars = load_1h('BTC')
    bars = indicators(bars)
    bars = precompute(bars)
    sw_low, sw_high = compute_swings(bars, 2)
    verify_examples(bars, sw_low, sw_high)

    symbols = ['BTC', 'ETH', 'SOL', 'BNB', 'SUI', 'HYPE']
    configs = [('RAW', 10, 72, 0.005), ('DB', 10, 72, 0.005), ('DB', 20, 72, 0.005), ('DB', 10, 96, 0.008)]
    rows = []
    print(f"\n{'='*100}\n双底/双顶过滤回测（DB=需形态确认；RAW=无）\n{'='*100}")
    print(f"{'币种':<6}{'模式':<4}{'窗口':>5}{'笔数':>7}{'胜率':>8}{'复利':>10}{'回撤':>9}{'taker后':>10} | 2024 / 2025 / 2026")
    for sym in symbols:
        bars = load_1h(sym)
        bars = indicators(bars)
        bars = precompute(bars)
        for mode, w, LB, tol in configs:
            trades, init, _ = backtest(bars, mode, w, LB=LB, tol=tol)
            st = stats(trades, init)
            if st is None:
                print(f"{sym:<6}{mode:<4}{w:>5} 无交易")
                continue
            yrs = yearly(trades)
            fee = with_fee(trades)
            c = " / ".join(f"{y}:{yrs.get(y, 0):.0f}%" for y in (2024, 2025, 2026))
            tag = f"{mode}(LB{'' if mode=='RAW' else LB},tol{'%.1f'%(tol*100) if mode=='DB' else ''}%)" if mode == 'DB' else mode
            print(f"{sym:<6}{tag:<4}{w:>5}{st['n']:>7}{st['win']:>7.1f}%{st['compound']:>9.1f}%{st['dd']:>8.1f}%{fee:>9.1f}% | {c}")
            rows.append({'symbol': sym, 'mode': mode, 'window': w, 'LB': LB, 'tol': tol,
                         'trades': st['n'], 'win_rate': round(st['win'], 1),
                         'compound': round(st['compound'], 1), 'max_dd': round(st['dd'], 1), 'fee': round(fee, 1),
                         'y2024': round(yrs.get(2024, 0), 1), 'y2025': round(yrs.get(2025, 0), 1),
                         'y2026': round(yrs.get(2026, 0), 1)})

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'feature_report',
                       'strategy_d_double_pattern_results.csv')
    with open(out, 'w', encoding='utf-8-sig', newline='') as f:
        w_ = csv.DictWriter(f, fieldnames=list(rows[0].keys()))
        w_.writeheader()
        w_.writerows(rows)
    print(f"\n结果已保存: {out}")


if __name__ == '__main__':
    main()
