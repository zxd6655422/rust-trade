"""盈利单 vs 亏损单的「后续走势」路径统计概率分析。

方法：
  1. 用对齐生产的回测（slow=480 + vol过滤 + 退出链全开）记录每笔交易的逐 bar 浮盈路径。
  2. 对比盈利单（移动止盈）与亏损单（MA288止损）在第 k 根 bar 的浮盈分布。
  3. 计算条件概率：P(最终盈利 | 入场后第 k 根 bar 浮盈 >= X%)，用于回答
     「早期浮盈能否预测最终盈亏」。

输出：feature_report/path_probability_report.md
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


def backtest_with_path(symbol, params, bars):
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
            pos["min_profit"] = min(pos["min_profit"], pnl)
            pos["path"].append(pnl)  # 逐 bar 浮盈
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
                trades.append({"symbol": symbol, "side": side, "ret_pct": ret * 100.0,
                               "reason": reason, "bars": i - pos["entry_idx"],
                               "mfe": pos["max_profit"], "mae": pos["min_profit"],
                               "path": pos["path"]})
                pos = None
                continue

        if pos is None and fast_ma is not None and slow_ma is not None and prev_fast_ma is not None:
            if vol48 is not None and vol48[i] is not None and vol48[i] >= params.realized_vol_threshold:
                continue
            if fast_ma > slow_ma:
                if prev_close < prev_fast_ma and close > fast_ma:
                    hard_stop = close * (1.0 - params.hard_stop_pct / 100.0)
                    pos = {"side": "LONG", "entry_price": close, "entry_idx": i,
                           "hard_stop_price": hard_stop, "max_profit": 0.0, "min_profit": 0.0, "path": []}
            elif fast_ma < slow_ma:
                if prev_close > prev_fast_ma and close < fast_ma:
                    hard_stop = close * (1.0 + params.hard_stop_pct / 100.0)
                    pos = {"side": "SHORT", "entry_price": close, "entry_idx": i,
                           "hard_stop_price": hard_stop, "max_profit": 0.0, "min_profit": 0.0, "path": []}

    if pos is not None:
        ret = (closes[-1] - pos["entry_price"]) / pos["entry_price"] if pos["side"] == "LONG" else (pos["entry_price"] - closes[-1]) / pos["entry_price"]
        trades.append({"symbol": symbol, "side": pos["side"], "ret_pct": ret * 100.0,
                       "reason": "持仓到结束", "bars": n - 1 - pos["entry_idx"],
                       "mfe": pos["max_profit"], "mae": pos["min_profit"], "path": pos["path"]})
    return trades


def pnl_at(path, k):
    """入场后第 k 根 bar 的浮盈（k 从 1 开始）；路径不够长则 None。"""
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
    add("# 盈利单 vs 亏损单 后续走势路径 · 统计概率")
    add("")
    add("- 口径：对齐生产（slow=480 + vol过滤 + 退出链全开）。")
    add("- path[k] = 入场后第 k 根 bar 收盘价相对入场价的浮盈%（k 从 1 起）。")
    add("- 盈利单 = 移动止盈平仓；亏损单 = MA288 止损平仓。")
    add("")

    KS = [1, 2, 3, 5, 8, 10, 15, 20, 30]
    for coin in ["BTCUSDT", "ETHUSDT"]:
        params = dc.SYMBOL_PARAMS[coin]
        bars = load_klines_30m(coin)
        trades = backtest_with_path(coin, params, bars)
        wins = [t for t in trades if t["reason"] == "移动止盈"]
        losses = [t for t in trades if t["reason"] == "MA288止损"]

        add(f"## {coin}  （盈利 {len(wins)} 笔 / 止损 {len(losses)} 笔）")
        add("")

        # 1. 路径轮廓：第 k 根 bar 浮盈中位数
        add("### 1. 后续走势轮廓（第 k 根 bar 浮盈中位数）")
        add("")
        add("| 第 k 根 bar | 盈利单中位浮盈 | 止损单中位浮盈 | 盈利单占比达+0.5% | 止损单占比达+0.5% |")
        add("|---|---|---|---|---|")
        for k in KS:
            wv = [pnl_at(t["path"], k) for t in wins]
            lv = [pnl_at(t["path"], k) for t in losses]
            wv = [v for v in wv if v is not None]
            lv = [v for v in lv if v is not None]
            w_half = sum(1 for v in wv if v >= 0.5) / len(wv) * 100 if wv else 0
            l_half = sum(1 for v in lv if v >= 0.5) / len(lv) * 100 if lv else 0
            add(f"| {k} | {median(wv):+.2f}% | {median(lv):+.2f}% | {w_half:.0f}% | {l_half:.0f}% |")
        add("")

        # 2. 条件概率：P(最终盈利 | 第 k 根 bar 浮盈区间)
        add("### 2. 条件概率 P(最终盈利 | 第 k 根 bar 浮盈区间)")
        add("")
        add("| 第 k 根 bar | 浮盈区间 | 样本数 | 最终盈利概率 | 最终亏损概率 |")
        add("|---|---|---|---|---|")
        all_ws = wins + losses
        for k in (2, 3, 5, 10):
            buckets = [(-1e9, 0.0, "<0%"), (0.0, 0.5, "0~0.5%"), (0.5, 1.0, "0.5~1%"),
                       (1.0, 2.0, "1~2%"), (2.0, 1e9, "≥2%")]
            for lo, hi, lab in buckets:
                sub = [t for t in all_ws if (v := pnl_at(t["path"], k)) is not None and lo <= v < hi]
                if len(sub) < 5:
                    continue
                w = sum(1 for t in sub if t["reason"] == "移动止盈")
                add(f"| {k} | {lab} | {len(sub)} | {w/len(sub)*100:.0f}% | {(len(sub)-w)/len(sub)*100:.0f}% |")
        add("")

        # 3. 时间止损规则量化
        add("### 3. 「入场后第 k 根 bar 浮盈仍 < X% 则离场」净效果")
        add("")
        add("| 规则 | 排除止损单 | 误伤盈利单 | 止损单被排掉收益 | 盈利单被误伤收益 |")
        add("|---|---|---|---|---|")
        for k, x in ((3, 0.5), (5, 0.5), (5, 1.0), (10, 1.0)):
            cut_loss = [t for t in losses if (v := pnl_at(t["path"], k)) is not None and v < x]
            cut_win = [t for t in wins if (v := pnl_at(t["path"], k)) is not None and v < x]
            if not cut_loss:
                continue
            add(f"| 第{k}根<{x}% | {len(cut_loss)} | {len(cut_win)} | "
                f"{sum(t['ret_pct'] for t in cut_loss):+.1f}% | {sum(t['ret_pct'] for t in cut_win):+.1f}% |")
        add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "path_probability_report.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
