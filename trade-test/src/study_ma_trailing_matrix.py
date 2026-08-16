"""MA 动态止盈线 · 多参数矩阵测试。

三维度：
  - ma_period：止盈均线周期 {48, 96, 192}
  - activate_pct：盈利达到多少才启用动态止盈 {2, 4, 6, 8, 10, 15}
  - confirm_bars：跌破止盈线后「连续 N 根收盘仍在线下」才离场 {1, 3, 5, 10}
    （=1 立即离场；=N 先观察，若重新站回线上则继续持有）

退出链：硬止损 → MA288止损 → [盈利≥activate] MA动态止盈(带确认) → 趋势反转。
口径：slow=480 + vol过滤 + 反转ON。

输出：feature_report/ma_trailing_matrix_report.md
"""
from __future__ import annotations

import math
import os
from itertools import product
from typing import List, Dict, Any, Optional

import data_config as dc
from loader import load_klines_30m


def sma_series(closes, period):
    n = len(closes)
    p = [0.0] * (n + 1)
    for i in range(n):
        p[i + 1] = p[i] + closes[i]
    out = [None] * n
    for i in range(period - 1, n):
        out[i] = (p[i + 1] - p[i + 1 - period]) / period
    return out


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


def backtest(symbol, params, bars, ma_series, activate_pct, confirm_bars):
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
            # MA 动态止盈（盈利 >= activate 后启用，带确认观察）
            if exit_price is None and pos["max_profit"] >= activate_pct and ma_series[i] is not None:
                ma_v = ma_series[i]
                if side == "LONG":
                    pos["below_count"] = pos["below_count"] + 1 if close < ma_v else 0
                else:
                    pos["below_count"] = pos["below_count"] + 1 if close > ma_v else 0
                if pos["below_count"] >= confirm_bars:
                    exit_price, reason = close, "MA止盈"
            if exit_price is None:
                if side == "LONG" and fast_ma < slow_ma:
                    exit_price, reason = close, "趋势反转"
                elif side == "SHORT" and fast_ma > slow_ma:
                    exit_price, reason = close, "趋势反转"
            if exit_price is not None:
                ret = (exit_price - entry) / entry if side == "LONG" else (entry - exit_price) / entry
                trades.append(ret)
                pos = None
                continue

        if pos is None and fast_ma is not None and slow_ma is not None and prev_fast_ma is not None:
            if vol48[i] is not None and vol48[i] >= params.realized_vol_threshold:
                continue
            if fast_ma > slow_ma and prev_close < prev_fast_ma and close > fast_ma:
                hs = close * (1.0 - params.hard_stop_pct / 100.0)
                pos = {"side": "LONG", "entry_price": close, "entry_idx": i, "hard_stop_price": hs, "max_profit": 0.0, "below_count": 0}
            elif fast_ma < slow_ma and prev_close > prev_fast_ma and close < fast_ma:
                hs = close * (1.0 + params.hard_stop_pct / 100.0)
                pos = {"side": "SHORT", "entry_price": close, "entry_idx": i, "hard_stop_price": hs, "max_profit": 0.0, "below_count": 0}

    if pos is not None:
        ret = (closes[-1] - pos["entry_price"]) / pos["entry_price"] if pos["side"] == "LONG" else (pos["entry_price"] - closes[-1]) / pos["entry_price"]
        trades.append(ret)
    return trades


def metrics(rets):
    n = len(rets)
    if n == 0:
        return dict(n=0, wr=0, simple=0, comp=0)
    wins = sum(1 for r in rets if r > 0)
    eq = 1.0
    for r in rets:
        eq *= (1.0 + r)
    return dict(n=n, wr=wins / n * 100, simple=sum(r for r in rets) * 100, comp=(eq - 1.0) * 100)


def main() -> int:
    md: List[str] = []
    add = md.append
    add("# MA 动态止盈线 多参数矩阵")
    add("")
    add("- 维度：止盈均线 {48,96,192} × activate {2,4,6,8,10,15}% × 确认 {1,3,5,10} 根。")
    add("")

    MA_PERIODS = [48, 96, 192]
    ACTS = [2.0, 4.0, 6.0, 8.0, 10.0, 15.0]
    CONFIRMS = [1, 3, 5, 10]

    for coin in ["BTCUSDT", "ETHUSDT", "SOLUSDT"]:
        params = dc.SYMBOL_PARAMS[coin]
        bars = load_klines_30m(coin)
        closes = [b.close for b in bars]
        ma_series_map = {p: sma_series(closes, p) for p in MA_PERIODS}

        add(f"## {coin}  （基线复利：BTC +229% / ETH +209% / SOL +229%）")
        add("")
        grid = []
        for mp, act, cb in product(MA_PERIODS, ACTS, CONFIRMS):
            rets = backtest(coin, params, bars, ma_series_map[mp], act, cb)
            m = metrics(rets)
            grid.append((mp, act, cb, m))
        grid.sort(key=lambda x: -x[3]["comp"])
        add("### Top 20（按复利）")
        add("")
        add("| 止盈均线 | activate | 确认根数 | 笔数 | 胜率 | 简单 | 复利 |")
        add("|---|---|---|---|---|---|---|")
        for mp, act, cb, m in grid[:20]:
            add(f"| MA{mp} | {act}% | {cb}根 | {m['n']} | {m['wr']:.1f}% | {m['simple']:+.2f}% | {m['comp']:+.2f}% |")
        add("")

        # 维度敏感性：固定 activate=最优，看 ma_period × confirm 的影响
        add("### 全组合复利热力（每格=该 ma_period × confirm 在所有 activate 下的最优复利）")
        add("")
        add("| 止盈均线 | confirm=1 | confirm=3 | confirm=5 | confirm=10 |")
        add("|---|---|---|---|---|")
        for mp in MA_PERIODS:
            cells = []
            for cb in CONFIRMS:
                best = max((g[3]["comp"] for g in grid if g[0] == mp and g[2] == cb), default=float('nan'))
                cells.append(f"{best:+.0f}%")
            add(f"| MA{mp} | " + " | ".join(cells) + " |")
        add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "ma_trailing_matrix_report.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
