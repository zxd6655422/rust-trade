#!/usr/bin/env python3
"""
方案D E6/E4 入场市场状态分析（纯标准库）

回答两个问题：
  Q1: 横盘震荡中的入场有多少？（E6 虽有10根同侧确认，横盘大区间仍可能触发）
  Q2: 盈利单 vs 亏损单入场时的区别？（zone宽度/MA48斜率/2日振幅/量比/持仓）

横盘定义（入场时，绝对阈值）：
  A 窄带   : zone_width_pct < 0.5%（MA48与中轨间隙<0.5%价格）
  B 平斜率 : |ma48_slope_24| < 0.1%（24h内MA48几乎水平）
  C 强横盘 : A 且 B
其余 = 趋势入场（对照）
"""
import os
import sys
from statistics import median

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from strategy_d_feasibility import load_1h, indicators, compute_trend_flags


def precompute(bars):
    """为每根K线附加市场状态特征"""
    n = len(bars)
    closes = [b['c'] for b in bars]
    highs = [b['h'] for b in bars]
    lows = [b['l'] for b in bars]
    vols = [b.get('v', 0.0) for b in bars]

    for i in range(n):
        b = bars[i]
        # zone 宽度（占价格%）
        if b['ztop'] is not None:
            b['zw_pct'] = (b['ztop'] - b['zbottom']) / b['c'] * 100
        else:
            b['zw_pct'] = None
        # MA48 24h 斜率
        if i >= 24 and b['ma48'] is not None and bars[i - 24]['ma48']:
            b['slope24'] = (b['ma48'] - bars[i - 24]['ma48']) / bars[i - 24]['ma48'] * 100
        else:
            b['slope24'] = None
        # 2日(48根)振幅
        if i >= 47:
            hi = max(highs[i - 47:i + 1])
            lo = min(lows[i - 47:i + 1])
            b['range48'] = (hi - lo) / b['c'] * 100
        else:
            b['range48'] = None
        # 量比（48根均量）
        if i >= 47:
            avg_v = sum(vols[i - 47:i + 1]) / 48
            b['vol_ratio'] = vols[i] / avg_v if avg_v > 0 else 1.0
        else:
            b['vol_ratio'] = None
    return bars


def backtest_with_state(bars, entry_mode, window=10):
    """回测并记录每笔交易的入场特征。返回 trades 列表。"""
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
                    'pnl_pct': pnl_pct, 'pnl_amount': pnl_amount, 'bars_held': bars_held,
                    'year': int(entry_time[:4]),
                    # 入场时市场状态（zone/斜率/振幅用入场bar；趋势状态用触发信号的前一根bar）
                    'zw_pct': bars[entry_idx]['zw_pct'],
                    'slope24': bars[entry_idx]['slope24'],
                    'range48': bars[entry_idx]['range48'],
                    'vol_ratio': bars[entry_idx]['vol_ratio'],
                    'trend_up': bars[entry_idx - 1]['trend_up'],
                    'trend_down': bars[entry_idx - 1]['trend_down'],
                })
                position = 0
                entry_price = 0.0
                entry_time = None

        if position == 0:
            prev = bars[i - 1]
            if prev['ztop'] is None or cur['ztop'] is None:
                sig = 0
            # 复用 strategy_d_feasibility 的入场逻辑
            elif entry_mode == 'E6':
                if prev['trend_up'] and prev['c'] > prev['ztop'] and cur['c'] < cur['ztop'] and cur['c'] > cur['zbottom']:
                    sig = 1
                elif prev['trend_down'] and prev['c'] < prev['zbottom'] and cur['c'] > cur['zbottom'] and cur['c'] < cur['ztop']:
                    sig = -1
                else:
                    sig = 0
            else:  # E4
                if prev['c'] > prev['ztop'] and cur['c'] < cur['ztop'] and cur['c'] > cur['zbottom']:
                    sig = 1
                elif prev['c'] < prev['zbottom'] and cur['c'] > cur['zbottom'] and cur['c'] < cur['ztop']:
                    sig = -1
                else:
                    sig = 0
            if sig != 0:
                entry_price = cur['c']
                entry_time = cur['t']
                entry_idx = i
                position = sig

    return trades, initial, capital


def is_sideways(b):
    """返回 (窄带, 平斜率, 强横盘) 三个布尔"""
    if b['zw_pct'] is None or b['slope24'] is None:
        return False, False, False
    a = b['zw_pct'] < 0.5
    bb = abs(b['slope24']) < 0.1
    return a, bb, (a and bb)


def fmt(x, nd=2):
    return '-' if x is None else f"{x:.{nd}f}"


def analyze(symbol):
    bars = load_1h(symbol)
    bars = indicators(bars)
    bars = compute_trend_flags(bars)
    bars = precompute(bars)

    print(f"\n{'='*96}\n{symbol} | {len(bars)} 根1hK线\n{'='*96}")

    for mode in ['E6', 'E4']:
        trades, _, _ = backtest_with_state(bars, mode, 10)
        n = len(trades)
        if n == 0:
            print(f"\n[{mode}] 无交易")
            continue
        wins = [t for t in trades if t['pnl_pct'] > 0]
        losses = [t for t in trades if t['pnl_pct'] <= 0]
        cap = 10000.0
        for t in trades:
            cap += t['pnl_amount']
        print(f"\n[{mode}] 共 {n} 笔 | 胜率 {len(wins)/n*100:.1f}% | 复利 {(cap-10000)/10000*100:.1f}%")

        # Q1: 入场时市场状态分布
        tu = sum(1 for t in trades if t['trend_up'])
        td = sum(1 for t in trades if t['trend_down'])
        print(f"  入场时10根状态: 上方趋势 {tu} ({tu/n*100:.0f}%) | 下方趋势 {td} ({td/n*100:.0f}%) | 其他 {n-tu-td}")

        # 横盘统计
        for label, idx in [('窄带(<0.5%)', 0), ('平斜率(<0.1%)', 1), ('强横盘(双条件)', 2)]:
            side = [t for t in trades if is_sideways(t)[idx]]
            other = [t for t in trades if not is_sideways(t)[idx]]
            if not side:
                print(f"  横盘[{label}]: 0 笔")
                continue
            sw = sum(1 for t in side if t['pnl_pct'] > 0) / len(side) * 100
            ow = sum(1 for t in other if t['pnl_pct'] > 0) / len(other) * 100 if other else 0
            sp = sum(t['pnl_pct'] for t in side) / len(side)
            op = sum(t['pnl_pct'] for t in other) / len(other) if other else 0
            print(f"  横盘[{label}]: {len(side)} 笔 ({len(side)/n*100:.0f}%) | 胜率 {sw:.1f}% 平均盈亏 {sp:+.2f}% "
                  f"| 对照(非横盘): 胜率 {ow:.1f}% 平均盈亏 {op:+.2f}%")

        # Q2: 胜 vs 亏 入场特征
        print("  胜 vs 亏 入场特征 (均值 / 中位数):")
        feats = [('zone宽度%', 'zw_pct'), ('MA48斜率%', 'slope24'), ('2日振幅%', 'range48'),
                 ('量比', 'vol_ratio'), ('持仓根数', 'bars_held')]
        print(f"    {'特征':<10}{'盈利均值':>10}{'盈利中位':>10}{'亏损均值':>10}{'亏损中位':>10}")
        for label, key in feats:
            wv = [t[key] for t in wins if t[key] is not None]
            lv = [t[key] for t in losses if t[key] is not None]
            if not wv or not lv:
                continue
            print(f"    {label:<10}{sum(wv)/len(wv):>10.3f}{median(wv):>10.3f}"
                  f"{sum(lv)/len(lv):>10.3f}{median(lv):>10.3f}")


def main():
    for sym in ['BTC', 'ETH', 'SOL', 'BNB', 'SUI', 'HYPE']:
        try:
            analyze(sym)
        except Exception as e:
            print(f"{sym} 失败: {e}")


if __name__ == '__main__':
    main()
