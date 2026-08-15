"""方向5：延迟入场（走势确认）+ 位置维度 组合过滤。

信号触发 → 延迟 N 根观察走势 → 确认时需同时满足：
  1) 浮盈 >= X%（走势走好）
  2) 位置不在极端（position_in_range_96 在合理区间，避免追顶/追底）

对照：无延迟、纯延迟、纯位置，看组合能否更有效过滤亏损单、保留盈利单。

输出：feature_report/delayed_position_report.md
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


def position_in_range_series(highs, lows, closes, window=96):
    """当前收盘价在近 window 根 bar 高-低区间中的百分位（0=底，100=顶）。"""
    n = len(closes)
    out = [None] * n
    # 单调队列求滚动 max/min，O(n)
    from collections import deque
    mxq = deque()
    mnq = deque()
    for i in range(n):
        while mxq and mxq[0] <= i - window:
            mxq.popleft()
        while mnq and mnq[0] <= i - window:
            mnq.popleft()
        while mxq and highs[mxq[-1]] <= highs[i]:
            mxq.pop()
        while mnq and lows[mnq[-1]] >= lows[i]:
            mnq.pop()
        mxq.append(i)
        mnq.append(i)
        if i >= window - 1:
            hi = highs[mxq[0]]
            lo = lows[mnq[0]]
            rng = hi - lo
            if rng > 0:
                out[i] = (closes[i] - lo) / rng * 100.0
    return out


def backtest(symbol, params, bars, pos_series, delay_n, min_gain, pos_lo, pos_hi):
    fast = params.fast_ma_period
    slow = params.slow_ma_period
    n = len(bars)
    closes = [b.close for b in bars]
    highs = [b.high for b in bars]
    lows = [b.low for b in bars]
    vol48 = realized_vol_48_series(closes) if params.realized_vol_threshold > 0.0 else None
    prefix = [0.0] * (n + 1)
    for i in range(n):
        prefix[i + 1] = prefix[i] + closes[i]

    def sma_at(idx, period):
        if idx + 1 < period:
            return None
        return (prefix[idx + 1] - prefix[idx + 1 - period]) / period

    def open_pos(side, entry, idx):
        hs = entry * (1.0 - params.hard_stop_pct / 100.0) if side == "LONG" else entry * (1.0 + params.hard_stop_pct / 100.0)
        return {"side": side, "entry_price": entry, "entry_idx": idx, "hard_stop_price": hs, "max_profit": 0.0}

    trades = []
    pos = None
    pending = None

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
            if exit_price is None and params.stop_mode == "ma288" and prev_fast_ma is not None:
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
                trades.append({"ret": ret, "ret_pct": ret * 100.0, "reason": reason,
                               "entry": entry, "mfe": pos["max_profit"], "bars": i - pos["entry_idx"]})
                pos = None
                continue

        if pending is not None:
            p = pending
            trend_broke = (p["side"] == "LONG" and fast_ma < slow_ma) or (p["side"] == "SHORT" and fast_ma > slow_ma)
            price_broke = (p["side"] == "LONG" and prev_close > prev_fast_ma and close < fast_ma) or \
                          (p["side"] == "SHORT" and prev_close < prev_fast_ma and close > fast_ma)
            if trend_broke or price_broke:
                pending = None
            elif i - p["trigger_idx"] >= delay_n:
                gain = (close - p["ref_price"]) / p["ref_price"] * 100.0 if p["side"] == "LONG" else (p["ref_price"] - close) / p["ref_price"] * 100.0
                pos_val = pos_series[i]
                pos_ok = pos_val is None or (pos_lo <= pos_val <= pos_hi)
                if gain >= min_gain and pos_ok:
                    pos = open_pos(p["side"], close, i)
                pending = None

        if pos is None and pending is None and fast_ma is not None and slow_ma is not None and prev_fast_ma is not None:
            if vol48 is not None and vol48[i] is not None and vol48[i] >= params.realized_vol_threshold:
                continue
            sig = None
            if fast_ma > slow_ma and prev_close < prev_fast_ma and close > fast_ma:
                sig = "LONG"
            elif fast_ma < slow_ma and prev_close > prev_fast_ma and close < fast_ma:
                sig = "SHORT"
            if sig:
                if delay_n is None:
                    pos = open_pos(sig, close, i)
                else:
                    pending = {"side": sig, "ref_price": close, "trigger_idx": i}

    if pos is not None:
        ret = (closes[-1] - pos["entry_price"]) / pos["entry_price"] if pos["side"] == "LONG" else (pos["entry_price"] - closes[-1]) / pos["entry_price"]
        trades.append({"ret": ret, "ret_pct": ret * 100.0, "reason": "持仓到结束",
                       "entry": pos["entry_price"], "mfe": pos["max_profit"], "bars": n - 1 - pos["entry_idx"]})
    return trades


def metrics(ts):
    n = len(ts)
    if n == 0:
        return dict(n=0, wr=0, simple=0, comp=0, avg_loss=0, avg_win=0)
    wins = [t for t in ts if t["ret_pct"] > 0]
    losses = [t for t in ts if t["ret_pct"] <= 0]
    eq = 1.0
    for t in ts:
        eq *= (1.0 + t["ret"])
    return dict(
        n=n, wr=len(wins) / n * 100,
        simple=sum(t["ret_pct"] for t in ts), comp=(eq - 1.0) * 100,
        avg_loss=(sum(t["ret_pct"] for t in losses) / len(losses)) if losses else 0.0,
        avg_win=(sum(t["ret_pct"] for t in wins) / len(wins)) if wins else 0.0,
    )


def main() -> int:
    md: List[str] = []
    add = md.append
    add("# 方向5：延迟入场 + 位置维度 组合")
    add("")
    add("- 延迟 N 根 + 浮盈 >= X% + position_in_range_96 在 [lo,hi] 区间，三者都满足才入场。")
    add("")

    COMBOS = [
        (None, 0.0, 0, 100, "基线(立即)"),
        (2, 0.0, 0, 100, "延迟2根≥0%,无位置"),
        (2, 0.0, 25, 75, "延迟2根≥0%,中继区[25,75]"),
        (2, 0.0, 0, 75, "延迟2根≥0%,避开顶部[0,75]"),
        (3, 0.0, 0, 100, "延迟3根≥0%,无位置"),
        (3, 0.0, 25, 75, "延迟3根≥0%,中继区[25,75]"),
        (3, 0.0, 0, 75, "延迟3根≥0%,避开顶部[0,75]"),
        (2, 0.3, 0, 100, "延迟2根≥0.3%,无位置"),
        (2, 0.3, 25, 75, "延迟2根≥0.3%,中继区[25,75]"),
    ]

    for coin in ["BTCUSDT", "ETHUSDT"]:
        params = dc.SYMBOL_PARAMS[coin]
        bars = load_klines_30m(coin)
        pos_series = position_in_range_series([b.high for b in bars], [b.low for b in bars], [b.close for b in bars])
        add(f"## {coin}")
        add("")
        add("| 方案 | 交易数 | 胜率 | 简单收益 | 复利收益 | 平均亏损 | 平均盈利 |")
        add("|---|---|---|---|---|---|---|")
        for dn, mg, lo, hi, name in COMBOS:
            ts = backtest(coin, params, bars, pos_series, dn, mg, lo, hi)
            m = metrics(ts)
            add(f"| {name} | {m['n']} | {m['wr']:.1f}% | {m['simple']:+.2f}% | {m['comp']:+.2f}% | {m['avg_loss']:+.2f}% | {m['avg_win']:+.2f}% |")
        add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "delayed_position_report.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
