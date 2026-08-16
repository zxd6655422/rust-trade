"""方向2验证：盈利达到 X% 后，改用 MA48 动态止盈线（替代回撤 callback）。

退出链（替代原移动止盈）：
  硬止损 → MA288止损 → [盈利≥X% 时] MA48 动态止盈（收盘价跌破/突破 MA48 离场） → 趋势反转

扫描 activate X，对比基线（原移动止盈 activate+callback）。

口径：slow=480 + vol过滤 + 反转ON。
输出：feature_report/ma48_trailing_report.md
"""
from __future__ import annotations

import math
import os
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


def backtest_ma48(symbol, params, bars, activate_pct):
    """盈利 >= activate 后用 MA48 动态止盈。"""
    fast = params.fast_ma_period
    slow = params.slow_ma_period
    n = len(bars)
    closes = [b.close for b in bars]
    highs = [b.high for b in bars]
    lows = [b.low for b in bars]
    ma48 = sma_series(closes, 48)
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
            # 1. 硬止损
            if params.hard_stop_pct > 0.0:
                if side == "LONG" and bar.low <= pos["hard_stop_price"]:
                    exit_price, reason = pos["hard_stop_price"], "硬止损"
                elif side == "SHORT" and bar.high >= pos["hard_stop_price"]:
                    exit_price, reason = pos["hard_stop_price"], "硬止损"
            # 2. MA288 止损
            if exit_price is None and prev_fast_ma is not None:
                if side == "LONG" and prev_close > prev_fast_ma and close < fast_ma:
                    exit_price, reason = close, "MA288止损"
                elif side == "SHORT" and prev_close < prev_fast_ma and close > fast_ma:
                    exit_price, reason = close, "MA288止损"
            # 3. MA48 动态止盈（盈利 >= activate 后启用）
            if exit_price is None and pos["max_profit"] >= activate_pct and ma48[i] is not None:
                if side == "LONG" and close < ma48[i]:
                    exit_price, reason = close, "MA48止盈"
                elif side == "SHORT" and close > ma48[i]:
                    exit_price, reason = close, "MA48止盈"
            # 4. 趋势反转
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
                pos = {"side": "LONG", "entry_price": close, "entry_idx": i, "hard_stop_price": hs, "max_profit": 0.0}
            elif fast_ma < slow_ma and prev_close > prev_fast_ma and close < fast_ma:
                hs = close * (1.0 + params.hard_stop_pct / 100.0)
                pos = {"side": "SHORT", "entry_price": close, "entry_idx": i, "hard_stop_price": hs, "max_profit": 0.0}

    if pos is not None:
        ret = (closes[-1] - pos["entry_price"]) / pos["entry_price"] if pos["side"] == "LONG" else (pos["entry_price"] - closes[-1]) / pos["entry_price"]
        trades.append(ret)
    return trades


def backtest_baseline(symbol, params, bars):
    """原移动止盈（activate + callback）。"""
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
            if params.hard_stop_pct > 0.0:
                if side == "LONG" and bar.low <= pos["hard_stop_price"]:
                    exit_price = pos["hard_stop_price"]
                elif side == "SHORT" and bar.high >= pos["hard_stop_price"]:
                    exit_price = pos["hard_stop_price"]
            if exit_price is None and prev_fast_ma is not None:
                if side == "LONG" and prev_close > prev_fast_ma and close < fast_ma:
                    exit_price = close
                elif side == "SHORT" and prev_close < prev_fast_ma and close > fast_ma:
                    exit_price = close
            if exit_price is None and params.take_profit_mode == "trailing":
                if pos["max_profit"] >= params.trailing_activate_pct:
                    if pos["max_profit"] - pnl >= params.trailing_callback_pct:
                        exit_price = close
            if exit_price is None:
                if side == "LONG" and fast_ma < slow_ma:
                    exit_price = close
                elif side == "SHORT" and fast_ma > slow_ma:
                    exit_price = close
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
                pos = {"side": "LONG", "entry_price": close, "entry_idx": i, "hard_stop_price": hs, "max_profit": 0.0}
            elif fast_ma < slow_ma and prev_close > prev_fast_ma and close < fast_ma:
                hs = close * (1.0 + params.hard_stop_pct / 100.0)
                pos = {"side": "SHORT", "entry_price": close, "entry_idx": i, "hard_stop_price": hs, "max_profit": 0.0}
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
    add("# MA48 动态止盈线验证（盈利≥X% 后用 MA48 止盈）")
    add("")
    add("- 退出链：硬止损 → MA288止损 → [盈利≥X%] MA48止盈 → 趋势反转。")
    add("")

    ACTS = [1.0, 2.0, 3.0, 4.0, 5.0, 8.0, 10.0, 15.0]
    for coin in ["BTCUSDT", "ETHUSDT", "SOLUSDT", "BNBUSDT", "SUIUSDT", "HYPEUSDT"]:
        params = dc.SYMBOL_PARAMS[coin]
        bars = load_klines_30m(coin)
        base = metrics(backtest_baseline(coin, params, bars))
        add(f"## {coin}")
        add("")
        add("| 方案 | 交易数 | 胜率 | 简单收益 | 复利收益 |")
        add("|---|---|---|---|---|")
        add(f"| 基线(移动止盈{params.trailing_activate_pct}+{params.trailing_callback_pct}) | {base['n']} | {base['wr']:.1f}% | {base['simple']:+.2f}% | {base['comp']:+.2f}% |")
        for act in ACTS:
            m = metrics(backtest_ma48(coin, params, bars, act))
            add(f"| MA48止盈(盈利≥{act}%) | {m['n']} | {m['wr']:.1f}% | {m['simple']:+.2f}% | {m['comp']:+.2f}% |")
        add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "ma48_trailing_report.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
