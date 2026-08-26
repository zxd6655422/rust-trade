#!/usr/bin/env python3
"""
方案D可行性研究（纯Python标准库实现，无pandas依赖）

策略描述（用户原话整理）：
  - 1h K线，指标 MA48 + BOLL(100, 2.0)
  - zone = MA48 与 BOLL中轨 之间的动态区间
  - 价格在 zone 外时，zone 构成支撑（上方）或压力（下方）
  - 价格穿过 zone 区域 -> 做多（下方向上穿，突破压力）或做空（上方向下穿，跌破支撑）
  - 滑动窗口内 >=50% K线与 zone 相交（被淹没）-> 平仓/止损

入场变体：
  E1 整根K线覆盖zone（h>=top 且 l<=bottom）且前一根收盘在zone下方 -> 多；上方 -> 空
  E2 收盘价完整穿越zone（prev_c<prev_bottom 且 c>top -> 多；prev_c>prev_top 且 c<bottom -> 空）
  E3 收盘价穿越zone边界（prev_c<prev_top 且 c>top -> 多；prev_c>prev_bottom 且 c<bottom -> 空）
出场：滑动窗口 W ∈ {10,20}，相交比例 >= 50% 平仓
输出：笔数/胜率/简单收益/复利/回撤/平均持仓/年度表现/含手续费
"""
import csv
import math
import os
import sys

DATA_DIR = r"D:\dev-projects\data_2026-08-13"


def load_1h(symbol):
    """加载1h K线，升序。列: symbol, open_time, open, high, low, close, volume, trade_count"""
    path = os.path.join(DATA_DIR, f"kline_1h_{symbol}.csv")
    bars = []
    with open(path, 'r', encoding='utf-8') as f:
        reader = csv.reader(f)
        next(reader)  # header
        for row in reader:
            if len(row) < 6:
                continue
            b = {
                't': row[1],
                'o': float(row[2]), 'h': float(row[3]),
                'l': float(row[4]), 'c': float(row[5]),
            }
            if len(row) > 6:
                b['v'] = float(row[6])
            bars.append(b)
    bars.sort(key=lambda b: b['t'])
    return bars


def sma(vals, window):
    out = [None] * len(vals)
    s = 0.0
    for i, v in enumerate(vals):
        s += v
        if i >= window:
            s -= vals[i - window]
        if i >= window - 1:
            out[i] = s / window
    return out


def rolling_std(vals, window, ddof=1):
    out = [None] * len(vals)
    for i in range(window - 1, len(vals)):
        seg = vals[i - window + 1:i + 1]
        m = sum(seg) / window
        var = sum((x - m) ** 2 for x in seg) / (window - ddof)
        out[i] = math.sqrt(var)
    return out


def indicators(bars):
    n = len(bars)
    closes = [b['c'] for b in bars]
    ma48 = sma(closes, 48)
    mid = sma(closes, 100)
    sd = rolling_std(closes, 100)
    up = [mid[i] + 2.0 * sd[i] if mid[i] is not None else None for i in range(n)]
    lo = [mid[i] - 2.0 * sd[i] if mid[i] is not None else None for i in range(n)]
    for i, b in enumerate(bars):
        b['ma48'] = ma48[i]
        b['mid'] = mid[i]
        b['up'] = up[i]
        b['lo'] = lo[i]
        if ma48[i] is not None and mid[i] is not None:
            b['ztop'] = max(ma48[i], mid[i])
            b['zbottom'] = min(ma48[i], mid[i])
        else:
            b['ztop'] = b['zbottom'] = None
    return bars


def intersects(b):
    """K线与zone是否相交（重叠）"""
    if b['ztop'] is None:
        return False
    return b['l'] <= b['ztop'] and b['h'] >= b['zbottom']


def spans_zone(b):
    """整根K线覆盖整个zone"""
    if b['ztop'] is None:
        return False
    return b['h'] >= b['ztop'] and b['l'] <= b['zbottom']


def compute_trend_flags(bars, n=10):
    """每根K线：之前n根收盘是否全部在zone上方/下方（含本根）"""
    for i in range(len(bars)):
        b = bars[i]
        up = True
        down = True
        for j in range(max(0, i - n + 1), i + 1):
            bj = bars[j]
            if bj['ztop'] is None:
                up = down = False
                break
            if bj['c'] <= bj['ztop']:
                up = False
            if bj['c'] >= bj['zbottom']:
                down = False
        b['trend_up'] = up
        b['trend_down'] = down
    return bars


def entry_signal(prev, cur, mode):
    """返回 1=做多, -1=做空, 0=无信号"""
    if cur['ztop'] is None or prev['ztop'] is None:
        return 0
    if mode == 'E1':
        if spans_zone(cur) and prev['c'] < prev['zbottom']:
            return 1
        if spans_zone(cur) and prev['c'] > prev['ztop']:
            return -1
    elif mode == 'E2':
        if prev['c'] < prev['zbottom'] and cur['c'] > cur['ztop']:
            return 1
        if prev['c'] > prev['ztop'] and cur['c'] < cur['zbottom']:
            return -1
    elif mode == 'E3':
        if prev['c'] < prev['ztop'] and cur['c'] > cur['ztop']:
            return 1
        if prev['c'] > prev['zbottom'] and cur['c'] < cur['zbottom']:
            return -1
    elif mode == 'E4':
        # 回踩读法：价格在zone上方，本根K线下穿进zone（触碰支撑，未跌破）-> 多
        if prev['c'] > prev['ztop'] and cur['c'] < cur['ztop'] and cur['c'] > cur['zbottom']:
            return 1
        # 价格在zone下方，本根K线上穿进zone（触碰压力，未升破）-> 空
        if prev['c'] < prev['zbottom'] and cur['c'] > cur['zbottom'] and cur['c'] < cur['ztop']:
            return -1
    elif mode == 'E6':
        # E4 + 趋势确认：入场前10根收盘都在zone同侧
        if prev['trend_up'] and prev['c'] > prev['ztop'] and cur['c'] < cur['ztop'] and cur['c'] > cur['zbottom']:
            return 1
        if prev['trend_down'] and prev['c'] < prev['zbottom'] and cur['c'] > cur['zbottom'] and cur['c'] < cur['ztop']:
            return -1
    return 0


def run_backtest(bars, entry_mode, window, hard_stop_pct=0.0):
    """回测。返回 trades 列表 + 初始资金。"""
    n = len(bars)
    initial = 10000.0
    capital = initial
    position = 0
    entry_price = 0.0
    entry_time = None
    entry_idx = 0
    trades = []

    # 滑动窗口相交计数（deque 用列表+头指针简化）
    recent_intersect = []  # 0/1 flags，最多 window 个

    for i in range(1, n):
        cur = bars[i]

        # 维护滑动窗口
        recent_intersect.append(1 if intersects(cur) else 0)
        if len(recent_intersect) > window:
            recent_intersect.pop(0)

        if position != 0:
            exit_reason = None
            exit_price = cur['c']
            exit_time = cur['t']
            bars_held = i - entry_idx

            # 硬止损（可选）
            if hard_stop_pct > 0:
                if position == 1 and (cur['c'] - entry_price) / entry_price * 100 <= -hard_stop_pct:
                    exit_reason = 'hard_stop'
                elif position == -1 and (entry_price - cur['c']) / entry_price * 100 <= -hard_stop_pct:
                    exit_reason = 'hard_stop'

            # 被淹没出场：窗口内相交比例 >= 50%
            if exit_reason is None and len(recent_intersect) == window:
                ratio = sum(recent_intersect) / window
                if ratio >= 0.5:
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
                    'pnl_pct': pnl_pct, 'pnl_amount': pnl_amount,
                    'exit_reason': exit_reason, 'bars_held': bars_held,
                    'year': int(entry_time[:4]),
                })
                position = 0
                entry_price = 0.0
                entry_time = None

        if position == 0:
            sig = entry_signal(bars[i - 1], cur, entry_mode)
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
    return {
        'n': n, 'win_rate': wins / n * 100,
        'simple_pnl': sum(t['pnl_pct'] for t in trades),
        'compound': (cap - initial) / initial * 100,
        'max_dd': max_dd,
        'avg_hold': sum(t['bars_held'] for t in trades) / n,
    }


def yearly(trades, start=2024):
    out = {}
    for t in trades:
        y = t['year']
        if y >= start:
            out.setdefault(y, []).append(t)
    res = {}
    for y, ts in out.items():
        cap = 10000.0
        for t in ts:
            cap += t['pnl_amount']
        res[y] = {'n': len(ts), 'compound': (cap - 10000.0) / 10000.0 * 100}
    return res


def with_fee(trades, fee_per_round):
    cap = 10000.0
    for t in trades:
        cap += t['pnl_amount'] - cap * (fee_per_round / 100)
    return (cap - 10000.0) / 10000.0 * 100


def main():
    symbols = ['BTC', 'ETH', 'SOL', 'BNB', 'SUI', 'HYPE']
    windows = [10, 20]
    rows = []
    print("方案D可行性：1h K线 | MA48 + BOLL(100,2.0) | zone突破 + 50%相交平仓")
    print("=" * 120)

    for sym in symbols:
        bars = load_1h(sym)
        bars = indicators(bars)
        bars = compute_trend_flags(bars)
        print(f"\n===== {sym} | {len(bars)} 根1hK线 | {bars[0]['t'][:10]} ~ {bars[-1]['t'][:10]} =====")
        print(f"{'入场':<5}{'窗口':>5}{'笔数':>7}{'胜率':>8}{'简单收益':>9}{'复利':>9}{'回撤':>9}"
              f"{'持仓':>7}{'taker后':>9} | 2024 / 2025 / 2026")
        for em in ['E1', 'E2', 'E3', 'E4', 'E6']:
            for w in windows:
                trades, init, _ = run_backtest(bars, em, w)
                st = stats(trades, init)
                if st is None:
                    print(f"{em:<5}{w:>5}  无交易")
                    continue
                yrs = yearly(trades)
                fee1 = with_fee(trades, 0.10)
                cells = " / ".join(f"{y}:{yrs.get(y, {}).get('compound', 0):.0f}%({yrs.get(y, {}).get('n', 0)}笔)"
                                   for y in (2024, 2025, 2026))
                print(f"{em:<5}{w:>5}{st['n']:>7}{st['win_rate']:>7.1f}%{st['simple_pnl']:>8.1f}%"
                      f"{st['compound']:>8.1f}%{st['max_dd']:>8.1f}%{st['avg_hold']:>6.1f}根{fee1:>8.1f}% | {cells}")
                rows.append({'symbol': sym, 'entry': em, 'window': w,
                             'trades': st['n'], 'win_rate': round(st['win_rate'], 1),
                             'simple': round(st['simple_pnl'], 1), 'compound': round(st['compound'], 1),
                             'max_dd': round(st['max_dd'], 1), 'avg_hold': round(st['avg_hold'], 1),
                             'fee1': round(fee1, 1),
                             'y2024': round(yrs.get(2024, {}).get('compound', 0), 1),
                             'y2025': round(yrs.get(2025, {}).get('compound', 0), 1),
                             'y2026': round(yrs.get(2026, {}).get('compound', 0), 1)})

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'feature_report',
                       'strategy_d_results.csv')
    with open(out, 'w', encoding='utf-8-sig', newline='') as f:
        writer = csv.DictWriter(f, fieldnames=list(rows[0].keys()))
        writer.writeheader()
        writer.writerows(rows)
    print(f"\n结果已保存: {out}")


if __name__ == '__main__':
    main()
