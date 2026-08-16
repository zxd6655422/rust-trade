"""MA48 指标与盈利/亏损的关系分析（找灵感）。

MA48 = SMA(close,48)，30m 上 = 24 小时均线（日线级短周期均线）。
派生维度（入场时）：
  - close_to_ma48 = (close−MA48)/MA48*100     —— 价格相对 MA48 的位置
  - ma48_slope_5/20 = MA48 近 5/20 根斜率    —— 短期动能（正=向上）
  - ma48_ma288_spread = (MA48−MA288)/MA288*100 —— 短期均线相对中期均线（正=短期在中期上方）

统计：分桶（笔数/胜率/平均收益）+ 相关性 + 盈利vs亏损均值。
口径：对齐生产（slow=480 + vol过滤 + 退出链全开）。

输出：feature_report/ma48_analysis.md
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


def backtest_with_ma48(symbol, params, bars, ma48, ma288):
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
                ei = pos["entry_idx"]
                trades.append(_make(entry, ret * 100.0, reason, ma48, ma288, ei))
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
        ei = pos["entry_idx"]
        trades.append(_make(pos["entry_price"], ret * 100.0, "持仓到结束", ma48, ma288, ei))
    return trades


def _make(entry, ret_pct, reason, ma48, ma288, i):
    d = {"ret_pct": ret_pct, "reason": reason}
    m48 = ma48[i]
    m288 = ma288[i]
    if m48 is not None:
        d["close_to_ma48"] = (entry - m48) / m48 * 100.0
        if i >= 5 and ma48[i - 5] is not None and ma48[i - 5] != 0:
            d["ma48_slope_5"] = (m48 - ma48[i - 5]) / ma48[i - 5] * 100.0
        if i >= 20 and ma48[i - 20] is not None and ma48[i - 20] != 0:
            d["ma48_slope_20"] = (m48 - ma48[i - 20]) / ma48[i - 20] * 100.0
    if m48 is not None and m288 is not None and m288 != 0:
        d["ma48_ma288_spread"] = (m48 - m288) / m288 * 100.0
    return d


def bucket(ts, key, bins, add):
    add(f"| 区间 | 笔数 | 胜率 | 平均收益 | 总收益 |")
    add("|---|---|---|---|---|")
    for lo, hi, lab in bins:
        b = [t for t in ts if t.get(key) is not None and lo <= t[key] < hi]
        if not b:
            continue
        n = len(b)
        w = sum(1 for t in b if t["ret_pct"] > 0)
        add(f"| {lab} | {n} | {w/n*100:.1f}% | {sum(t['ret_pct'] for t in b)/n:+.3f}% | {sum(t['ret_pct'] for t in b):+.1f}% |")
    add("")


def pearson(xs, ys):
    n = len(xs)
    if n < 3:
        return None
    mx = sum(xs) / n
    my = sum(ys) / n
    sxy = sum((x - mx) * (y - my) for x, y in zip(xs, ys))
    sxx = sum((x - mx) ** 2 for x in xs)
    syy = sum((y - my) ** 2 for y in ys)
    if sxx == 0 or syy == 0:
        return None
    return sxy / (sxx ** 0.5 * syy ** 0.5)


def main() -> int:
    md: List[str] = []
    add = md.append
    add("# MA48 指标与盈利/亏损的关系分析")
    add("")
    add("- MA48 = SMA(close,48)，30m 上 = 24 小时均线。口径对齐生产（slow=480 + vol过滤 + 退出链全开）。")
    add("")

    KEYS = ["close_to_ma48", "ma48_slope_5", "ma48_slope_20", "ma48_ma288_spread"]

    for coin in ["BTCUSDT", "ETHUSDT", "SOLUSDT", "BNBUSDT", "SUIUSDT", "HYPEUSDT"]:
        params = dc.SYMBOL_PARAMS[coin]
        bars = load_klines_30m(coin)
        closes = [b.close for b in bars]
        ma48 = sma_series(closes, 48)
        ma288 = sma_series(closes, 288)
        ts = backtest_with_ma48(coin, params, bars, ma48, ma288)

        add(f"## {coin}  （{len(ts)} 笔，胜率 {sum(1 for t in ts if t['ret_pct']>0)/len(ts)*100:.1f}%）")
        add("")

        add("### 1. 价格相对 MA48 close_to_ma48（正=MA48上方）")
        add("")
        bucket(ts, "close_to_ma48", [(-1e9, -3, "<-3%"), (-3, -1, "-3~-1%"), (-1, 0, "-1~0%"), (0, 1, "0~1%"), (1, 3, "1~3%"), (3, 6, "3~6%"), (6, 1e9, "≥6%")], add)

        add("### 2. MA48 斜率 ma48_slope_5（正=向上）")
        add("")
        bucket(ts, "ma48_slope_5", [(-1e9, -1, "<-1%"), (-1, -0.3, "-1~-0.3%"), (-0.3, 0.3, "-0.3~0.3%"), (0.3, 1, "0.3~1%"), (1, 1e9, "≥1%")], add)

        add("### 3. MA48 斜率 ma48_slope_20（正=向上）")
        add("")
        bucket(ts, "ma48_slope_20", [(-1e9, -3, "<-3%"), (-3, -1, "-3~-1%"), (-1, 0, "-1~0%"), (0, 1, "0~1%"), (1, 3, "1~3%"), (3, 1e9, "≥3%")], add)

        add("### 4. MA48 相对 MA288 ma48_ma288_spread（正=短期在中期上方）")
        add("")
        bucket(ts, "ma48_ma288_spread", [(-1e9, -3, "<-3%"), (-3, -1, "-3~-1%"), (-1, 0, "-1~0%"), (0, 1, "0~1%"), (1, 3, "1~3%"), (3, 1e9, "≥3%")], add)

        add("### 5. 各维度相关性")
        add("")
        add("| 维度 | 相关系数 | 样本数 |")
        add("|---|---|---|")
        for k in KEYS:
            pairs = [(t[k], t["ret_pct"]) for t in ts if t.get(k) is not None]
            if len(pairs) < 30:
                continue
            add(f"| {k} | {pearson([p[0] for p in pairs], [p[1] for p in pairs]):+.3f} | {len(pairs)} |")
        add("")

        add("### 6. 盈利单 vs 亏损单 均值")
        add("")
        add("| 维度 | 盈利单均值 | 亏损单均值 | 差值 |")
        add("|---|---|---|---|")
        wins = [t for t in ts if t["ret_pct"] > 0]
        losses = [t for t in ts if t["ret_pct"] <= 0]
        for k in KEYS:
            wv = [t[k] for t in wins if t.get(k) is not None]
            lv = [t[k] for t in losses if t.get(k) is not None]
            if not wv or not lv:
                continue
            mw = sum(wv) / len(wv)
            ml = sum(lv) / len(lv)
            add(f"| {k} | {mw:.3f} | {ml:.3f} | {mw-ml:+.3f} |")
        add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "ma48_analysis.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
