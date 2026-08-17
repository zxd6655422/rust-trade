"""5m 双均线回踩策略（参数照搬 30m 生产配置）。

- 用 5m K 线计算 MA288/MA480，趋势 + 收盘穿越 MA288 入场。
- 退出链：硬止损 → MA288止损 → 移动止盈 → 趋势反转（全开，对齐生产）。
- vol 过滤：realized_vol_48（48 根 5m bar 波动率）>= realized_vol_threshold 跳过。
- 参数完全沿用 data_config.SYMBOL_PARAMS（slow=480 + 30m 的 realized_vol_threshold）。

输出：feature_report/5m_strategy_report.md
"""
from __future__ import annotations

import csv
import math
import os
from datetime import datetime
from typing import List, Dict, Any, Optional

import data_config as dc
from ma_trend_pullback import KlineBar

BASE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# 数据已移到仓库外：rust-projects/data_2026-08-13
DATA = os.path.join(os.path.dirname(os.path.dirname(BASE)), "data_2026-08-13")

CSV_5M = {
    "BTCUSDT": "kline_5m_202608131243_BTC.csv",
    "ETHUSDT": "kline_5m_202608131246_ETH.csv",
    "SOLUSDT": "kline_5m_202608131248_SOL.csv",
    "BNBUSDT": "kline_5m_202608141531_BNB.csv",
    "SUIUSDT": "kline_5m_202608141535_SUI.csv",
    "HYPEUSDT": "kline_5m_202608141538_HYPE.csv",
}


def load_klines_5m(symbol):
    path = os.path.join(DATA, CSV_5M[symbol])
    bars = []
    with open(path, "r", encoding="utf-8", newline="") as f:
        reader = csv.DictReader(f)
        for row in reader:
            dt = datetime.strptime(row["open_time"], "%Y-%m-%d %H:%M:%S.%f %z")
            bars.append(KlineBar(
                open_time=int(dt.timestamp() * 1000),
                open=float(row["open"]), high=float(row["high"]),
                low=float(row["low"]), close=float(row["close"]), volume=float(row["volume"]),
            ))
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


def backtest(symbol, params, bars):
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
    vol_skipped = 0

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
                               "mfe": pos["max_profit"], "bars": i - pos["entry_idx"]})
                pos = None
                continue

        if pos is None and fast_ma is not None and slow_ma is not None and prev_fast_ma is not None:
            if vol48 is not None and vol48[i] is not None and vol48[i] >= params.realized_vol_threshold:
                vol_skipped += 1
                continue
            if fast_ma > slow_ma and prev_close < prev_fast_ma and close > fast_ma:
                hs = close * (1.0 - params.hard_stop_pct / 100.0)
                pos = {"side": "LONG", "entry_price": close, "entry_idx": i, "hard_stop_price": hs, "max_profit": 0.0}
            elif fast_ma < slow_ma and prev_close > prev_fast_ma and close < fast_ma:
                hs = close * (1.0 + params.hard_stop_pct / 100.0)
                pos = {"side": "SHORT", "entry_price": close, "entry_idx": i, "hard_stop_price": hs, "max_profit": 0.0}

    if pos is not None:
        ret = (closes[-1] - pos["entry_price"]) / pos["entry_price"] if pos["side"] == "LONG" else (pos["entry_price"] - closes[-1]) / pos["entry_price"]
        trades.append({"ret": ret, "ret_pct": ret * 100.0, "reason": "持仓到结束",
                       "mfe": pos["max_profit"], "bars": n - 1 - pos["entry_idx"]})
    return trades, vol_skipped


def metrics(ts):
    n = len(ts)
    if n == 0:
        return dict(n=0, wr=0, simple=0, comp=0)
    wins = [t for t in ts if t["ret_pct"] > 0]
    eq = 1.0
    for t in ts:
        eq *= (1.0 + t["ret"])
    return dict(n=n, wr=len(wins) / n * 100, simple=sum(t["ret_pct"] for t in ts), comp=(eq - 1.0) * 100)


def main() -> int:
    md: List[str] = []
    add = md.append
    add("# 5m 双均线回踩策略（参数照搬 30m 生产）")
    add("")
    add("- 5m K 线，MA288/MA480，趋势 + 收盘穿越入场；退出链全开；vol 过滤（48 根 5m bar）。")
    add("- 参数 = data_config.SYMBOL_PARAMS（slow=480 + 30m realized_vol_threshold）。")
    add("")
    add("| 币种 | 交易数 | 胜率 | 简单收益 | 复利收益 | vol跳过 | 平仓原因(硬/MA288/止盈/反转/结束) |")
    add("|---|---|---|---|---|---|---|")
    for sym in dc.SYMBOLS:
        params = dc.SYMBOL_PARAMS[sym]
        bars = load_klines_5m(sym)
        ts, vs = backtest(sym, params, bars)
        m = metrics(ts)
        from collections import Counter
        rc = Counter(t["reason"] for t in ts)
        reasons = f"{rc.get('硬止损',0)}/{rc.get('MA288止损',0)}/{rc.get('移动止盈',0)}/{rc.get('趋势反转',0)}/{rc.get('持仓到结束',0)}"
        add(f"| {sym} | {m['n']} | {m['wr']:.1f}% | {m['simple']:+.2f}% | {m['comp']:+.2f}% | {vs} | {reasons} |")
    add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "5m_strategy_report.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    # 控制台打印
    print(f"{'币种':10} {'笔数':>6} {'胜率':>7} {'简单':>10} {'复利':>10}")
    for sym in dc.SYMBOLS:
        params = dc.SYMBOL_PARAMS[sym]
        bars = load_klines_5m(sym)
        ts, vs = backtest(sym, params, bars)
        m = metrics(ts)
        print(f"{sym:10} {m['n']:>6} {m['wr']:>6.1f}% {m['simple']:>+10.2f}% {m['comp']:>+10.2f}%")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
