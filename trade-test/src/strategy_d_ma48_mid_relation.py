#!/usr/bin/env python3
"""分析用户指定的三个时点附近 MA48 与 BOLL中轨 的关系"""
import sys
sys.path.insert(0, '.')
from strategy_d_feasibility import load_1h, indicators

bars = load_1h('BTC')
bars = indicators(bars)
n = len(bars)

# 关键时点（1h bar open_time）
targets = ['2026-08-03 10:00', '2026-08-10 21:00', '2026-08-17 10:00']

def near(t):
    return t[:13]

for tg in targets:
    idx = None
    for i, b in enumerate(bars):
        if b['t'].startswith(tg):
            idx = i
            break
    if idx is None:
        print(f"\n=== {tg} 未找到 ===\n")
        continue
    print(f"\n{'='*100}\n时点 {tg}  (bar #{idx})\n{'='*100}")
    print(f"{'时间':<17}{'收盘':>10}{'MA48':>10}{'BOLL中轨':>10}{'MA48-中轨':>10}{'收盘-中轨':>10}{'K线相对zone'}")
    for k in range(-6, 7):
        j = idx + k
        if j < 0 or j >= n:
            continue
        b = bars[j]
        ma48 = b['ma48']; mid = b['mid']
        if ma48 is None or mid is None:
            continue
        diff = ma48 - mid
        cd = b['c'] - mid
        # K线相对 zone（ma48~中轨）位置
        if b['c'] > b['ztop']:
            pos = '上方(zone=支撑)'
        elif b['c'] < b['zbottom']:
            pos = '下方(zone=压力)'
        else:
            pos = '带内(穿梭)'
        mark = ' <== 该时点' if k == 0 else ''
        print(f"{b['t'][:16]:<17}{b['c']:>10.1f}{ma48:>10.1f}{mid:>10.1f}{diff:>+10.1f}{cd:>+10.1f}  {pos}{mark}")

# 全序列 MA48 与中轨的交叉事件（最近几个）
print(f"\n{'='*100}\n最近 MA48 上穿/下穿 BOLL中轨 的事件（金叉=做多候选，死叉=做空候选）\n{'='*100}")
for i in range(1, n):
    a, b = bars[i-1], bars[i]
    if a['ma48'] is None or a['mid'] is None or b['ma48'] is None or b['mid'] is None:
        continue
    if a['ma48'] <= a['mid'] and b['ma48'] > b['mid']:
        if b['t'][:7] >= '2026-07':
            print(f"金叉(MA48上穿中轨): {b['t'][:16]}  收盘 {b['c']:.1f}  MA48={b['ma48']:.1f} 中轨={b['mid']:.1f}")
    if a['ma48'] >= a['mid'] and b['ma48'] < b['mid']:
        if b['t'][:7] >= '2026-07':
            print(f"死叉(MA48下穿中轨): {b['t'][:16]}  收盘 {b['c']:.1f}  MA48={b['ma48']:.1f} 中轨={b['mid']:.1f}")
