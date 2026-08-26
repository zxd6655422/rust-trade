#!/usr/bin/env python3
"""观察用户举例的双底/双顶形态：精确高低点与间隔"""
import sys
sys.path.insert(0, '.')
from strategy_d_feasibility import load_1h, indicators

bars = load_1h('BTC')
bars = indicators(bars)
n = len(bars)

def idx_at(t):
    for i, b in enumerate(bars):
        if b['t'].startswith(t):
            return i
    return None

def dump_range(t0, t1, label):
    i0, i1 = idx_at(t0), idx_at(t1)
    if i0 is None or i1 is None:
        print(f"{label}: 未找到 {t0} 或 {t1}")
        return
    print(f"\n===== {label} ({t0} ~ {t1}) =====")
    lo = min(bars[i0:i1+1], key=lambda b: b['l'])
    hi = max(bars[i0:i1+1], key=lambda b: b['h'])
    print(f"  区间最低: {lo['t'][:16]} low={lo['l']:.1f}")
    print(f"  区间最高: {hi['t'][:16]} high={hi['h']:.1f}")
    print(f"{'时间':<17}{'低':>10}{'高':>10}{'收':>10}")
    for i in range(i0, i1+1):
        b = bars[i]
        mark = ''
        if b['l'] == lo['l']:
            mark = ' <== 区间最低'
        if b['h'] == hi['h']:
            mark = ' <== 区间最高'
        print(f"{b['t'][:16]:<17}{b['l']:>10.1f}{b['h']:>10.1f}{b['c']:>10.1f}{mark}")

# 用户举例的三处
print("="*90)
print("【0817 做多 的双底】：0814 21:00~22:00 底1  vs  0817 05:00~08:00 底2")
dump_range('2026-08-14 20:00', '2026-08-14 23:00', '底1 (0814)')
dump_range('2026-08-17 04:00', '2026-08-17 09:00', '底2 (0817)')

print("="*90)
print("【0810 做空 的双顶】：0807 20:00附近 顶1  vs  0810 06:00附近 顶2")
dump_range('2026-08-07 18:00', '2026-08-07 22:00', '顶1 (0807)')
dump_range('2026-08-10 04:00', '2026-08-10 08:00', '顶2 (0810)')

print("="*90)
print("【0803 做多 的双/三底】：0731 23:00附近 底1 / 0802 02:00附近 底2 / 0803 16:00附近 底3")
dump_range('2026-07-31 21:00', '2026-08-01 01:00', '底1 (0731)')
dump_range('2026-08-02 00:00', '2026-08-02 04:00', '底2 (0802)')
dump_range('2026-08-03 14:00', '2026-08-03 18:00', '底3 (0803)')
