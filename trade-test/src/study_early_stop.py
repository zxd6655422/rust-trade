"""方向1：更早止损 —— 盘中浮动止损扫描（BTC/ETH）。

在当前退出链（硬止损→MA288止损→移动止盈→趋势反转）基础上，新增/替换一层
「盘中浮动止损」：持仓期间，浮盈跌破 -X% 即用盘中价离场，而非等收盘穿越 MA288。
扫描 X 找出是否能用更小亏损提前离场、提升复利。

输出：feature_report/early_stop_report.md
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


def backtest(symbol, params, bars, early_stop_pct: Optional[float]):
    fast = params.fast_ma_period
    slow = params.slow_ma_period
    n = len(bars)
    closes = [b.close for b in bars]
    vol48 = realized_vol_48_series(closes) if params.realized_vol_threshold > 0.0 else None

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

            # 0. 盘中浮动止损（新增，最早触发）
            if early_stop_pct is not None:
                stop_price = entry * (1.0 - early_stop_pct / 100.0) if side == "LONG" else entry * (1.0 + early_stop_pct / 100.0)
                if side == "LONG" and bar.low <= stop_price:
                    exit_price, reason = stop_price, "浮动止损"
                elif side == "SHORT" and bar.high >= stop_price:
                    exit_price, reason = stop_price, "浮动止损"

            # 1. 硬止损
            if exit_price is None and params.hard_stop_pct > 0.0:
                if side == "LONG" and bar.low <= pos["hard_stop_price"]:
                    exit_price, reason = pos["hard_stop_price"], "硬止损"
                elif side == "SHORT" and bar.high >= pos["hard_stop_price"]:
                    exit_price, reason = pos["hard_stop_price"], "硬止损"

            # 2. MA288 止损
            if exit_price is None and params.stop_mode == "ma288" and prev_fast_ma is not None:
                if side == "LONG" and prev_close > prev_fast_ma and close < fast_ma:
                    exit_price, reason = close, "MA288止损"
                elif side == "SHORT" and prev_close < prev_fast_ma and close > fast_ma:
                    exit_price, reason = close, "MA288止损"

            # 3. 移动止盈
            if exit_price is None and params.take_profit_mode == "trailing":
                if pos["max_profit"] >= params.trailing_activate_pct:
                    if pos["max_profit"] - pnl >= params.trailing_callback_pct:
                        exit_price, reason = close, "移动止盈"

            # 4. 趋势反转
            if exit_price is None:
                if side == "LONG" and fast_ma < slow_ma:
                    exit_price, reason = close, "趋势反转"
                elif side == "SHORT" and fast_ma > slow_ma:
                    exit_price, reason = close, "趋势反转"

            if exit_price is not None:
                ret = (exit_price - entry) / entry if side == "LONG" else (entry - exit_price) / entry
                trades.append({"ret": ret, "ret_pct": ret * 100.0, "reason": reason,
                               "mfe": pos["max_profit"], "bars": i - pos["entry_idx"]})
                pos = None
                continue

        if pos is None and fast_ma is not None and slow_ma is not None and prev_fast_ma is not None:
            if vol48 is not None and vol48[i] is not None and vol48[i] >= params.realized_vol_threshold:
                continue
            if fast_ma > slow_ma:
                if prev_close < prev_fast_ma and close > fast_ma:
                    hard_stop = close * (1.0 - params.hard_stop_pct / 100.0)
                    pos = {"side": "LONG", "entry_price": close, "entry_idx": i,
                           "hard_stop_price": hard_stop, "max_profit": 0.0}
            elif fast_ma < slow_ma:
                if prev_close > prev_fast_ma and close < fast_ma:
                    hard_stop = close * (1.0 + params.hard_stop_pct / 100.0)
                    pos = {"side": "SHORT", "entry_price": close, "entry_idx": i,
                           "hard_stop_price": hard_stop, "max_profit": 0.0}

    if pos is not None:
        ret = (closes[-1] - pos["entry_price"]) / pos["entry_price"] if pos["side"] == "LONG" else (pos["entry_price"] - closes[-1]) / pos["entry_price"]
        trades.append({"ret": ret, "ret_pct": ret * 100.0, "reason": "持仓到结束",
                       "mfe": pos["max_profit"], "bars": n - 1 - pos["entry_idx"]})
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
        n=n,
        wr=len(wins) / n * 100,
        simple=sum(t["ret_pct"] for t in ts),
        comp=(eq - 1.0) * 100,
        avg_loss=(sum(t["ret_pct"] for t in losses) / len(losses)) if losses else 0.0,
        avg_win=(sum(t["ret_pct"] for t in wins) / len(wins)) if wins else 0.0,
    )


def main() -> int:
    md: List[str] = []
    add = md.append
    add("# 方向1：更早止损（盘中浮动止损）扫描")
    add("")
    add("- 口径：对齐生产（slow=480 + vol过滤 + 退出链全开），额外加「盘中浮动止损：浮盈跌破 -X% 即离场」。")
    add("")

    XS = [None, 0.15, 0.2, 0.3, 0.5, 0.8, 1.0, 1.2]
    for coin in ["BTCUSDT", "ETHUSDT"]:
        params = dc.SYMBOL_PARAMS[coin]
        bars = load_klines_30m(coin)
        add(f"## {coin}")
        add("")
        add("| 浮动止损 | 交易数 | 胜率 | 简单收益 | 复利收益 | 平均亏损 | 平均盈利 |")
        add("|---|---|---|---|---|---|---|")
        for x in XS:
            ts = backtest(coin, params, bars, x)
            m = metrics(ts)
            lab = "基线(无)" if x is None else f"-{x}%"
            add(f"| {lab} | {m['n']} | {m['wr']:.1f}% | {m['simple']:+.2f}% | {m['comp']:+.2f}% | "
                f"{m['avg_loss']:+.2f}% | {m['avg_win']:+.2f}% |")
        add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "early_stop_report.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
