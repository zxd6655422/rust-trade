"""方向2 深入诊断：延迟入场为什么没利用好「盈利 vs 止损」的走势差异。

拆解三个问题：
  Q1 追高代价：延迟确认时以「确认价」入场，比「信号触发价」高多少？吃掉盈利单多少利润？
  Q2 放弃信号质量：被放弃的信号里，多少本会止损（正确过滤）、多少本会盈利（误伤）？
  Q3 理想对照：若入场价仍用「信号价」（隔离追高），延迟确认的净效果还剩多少？

输出：feature_report/delayed_entry_deep_report.md
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


def _exit_ret(side, entry, exit_price):
    return (exit_price - entry) / entry if side == "LONG" else (entry - exit_price) / entry


def _run(symbol, params, bars, delay_n: Optional[int], min_gain: float, use_signal_price: bool):
    """返回 (trades, signals)。signals 记录每个触发信号的立即入场结果 + 延迟处置。"""
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
    sigs = []  # 每个信号：{idx, side, ref_price, immediate_ret, delayed}
    pos = None
    pending = None
    signal_counter = 0

    def open_pos(side, entry, idx, ref_side):
        hs = entry * (1.0 - params.hard_stop_pct / 100.0) if side == "LONG" else entry * (1.0 + params.hard_stop_pct / 100.0)
        return {"side": side, "entry_price": entry, "entry_idx": idx, "hard_stop_price": hs, "max_profit": 0.0}

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
                ret = _exit_ret(side, entry, exit_price)
                trades.append({"ret": ret, "ret_pct": ret * 100.0, "reason": reason, "sig": pos.get("sig"),
                               "entry": entry, "mfe": pos["max_profit"]})
                pos = None
                continue

        if pending is not None:
            p = pending
            trend_broke = (p["side"] == "LONG" and fast_ma < slow_ma) or (p["side"] == "SHORT" and fast_ma > slow_ma)
            # 价格反向穿越 MA288（与 MA288 止损同口径）
            price_broke = (p["side"] == "LONG" and prev_close > prev_fast_ma and close < fast_ma) or \
                          (p["side"] == "SHORT" and prev_close < prev_fast_ma and close > fast_ma)
            if trend_broke or price_broke:
                p["abandon"] = "反转"
                pending = None
            elif i - p["trigger_idx"] >= delay_n:
                gain = (close - p["ref_price"]) / p["ref_price"] * 100.0 if p["side"] == "LONG" else (p["ref_price"] - close) / p["ref_price"] * 100.0
                if gain >= min_gain:
                    entry = p["ref_price"] if use_signal_price else close
                    pos = open_pos(p["side"], entry, i, p["side"])
                    pos["sig"] = p["id"]
                    p["confirmed"] = True
                    p["confirm_gain"] = gain
                else:
                    p["abandon"] = "浮盈不足"
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
                sid = signal_counter
                signal_counter += 1
                if delay_n is None:
                    pos = open_pos(sig, close, i, sig)
                    pos["sig"] = sid
                else:
                    pending = {"id": sid, "side": sig, "ref_price": close, "trigger_idx": i,
                               "confirmed": False, "abandon": None, "confirm_gain": None}

    if pos is not None:
        ret = (closes[-1] - pos["entry_price"]) / pos["entry_price"] if pos["side"] == "LONG" else (pos["entry_price"] - closes[-1]) / pos["entry_price"]
        trades.append({"ret": ret, "ret_pct": ret * 100.0, "reason": "持仓到结束", "sig": pos.get("sig"),
                       "entry": pos["entry_price"], "mfe": pos["max_profit"]})
    return trades


def compound(ts):
    eq = 1.0
    for t in ts:
        eq *= (1.0 + t["ret"])
    return (eq - 1.0) * 100.0


def main() -> int:
    md: List[str] = []
    add = md.append
    add("# 方向2 深入诊断：延迟入场为何没用好走势差异")
    add("")
    add("- 基线 = 立即入场（含 realized_vol_threshold 过滤，slow=480，退出链全开）。")
    add("- 追高 = 确认时以「确认价」入场；理想对照 = 确认时仍以「信号价」入场（隔离追高）。")
    add("")

    for coin in ["BTCUSDT", "ETHUSDT"]:
        params = dc.SYMBOL_PARAMS[coin]
        bars = load_klines_30m(coin)
        add(f"## {coin}")
        add("")

        base = _run(coin, params, bars, None, 0.0, False)
        base_by_sig = {t["sig"]: t for t in base if t["sig"] is not None}
        add(f"- 基线：{len(base)} 笔，简单 {sum(t['ret_pct'] for t in base):+.2f}%，复利 {compound(base):+.2f}%。")
        add("")

        add("| 延迟N | 浮盈≥X | 入场价 | 交易数 | 复利 | 放弃中本会盈利(误伤) | 放弃中本会止损(正确) | 误伤总收益 |")
        add("|---|---|---|---|---|---|---|---|")
        for dn, mg in [(1, 0.0), (2, 0.0), (3, 0.0), (2, -0.3), (3, -0.5), (2, -1e9)]:
            for use_sp, label in [(False, "确认价"), (True, "信号价(理想)")]:
                ts = _run(coin, params, bars, dn, mg, use_sp)
                confirmed = {t["sig"]: t for t in ts if t["sig"] is not None}
                # 放弃的信号 = 基线里有但延迟没有
                abandoned = [base_by_sig[s] for s in base_by_sig if s not in confirmed]
                ab_wins = [t for t in abandoned if t["ret_pct"] > 0]
                ab_loss = [t for t in abandoned if t["ret_pct"] <= 0]
                add(f"| {dn} | ≥{mg}% | {label} | {len(ts)} | {compound(ts):+.2f}% | "
                    f"{len(ab_wins)} | {len(ab_loss)} | {sum(t['ret_pct'] for t in ab_wins):+.2f}% |")
        add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "delayed_entry_deep_report.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
