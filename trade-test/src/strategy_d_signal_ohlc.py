#!/usr/bin/env python3
"""三信号K线的完整OHLC vs MA48/中轨/zone 边界"""
import sys
sys.path.insert(0, '.')
from strategy_d_feasibility import load_1h, indicators

bars = load_1h('BTC')
bars = indicators(bars)

targets = ['2026-08-03 10:00', '2026-08-10 21:00', '2026-08-17 10:00']

def find(t):
    for i, b in enumerate(bars):
        if b['t'].startswith(t):
            return i
    return None

for tg in targets:
    idx = find(tg)
    b = bars[idx]
    print(f"\n===== {tg} =====")
    print(f"  OHLC: 开{b['o']:.1f} 高{b['h']:.1f} 低{b['l']:.1f} 收{b['c']:.1f}")
    print(f"  MA48={b['ma48']:.1f}  中轨={b['mid']:.1f}  上轨={b['up']:.1f}  下轨={b['lo']:.1f}")
    top, bot = b['ztop'], b['zbottom']
    print(f"  zone=[{bot:.1f}, {top:.1f}]  (MA48与中轨谁在上谁在下)")
    # K线相对zone
    if b['l'] <= top and b['h'] >= bot:
        span = '整根K线覆盖整个zone(穿过)' if (b['h'] >= top and b['l'] <= bot) else 'K线触及/部分穿过zone'
    else:
        span = 'K线完全在zone之外'
    print(f"  K线 vs zone: {span}")
    # 收盘相对中轨
    print(f"  收盘 vs 中轨: {'上方(+%.1f)' % (b['c']-b['mid']) if b['c'] >= b['mid'] else '下方(%.1f)' % (b['c']-b['mid'])}")
    # 前一根
    p = bars[idx-1]
    print(f"  前一根({p['t'][11:16]}): 收{p['c']:.1f}  相对zone: {'上方' if p['c']>p['ztop'] else ('下方' if p['c']<p['zbottom'] else '带内')}")
