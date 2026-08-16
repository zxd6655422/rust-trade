"""优化后 30m 策略：①亏损单位置 ②盈利单是否拿满趋势幅度。

Part1（读 trade_features.json）：亏损单(MA288止损/硬止损) 的入场位置分布。
Part2（重新回测）：盈利单(移动止盈) 离场后，趋势又走了多少（漏掉的幅度）。

口径：对齐生产（slow=480 + vol过滤 + 退出链全开）。

输出：feature_report/profit_fullness_report.md
"""
from __future__ import annotations

import json
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
    """对齐生产回测，记录盈利单的入场/离场 idx、价、收益。"""
    import math
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
                               "entry_idx": pos["entry_idx"], "mfe": pos["max_profit"]})
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
                       "entry": pos["entry_price"], "exit": closes[-1], "exit_idx": n - 1,
                       "entry_idx": pos["entry_idx"], "mfe": pos["max_profit"]})
    return trades, highs, lows


def mean(xs):
    return sum(xs) / len(xs) if xs else 0.0


def main() -> int:
    md: List[str] = []
    add = md.append
    add("# 优化后 30m 策略：亏损单位置 + 盈利单拿满程度")
    add("")

    # Part 1: 亏损单位置（读 trade_features.json）
    SRC = os.path.dirname(os.path.abspath(__file__))
    trades_json = json.load(open(os.path.join(SRC, "feature_report", "trade_features.json"), encoding="utf-8"))
    add("## Part 1. 亏损单位置分布（入场时区间位置）")
    add("")
    add("| 币种 | 亏损单数 | 底部(<40%) | 中部(40~60%) | 顶部(>60%) | 入场离MA288均值 |")
    add("|---|---|---|---|---|---|")
    for coin in ["BTCUSDT", "ETHUSDT", "SOLUSDT", "BNBUSDT", "SUIUSDT", "HYPEUSDT"]:
        losses = [t for t in trades_json if t["symbol"] == coin and t["reason"] in ("MA288止损", "硬止损")]
        if not losses:
            continue
        pos = [t["entry"].get("position_in_range_96") for t in losses if t["entry"].get("position_in_range_96") is not None]
        c2m = [t["entry"].get("close_to_ma288_pct") for t in losses if t["entry"].get("close_to_ma288_pct") is not None]
        bot = sum(1 for p in pos if p < 40)
        mid = sum(1 for p in pos if 40 <= p < 60)
        top = sum(1 for p in pos if p >= 60)
        n = len(pos)
        add(f"| {coin} | {len(losses)} | {bot}({bot/n*100:.0f}%) | {mid}({mid/n*100:.0f}%) | {top}({top/n*100:.0f}%) | {mean(c2m):+.3f}% |")
    add("")

    # Part 2: 盈利单拿满程度
    add("## Part 2. 盈利单是否拿满趋势幅度（离场后趋势又走了多少）")
    add("")
    add("| 币种 | 盈利单数 | 平均已拿收益 | 平均MFE | 止盈回撤(MFE-收益) | 离场后20根继续 | 离场后50根继续 | 离场后100根继续 |")
    add("|---|---|---|---|---|---|---|---|")
    for coin in ["BTCUSDT", "ETHUSDT", "SOLUSDT", "BNBUSDT", "SUIUSDT", "HYPEUSDT"]:
        params = dc.SYMBOL_PARAMS[coin]
        bars = load_klines_30m(coin)
        ts, highs, lows = backtest_full(coin, params, bars)
        n = len(bars)
        wins = [t for t in ts if t["reason"] == "移动止盈"]
        if not wins:
            continue
        avg_ret = mean([t["ret_pct"] for t in wins])
        avg_mfe = mean([t["mfe"] for t in wins])
        # 离场后继续走的幅度
        for t in wins:
            for horizon in (20, 50, 100):
                ei = t["exit_idx"]
                if ei + 1 >= n:
                    t[f"miss_{horizon}"] = 0.0
                    continue
                if t["side"] == "LONG":
                    fh = max(highs[ei + 1: min(n, ei + 1 + horizon)])
                    t[f"miss_{horizon}"] = (fh - t["exit"]) / t["exit"] * 100.0
                else:
                    fl = min(lows[ei + 1: min(n, ei + 1 + horizon)])
                    t[f"miss_{horizon}"] = (t["exit"] - fl) / t["exit"] * 100.0
        add(f"| {coin} | {len(wins)} | {avg_ret:+.2f}% | {avg_mfe:+.2f}% | {avg_mfe-avg_ret:+.2f}% | "
            f"{mean([t['miss_20'] for t in wins]):+.2f}% | {mean([t['miss_50'] for t in wins]):+.2f}% | {mean([t['miss_100'] for t in wins]):+.2f}% |")
    add("")
    add("> 解读：「止盈回撤」= 移动止盈在 peak 后回撤 callback 才离场，损失的利润；「离场后继续」= 离场后趋势又走的幅度（漏掉的）。")
    add("> 若「离场后继续」很大，说明移动止盈过早离场，没拿满趋势。")
    add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "profit_fullness_report.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
