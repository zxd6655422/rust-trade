"""方向3：涨幅确认入场（信号价→当前价涨幅达到 X% 才入场）—— 2×2 矩阵。

两个维度：
  入场价模式：
    - limit：价格触及 ref_price*(1+X%) 时以限价成交（入场价 = 信号价*(1+X%)）
    - close：某根 bar 收盘涨幅 >= X% 时以当根收盘价成交（可能 > X%）
  放弃条件（涨不到 X% 时）：
    - hold：只在趋势反转(MA288/MA480 交叉)时放弃，否则一直等
    - reject：趋势反转 或 价格反向跌破 MA288 时放弃

输出：feature_report/gain_entry_report.md
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


def backtest_gain(symbol, params, bars, X: float, entry_mode: str, reject_mode: str):
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

        # 待入场信号处理
        if pending is not None:
            p = pending
            bar = bars[i]
            trend_broke = (p["side"] == "LONG" and fast_ma < slow_ma) or (p["side"] == "SHORT" and fast_ma > slow_ma)
            price_broke = (p["side"] == "LONG" and close < fast_ma) or (p["side"] == "SHORT" and close > fast_ma)
            reject = trend_broke or (reject_mode == "reject" and price_broke)
            if reject:
                pending = None
            else:
                if entry_mode == "limit":
                    target = p["ref_price"] * (1.0 + X / 100.0) if p["side"] == "LONG" else p["ref_price"] * (1.0 - X / 100.0)
                    if (p["side"] == "LONG" and bar.high >= target) or (p["side"] == "SHORT" and bar.low <= target):
                        pos = open_pos(p["side"], target, i)
                        pending = None
                else:  # close
                    gain = (close - p["ref_price"]) / p["ref_price"] * 100.0 if p["side"] == "LONG" else (p["ref_price"] - close) / p["ref_price"] * 100.0
                    if gain >= X:
                        pos = open_pos(p["side"], close, i)
                        pending = None

        # 新信号
        if pos is None and pending is None and fast_ma is not None and slow_ma is not None and prev_fast_ma is not None:
            if vol48 is not None and vol48[i] is not None and vol48[i] >= params.realized_vol_threshold:
                continue
            sig = None
            if fast_ma > slow_ma and prev_close < prev_fast_ma and close > fast_ma:
                sig = "LONG"
            elif fast_ma < slow_ma and prev_close > prev_fast_ma and close < fast_ma:
                sig = "SHORT"
            if sig:
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
    add("# 方向3：涨幅确认入场 2×2 矩阵")
    add("")
    add("- 信号触发后，等价格相对信号价涨幅达 X% 才入场。")
    add("- 入场价：limit=限价(信号价*(1+X%))；close=当根收盘价。")
    add("- 放弃：hold=仅趋势反转放弃；reject=趋势反转或跌破MA288放弃。")
    add("")

    XS = [0.3, 0.5, 0.8, 1.0, 1.5, 2.0]
    for coin in ["BTCUSDT", "ETHUSDT"]:
        params = dc.SYMBOL_PARAMS[coin]
        bars = load_klines_30m(coin)
        add(f"## {coin}")
        add("")
        # 基线
        base = backtest_gain(coin, params, bars, 0.0, "close", "reject")
        # 基线应该用 X=0 立即入场，但这里 X=0 时 close 模式 gain>=0 立即入场，等价
        # 直接用 X=0 的 close 模式作为基线近似（gain>=0 当根 close>=ref 即入场=ref=close）
        add("")
        add("| X | 入场价模式 | 放弃条件 | 交易数 | 胜率 | 简单收益 | 复利收益 | 平均亏损 | 平均盈利 |")
        add("|---|---|---|---|---|---|---|---|---|")
        for em in ["limit", "close"]:
            for rm in ["hold", "reject"]:
                for X in XS:
                    ts = backtest_gain(coin, params, bars, X, em, rm)
                    m = metrics(ts)
                    add(f"| {X}% | {em} | {rm} | {m['n']} | {m['wr']:.1f}% | {m['simple']:+.2f}% | {m['comp']:+.2f}% | {m['avg_loss']:+.2f}% | {m['avg_win']:+.2f}% |")
        add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "gain_entry_report.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
