#!/usr/bin/env python3
"""
验证用户规则A：做多=收盘上穿BOLL中轨，做空=收盘下穿BOLL中轨
并回测（出场沿用"滑动窗口内50%K线与ma48~中轨区域相交则平仓"）
"""
import os
import sys
import csv

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from strategy_d_feasibility import load_1h, indicators
from strategy_d_entry_state_analysis import precompute


def verify_signals(symbol):
    bars = load_1h(symbol)
    bars = indicators(bars)
    n = len(bars)
    targets = ['2026-08-03 22:00', '2026-08-10 21:00', '2026-08-17 10:00']
    print(f"\n===== {symbol} 信号验证（收盘 vs 中轨 穿越） =====")
    for tg in targets:
        for i in range(1, n):
            if bars[i]['t'].startswith(tg):
                b, p = bars[i], bars[i-1]
                up = (p['c'] < p['mid'] and b['c'] > b['mid'])
                dn = (p['c'] > p['mid'] and b['c'] < b['mid'])
                print(f"  {tg}: 前收{p['c']:.1f}(中轨{p['mid']:.1f}) -> 收{b['c']:.1f}(中轨{b['mid']:.1f}) | "
                      f"上穿={up} 下穿={dn}")
                break
        else:
            print(f"  {tg}: 未找到bar")


def backtest_mid_cross(bars, window=10, mode='RAW'):
    """入场=收盘穿越中轨；mode: RAW=全部穿越, REV=仅反转穿越(做多需MA48<中轨/做空需MA48>中轨), CONT=仅顺势穿越
    出场=滑动窗口内50%K线与ma48~中轨zone相交"""
    n = len(bars)
    initial = 10000.0
    capital = initial
    position = 0
    entry_price = 0.0
    entry_time = None
    entry_idx = 0
    trades = []
    recent = []

    for i in range(1, n):
        cur = bars[i]
        # 相交判断（ma48~中轨 zone）
        if cur['ztop'] is not None:
            inter = cur['l'] <= cur['ztop'] and cur['h'] >= cur['zbottom']
        else:
            inter = False
        recent.append(1 if inter else 0)
        if len(recent) > window:
            recent.pop(0)

        # 出场
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

        # 入场：收盘穿越中轨
        if position == 0:
            prev = bars[i-1]
            if prev['mid'] is not None and cur['mid'] is not None and cur['ma48'] is not None:
                sig = 0
                if prev['c'] < prev['mid'] and cur['c'] > cur['mid']:
                    sig = 1
                elif prev['c'] > prev['mid'] and cur['c'] < cur['mid']:
                    sig = -1
                if sig != 0 and mode == 'REV':
                    # 反转穿越：做多需 MA48<中轨（空头排列，埋伏金叉）；做空需 MA48>中轨（埋伏死叉）
                    if sig == 1 and not (cur['ma48'] < cur['mid']):
                        sig = 0
                    if sig == -1 and not (cur['ma48'] > cur['mid']):
                        sig = 0
                if sig != 0 and mode == 'CONT':
                    # 顺势穿越：做多需 MA48>中轨；做空需 MA48<中轨
                    if sig == 1 and not (cur['ma48'] > cur['mid']):
                        sig = 0
                    if sig == -1 and not (cur['ma48'] < cur['mid']):
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


def main():
    verify_signals('BTC')

    symbols = ['BTC', 'ETH', 'SOL', 'BNB', 'SUI', 'HYPE']
    rows = []
    print(f"\n{'='*100}\n规则A回测（RAW=全部穿越 / REV=反转穿越 / CONT=顺势穿越）\n{'='*100}")
    print(f"{'币种':<6}{'模式':<6}{'窗口':>5}{'笔数':>7}{'胜率':>8}{'复利':>10}{'回撤':>9}{'持仓':>7}{'taker后':>10} | 2024 / 2025 / 2026")
    for sym in symbols:
        bars = load_1h(sym)
        bars = indicators(bars)
        bars = precompute(bars)
        for mode in ('RAW', 'REV', 'CONT'):
            for w in (10, 20):
                trades, init, _ = backtest_mid_cross(bars, w, mode)
                st = stats(trades, init)
                if st is None:
                    print(f"{sym:<6}{mode:<6}{w:>5} 无交易")
                    continue
                yrs = yearly(trades)
                fee = with_fee(trades)
                c = " / ".join(f"{y}:{yrs.get(y, 0):.0f}%" for y in (2024, 2025, 2026))
                print(f"{sym:<6}{mode:<6}{w:>5}{st['n']:>7}{st['win']:>7.1f}%{st['compound']:>9.1f}%{st['dd']:>8.1f}%"
                      f"{st['avg_hold']:>6.1f}根{fee:>9.1f}% | {c}")
                rows.append({'symbol': sym, 'mode': mode, 'window': w, 'trades': st['n'],
                             'win_rate': round(st['win'], 1), 'compound': round(st['compound'], 1),
                             'max_dd': round(st['dd'], 1), 'avg_hold': round(st['avg_hold'], 1),
                             'fee': round(fee, 1),
                             'y2024': round(yrs.get(2024, 0), 1), 'y2025': round(yrs.get(2025, 0), 1),
                             'y2026': round(yrs.get(2026, 0), 1)})

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'feature_report',
                       'strategy_d_mid_cross_results.csv')
    with open(out, 'w', encoding='utf-8-sig', newline='') as f:
        w_ = csv.DictWriter(f, fieldnames=list(rows[0].keys()))
        w_.writeheader()
        w_.writerows(rows)
    print(f"\n结果已保存: {out}")


if __name__ == '__main__':
    main()
