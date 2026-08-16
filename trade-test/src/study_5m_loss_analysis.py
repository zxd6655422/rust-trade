"""5m 亏损研究：能否减少亏损、保住盈利单。

统计手段：
  1) 止损单 MFE 分桶（入场即错 vs 涨过回撤）
  2) 盈利单 vs 止损单的逐 bar 浮盈路径（第 k 根中位数）
  3) 条件概率 P(最终盈利 | 第 k 根浮盈区间)
  4) 时间止损规则净效果（第 k 根浮盈 < X% 则离场）

口径：5m 最优参数 + 最优阈值（BEST）。
输出：feature_report/5m_loss_analysis.md
"""
from __future__ import annotations

import csv
import math
import os
from collections import defaultdict
from datetime import datetime
from typing import List, Dict, Any, Optional

from ma_trend_pullback import KlineBar, Params

BASE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DATA = os.path.join(BASE, "data_2026-08-13")

CSV_5M = {
    "BTCUSDT": "kline_5m_202608131243_BTC.csv",
    "ETHUSDT": "kline_5m_202608131246_ETH.csv",
    "SOLUSDT": "kline_5m_202608131248_SOL.csv",
    "BNBUSDT": "kline_5m_202608141531_BNB.csv",
    "SUIUSDT": "kline_5m_202608141535_SUI.csv",
    "HYPEUSDT": "kline_5m_202608141538_HYPE.csv",
}

BEST = {
    "BTCUSDT": (0.5, 1.0, 1.5, 0.143),
    "ETHUSDT": (1.0, 1.0, 1.5, 0.356),
    "SOLUSDT": (0.5, 1.0, 1.0, 0.369),
    "BNBUSDT": (0.5, 1.0, 1.5, 0.0),
    "SUIUSDT": (1.0, 2.0, 1.5, 0.568),
    "HYPEUSDT": (0.5, 3.0, 1.0, 0.0),
}


def load_klines_5m(symbol):
    path = os.path.join(DATA, CSV_5M[symbol])
    bars = []
    with open(path, "r", encoding="utf-8", newline="") as f:
        for row in csv.DictReader(f):
            dt = datetime.strptime(row["open_time"], "%Y-%m-%d %H:%M:%S.%f %z")
            bars.append(KlineBar(open_time=int(dt.timestamp() * 1000), open=float(row["open"]),
                                 high=float(row["high"]), low=float(row["low"]),
                                 close=float(row["close"]), volume=float(row["volume"])))
    bars.sort(key=lambda b: b.open_time)
    return bars


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


def make_params(hs, act, cb):
    return Params(fast_ma_period=288, slow_ma_period=480, stop_mode="ma288", hard_stop_pct=hs,
                  take_profit_mode="trailing", trailing_activate_pct=act, trailing_callback_pct=cb,
                  slope_threshold=0.0, bbw_threshold=0.0, vol_threshold=0.0, realized_vol_threshold=0.0,
                  use_30m_expanding=False, use_5m_expanding=False, min_angle_5m=0.0, entry_timeframe="30m")


def backtest_with_path(symbol, params, bars, vol_threshold):
    fast = params.fast_ma_period
    slow = params.slow_ma_period
    n = len(bars)
    closes = [b.close for b in bars]
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
            pos["min_profit"] = min(pos["min_profit"], pnl)
            pos["path"].append(pnl)
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
                trades.append({"ret_pct": ret * 100.0, "reason": reason, "bars": i - pos["entry_idx"],
                               "mfe": pos["max_profit"], "mae": pos["min_profit"], "path": pos["path"]})
                pos = None
                continue

        if pos is None and fast_ma is not None and slow_ma is not None and prev_fast_ma is not None:
            if vol_threshold > 0.0 and vol48[i] is not None and vol48[i] >= vol_threshold:
                continue
            if fast_ma > slow_ma and prev_close < prev_fast_ma and close > fast_ma:
                hs = close * (1.0 - params.hard_stop_pct / 100.0)
                pos = {"side": "LONG", "entry_price": close, "entry_idx": i, "hard_stop_price": hs,
                       "max_profit": 0.0, "min_profit": 0.0, "path": []}
            elif fast_ma < slow_ma and prev_close > prev_fast_ma and close < fast_ma:
                hs = close * (1.0 + params.hard_stop_pct / 100.0)
                pos = {"side": "SHORT", "entry_price": close, "entry_idx": i, "hard_stop_price": hs,
                       "max_profit": 0.0, "min_profit": 0.0, "path": []}

    if pos is not None:
        ret = (closes[-1] - pos["entry_price"]) / pos["entry_price"] if pos["side"] == "LONG" else (pos["entry_price"] - closes[-1]) / pos["entry_price"]
        trades.append({"ret_pct": ret * 100.0, "reason": "持仓到结束", "bars": n - 1 - pos["entry_idx"],
                       "mfe": pos["max_profit"], "mae": pos["min_profit"], "path": pos["path"]})
    return trades


def pnl_at(path, k):
    if path is None or k > len(path):
        return None
    return path[k - 1]


def median(xs):
    if not xs:
        return float('nan')
    s = sorted(xs)
    m = len(s) // 2
    return s[m] if len(s) % 2 else (s[m - 1] + s[m]) / 2.0


def main() -> int:
    md: List[str] = []
    add = md.append
    add("# 5m 亏损研究：能否减少亏损、保住盈利单")
    add("")

    KS = [1, 2, 3, 5, 10, 20, 30]
    for coin in ["BTCUSDT", "ETHUSDT", "SOLUSDT", "BNBUSDT", "SUIUSDT", "HYPEUSDT"]:
        hs, act, cb, thr = BEST[coin]
        params = make_params(hs, act, cb)
        bars = load_klines_5m(coin)
        ts = backtest_with_path(coin, params, bars, thr)
        wins = [t for t in ts if t["reason"] == "移动止盈"]
        losses = [t for t in ts if t["reason"] == "MA288止损"]
        add(f"## {coin}  （盈利 {len(wins)} / 止损 {len(losses)}）")
        add("")

        # 1. 止损单 MFE 分桶
        add("### 1. 止损单 MFE 分桶")
        add("")
        add("| MFE | 笔数 | 占比 | 平均收益 | 平均bar |")
        add("|---|---|---|---|---|")
        for lo, hi, lab in [(-1e9, 0.3, "<0.3%"), (0.3, 0.5, "0.3~0.5%"), (0.5, 1.0, "0.5~1%"),
                            (1.0, 2.0, "1~2%"), (2.0, 1e9, "≥2%")]:
            b = [t for t in losses if lo <= t["mfe"] < hi]
            if not b:
                continue
            add(f"| {lab} | {len(b)} | {len(b)/len(losses)*100:.0f}% | {sum(t['ret_pct'] for t in b)/len(b):+.2f}% | {sum(t['bars'] for t in b)/len(b):.1f} |")
        add("")

        # 2. 路径中位数
        add("### 2. 第 k 根浮盈中位数（盈利 vs 止损）")
        add("")
        add("| k | 盈利单 | 止损单 |")
        add("|---|---|---|")
        for k in KS:
            wv = [v for v in (pnl_at(t["path"], k) for t in wins) if v is not None]
            lv = [v for v in (pnl_at(t["path"], k) for t in losses) if v is not None]
            add(f"| {k} | {median(wv):+.3f}% | {median(lv):+.3f}% |")
        add("")

        # 3. 条件概率
        add("### 3. 条件概率 P(盈利 | 第 k 根浮盈区间)")
        add("")
        add("| k | 区间 | 样本 | 盈利概率 |")
        add("|---|---|---|---|")
        all_ws = wins + losses
        for k in (2, 3, 5, 10):
            for lo, hi, lab in [(-1e9, 0.0, "<0%"), (0.0, 0.5, "0~0.5%"), (0.5, 1.0, "0.5~1%"), (1.0, 1e9, "≥1%")]:
                sub = [t for t in all_ws if (v := pnl_at(t["path"], k)) is not None and lo <= v < hi]
                if len(sub) < 5:
                    continue
                w = sum(1 for t in sub if t["reason"] == "移动止盈")
                add(f"| {k} | {lab} | {len(sub)} | {w/len(sub)*100:.0f}% |")
        add("")

        # 4. 时间止损净效果
        add("### 4. 时间止损净效果（第 k 根浮盈 < X 则离场）")
        add("")
        add("| 规则 | 排除止损 | 误伤盈利 | 排除止损收益 | 误伤盈利收益 |")
        add("|---|---|---|---|---|")
        for k, x in ((3, 0.0), (5, 0.0), (5, 0.3), (10, 0.3)):
            cut_loss = [t for t in losses if (v := pnl_at(t["path"], k)) is not None and v < x]
            cut_win = [t for t in wins if (v := pnl_at(t["path"], k)) is not None and v < x]
            if not cut_loss:
                continue
            add(f"| 第{k}根<{x}% | {len(cut_loss)} | {len(cut_win)} | {sum(t['ret_pct'] for t in cut_loss):+.1f}% | {sum(t['ret_pct'] for t in cut_win):+.1f}% |")
        add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "5m_loss_analysis.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
