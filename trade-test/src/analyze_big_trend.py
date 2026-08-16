"""大趋势空间 + 亏损单大周期/小周期维度分析。

Part A：盈利单（移动止盈）离场后，未来 30/90/180/365 天的最高价涨幅
        —— 量化「大趋势没拿住的空间」。
Part B：亏损单（MA288止损/硬止损）在「大周期（日线）维度」的状态分布
        —— 日线趋势方向、价格相对日线 MA20/MA60 的位置、日线 RSI。

口径：对齐生产（slow=480 + vol过滤 + 退出链全开）。
输出：feature_report/big_trend_report.md
"""
from __future__ import annotations

import math
import os
from typing import List, Dict, Any, Optional

import data_config as dc
from loader import load_klines_30m


def realized_vol_48_series(closes):
    n = len(closes)
    rets = [0.0] * n
    for i in range(1, n):
        if closes[i - 1] != 0.0:
            rets[i] = closes[i] / closes[i - 1] - 1.0
    p = [0.0] * (n + 1)
    p2 = [0.0] * (n + 1)
    for i in range(n):
        p[i + 1] = p[i] + rets[i]
        p2[i + 1] = p2[i] + rets[i] * rets[i]
    W = 48
    out = [None] * n
    for i in range(W, n):
        mean = (p[i + 1] - p[i + 1 - W]) / W
        msq = (p2[i + 1] - p2[i + 1 - W]) / W
        var = msq - mean * mean
        if var < 0.0:
            var = 0.0
        out[i] = math.sqrt(var) * 100.0
    return out


def backtest_full(symbol, params, bars):
    fast = params.fast_ma_period
    slow = params.slow_ma_period
    n = len(bars)
    closes = [b.close for b in bars]
    highs = [b.high for b in bars]
    lows = [b.low for b in bars]
    vol48 = realized_vol_48_series(closes)
    prefix = [0.0] * (n + 1)
    for i in range(n):
        prefix[i + 1] = prefix[i] + closes[i]

    def sma_at(idx, period):
        if idx + 1 < period:
            return None
        return (prefix[idx + 1] - prefix[idx + 1 - period]) / period

    trades = []
    pos = None
    for i in range(n):
        if i + 1 < slow:
            continue
        close = closes[i]
        prev_close = closes[i - 1]
        fast_ma = sma_at(i, fast)
        slow_ma = sma_at(i, slow)
        prev_fast_ma = sma_at(i - 1, fast)

        if pos is not None:
            bar = bars[i]
            side = pos["side"]
            entry = pos["entry_price"]
            pnl = (close - entry) / entry * 100.0 if side == "LONG" else (entry - close) / entry * 100.0
            pos["max_profit"] = max(pos["max_profit"], pnl)
            exit_price = None
            reason = ""
            if params.hard_stop_pct > 0.0:
                if side == "LONG" and bar.low <= pos["hard_stop_price"]:
                    exit_price, reason = pos["hard_stop_price"], "硬止损"
                elif side == "SHORT" and bar.high >= pos["hard_stop_price"]:
                    exit_price, reason = pos["hard_stop_price"], "硬止损"
            if exit_price is None and prev_fast_ma is not None:
                if side == "LONG" and prev_close > prev_fast_ma and close < fast_ma:
                    exit_price, reason = close, "MA288止损"
                elif side == "SHORT" and prev_close < prev_fast_ma and close > fast_ma:
                    exit_price, reason = close, "MA288止损"
            if exit_price is None and params.take_profit_mode == "trailing":
                if pos["max_profit"] >= params.trailing_activate_pct:
                    if pos["max_profit"] - pnl >= params.trailing_callback_pct:
                        exit_price, reason = close, "移动止盈"
            if exit_price is None:
                if side == "LONG" and fast_ma < slow_ma:
                    exit_price, reason = close, "趋势反转"
                elif side == "SHORT" and fast_ma > slow_ma:
                    exit_price, reason = close, "趋势反转"
            if exit_price is not None:
                ret = (exit_price - entry) / entry if side == "LONG" else (entry - exit_price) / entry
                trades.append({"side": side, "reason": reason, "ret_pct": ret * 100.0,
                               "entry": entry, "exit": exit_price, "exit_idx": i,
                               "entry_idx": pos["entry_idx"]})
                pos = None
                continue

        if pos is None and fast_ma is not None and slow_ma is not None and prev_fast_ma is not None:
            if vol48[i] is not None and vol48[i] >= params.realized_vol_threshold:
                continue
            if fast_ma > slow_ma and prev_close < prev_fast_ma and close > fast_ma:
                hs = close * (1.0 - params.hard_stop_pct / 100.0)
                pos = {"side": "LONG", "entry_price": close, "entry_idx": i, "hard_stop_price": hs, "max_profit": 0.0}
            elif fast_ma < slow_ma and prev_close > prev_fast_ma and close < fast_ma:
                hs = close * (1.0 + params.hard_stop_pct / 100.0)
                pos = {"side": "SHORT", "entry_price": close, "entry_idx": i, "hard_stop_price": hs, "max_profit": 0.0}

    if pos is not None:
        ret = (closes[-1] - pos["entry_price"]) / pos["entry_price"] if pos["side"] == "LONG" else (pos["entry_price"] - closes[-1]) / pos["entry_price"]
        trades.append({"side": pos["side"], "reason": "持仓到结束", "ret_pct": ret * 100.0,
                       "entry": pos["entry_price"], "exit": closes[-1], "exit_idx": n - 1, "entry_idx": pos["entry_idx"]})
    return trades, highs, lows


def median(xs):
    if not xs:
        return float('nan')
    s = sorted(xs)
    m = len(s) // 2
    return s[m] if len(s) % 2 else (s[m - 1] + s[m]) / 2.0


def main() -> int:
    md: List[str] = []
    add = md.append
    add("# 大趋势空间 + 亏损单大周期维度分析")
    add("")

    DAY_BARS = 48  # 30m 一天 48 根
    HORIZONS = [(30, "30天"), (90, "90天"), (180, "180天"), (365, "365天")]

    for coin in ["BTCUSDT", "ETHUSDT"]:
        params = dc.SYMBOL_PARAMS[coin]
        bars = load_klines_30m(coin)
        n = len(bars)
        ts, highs, lows = backtest_full(coin, params, bars)
        wins = [t for t in ts if t["reason"] == "移动止盈"]

        add(f"## {coin}")
        add("")

        # Part A: 大趋势空间
        add(f"### A. 盈利单离场后的大趋势空间（{len(wins)} 笔盈利单）")
        add("")
        add(f"- 平均已拿到 +{sum(t['ret_pct'] for t in wins)/len(wins):.2f}%（策略止盈离场时的收益）")
        add("")
        add("| 离场后时间 | 平均最高涨幅 | 中位最高涨幅 | 对比已拿到 |")
        add("|---|---|---|---|")
        for days, lab in HORIZONS:
            hb = days * DAY_BARS
            vals = []
            for t in wins:
                ei = t["exit_idx"]
                if ei + 1 >= n:
                    continue
                end = min(n, ei + 1 + hb)
                if t["side"] == "LONG":
                    fh = max(highs[ei + 1: end])
                    v = (fh - t["exit"]) / t["exit"] * 100.0
                else:
                    fl = min(lows[ei + 1: end])
                    v = (t["exit"] - fl) / t["exit"] * 100.0
                vals.append(v)
            avg_ret = sum(t['ret_pct'] for t in wins) / len(wins)
            add(f"| {lab} | +{sum(vals)/len(vals):.2f}% | +{median(vals):.2f}% | 策略已拿 +{avg_ret:.2f}% |")
        add("")
        add("> 解读：若「离场后 90/180/365 天最高涨幅」远大于「已拿到」，说明策略只吃了小波段，大趋势的空间被移动止盈提前让掉了。")
        add("")

        # Part B: 亏损单的大周期（日线）维度
        add("### B. 亏损单的大周期（日线）维度")
        add("")
        # 重采样 30m -> 日线（每天收盘价）
        from collections import defaultdict
        import datetime
        daily = defaultdict(list)
        for b in bars:
            dt = datetime.datetime.fromtimestamp(b.open_time / 1000).date()
            daily[dt].append(b.close)
        daily_closes = [daily[d][-1] for d in sorted(daily)]
        # 日线 MA20 / MA60
        def sma_list(vals, period):
            p = [0.0]
            for v in vals:
                p.append(p[-1] + v)
            out = []
            for i in range(len(vals)):
                if i + 1 >= period:
                    out.append((p[i + 1] - p[i + 1 - period]) / period)
                else:
                    out.append(None)
            return out
        dma20 = sma_list(daily_closes, 20)
        dma60 = sma_list(daily_closes, 60)
        # 每日 index 映射（用 open_time 的日期）
        dates = sorted(daily)
        date_to_idx = {d: i for i, d in enumerate(dates)}

        losses = [t for t in ts if t["reason"] in ("MA288止损", "硬止损")]
        # 亏损单入场日线状态
        add("| 维度 | 亏损单均值 | 说明 |")
        add("|---|---|---|")
        # 日线趋势：close vs MA20 vs MA60
        def daily_state(t, key):
            ei = t["entry_idx"]
            dt = datetime.datetime.fromtimestamp(bars[ei].open_time / 1000).date()
            if dt not in date_to_idx:
                return None
            di = date_to_idx[dt]
            c = daily_closes[di]
            m20 = dma20[di]
            m60 = dma60[di]
            if m20 is None or m60 is None:
                return None
            if key == "close_vs_ma20":
                return (c - m20) / m20 * 100.0
            if key == "close_vs_ma60":
                return (c - m60) / m60 * 100.0
            if key == "ma20_vs_ma60":
                return (m20 - m60) / m60 * 100.0
            return None
        for key, label in [("close_vs_ma20", "日线收盘 vs 日线MA20"), ("close_vs_ma60", "日线收盘 vs 日线MA60"), ("ma20_vs_ma60", "日线MA20 vs MA60（趋势方向）")]:
            vals = [daily_state(t, key) for t in losses]
            vals = [v for v in vals if v is not None]
            if vals:
                add(f"| {label} | {sum(vals)/len(vals):+.3f}% | {'正=在上方/多头' if 'MA20' in label else ''} |")
        add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "big_trend_report.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
