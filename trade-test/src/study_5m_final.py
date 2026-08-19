"""5m 完整最优：5m 最优参数（任务2）+ 5m 专属 vol 阈值（任务1 基础上重扫）。

在 5m 最优参数（hard_stop/activate/callback）基础上，用循环内过滤重新扫描 realized_vol_48 阈值，
找「最优参数 × 最优阈值」的完整口径。

输出：feature_report/5m_final_report.md
"""
from __future__ import annotations

import csv
import math
import os
from datetime import datetime
from typing import List, Dict, Any, Optional

from ma_trend_pullback import KlineBar, Params

BASE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# 数据已移到仓库外：rust-projects/data_2026-08-13
DATA = os.path.join(os.path.dirname(os.path.dirname(BASE)), "data_2026-08-13")

CSV_5M = {
    "BTCUSDT": "kline_5m_BTC.csv",
    "ETHUSDT": "kline_5m_ETH.csv",
    "SOLUSDT": "kline_5m_SOL.csv",
    "BNBUSDT": "kline_5m_BNB.csv",
    "SUIUSDT": "kline_5m_SUI.csv",
    "HYPEUSDT": "kline_5m_HYPE.csv",
}

# 5m 最优参数（任务2 Top1）：{symbol: (hard_stop, activate, callback)}
BEST_PARAMS = {
    "BTCUSDT": (0.5, 1.0, 1.5),
    "ETHUSDT": (1.0, 1.0, 1.5),
    "SOLUSDT": (0.5, 1.0, 1.0),
    "BNBUSDT": (0.5, 1.0, 1.5),
    "SUIUSDT": (1.0, 2.0, 1.5),
    "HYPEUSDT": (0.5, 3.0, 1.0),
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
    return Params(
        fast_ma_period=288, slow_ma_period=480,
        stop_mode="ma288", hard_stop_pct=hs,
        take_profit_mode="trailing", trailing_activate_pct=act, trailing_callback_pct=cb,
        slope_threshold=0.0, bbw_threshold=0.0, vol_threshold=0.0, realized_vol_threshold=0.0,
        use_30m_expanding=False, use_5m_expanding=False, min_angle_5m=0.0, entry_timeframe="30m",
    )


def backtest(symbol, params, bars, vol_threshold):
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
            if vol_threshold > 0.0 and vol48[i] is not None and vol48[i] >= vol_threshold:
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
    add("# 5m 完整最优（最优参数 × 最优 vol 阈值）")
    add("")
    add("- 参数 = 任务2 最优（逐币种）；在此基础上循环内重扫 realized_vol_48 阈值。")
    add("")
    add("| 币种 | 参数(hs/act/cb) | 阈值 | 笔数 | 胜率 | 简单 | 复利 |")
    add("|---|---|---|---|---|---|---|")

    final = {}
    for sym in ["BTCUSDT", "ETHUSDT", "SOLUSDT", "BNBUSDT", "SUIUSDT", "HYPEUSDT"]:
        hs, act, cb = BEST_PARAMS[sym]
        params = make_params(hs, act, cb)
        bars = load_klines_5m(sym)
        # 先无过滤跑，收集 vol 分布
        base_rets = backtest(sym, params, bars, 0.0)
        # 收集入场 vol（重新算一遍，或用 vol48 分布近似）
        closes = [b.close for b in bars]
        vol48 = realized_vol_48_series(closes)
        vols = [v for v in vol48 if v is not None]
        vols_sorted = sorted(vols)
        # 阈值候选：分位数 P60~P95
        cand = [0.0]
        for q in (0.60, 0.70, 0.75, 0.80, 0.85, 0.90, 0.95):
            cand.append(vols_sorted[min(len(vols_sorted) - 1, int(q * len(vols_sorted)))])
        best = None
        for thr in cand:
            rets = backtest(sym, params, bars, thr)
            m = metrics(rets)
            if best is None or m["comp"] > best[1]:
                best = (thr, m["comp"], m)
        thr, comp, m = best
        final[sym] = (hs, act, cb, thr, m)
        add(f"| {sym} | {hs}/{act}/{cb} | {thr:.3f} | {m['n']} | {m['wr']:.1f}% | {m['simple']:+.2f}% | {m['comp']:+.2f}% |")
    add("")
    add("> 注意：均为 2024-2026 样本内最优，5m 数据仅 2.5 年，样本外验证待补。")
    add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "5m_final_report.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    for sym, (hs, act, cb, thr, m) in final.items():
        print(f"{sym:10} {hs}/{act}/{cb} thr={thr:.3f} n={m['n']} wr={m['wr']:.1f}% comp={m['comp']:+.2f}%")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
