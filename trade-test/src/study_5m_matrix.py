"""任务2：5m 参数矩阵（硬止损 × activate × callback）。

5m 单笔波动更小，网格比 30m 更紧：硬止损 {0.5,1.0,1.5}%、activate {1,2,3,4}%、callback {0.5,1,1.5}%。
固定 fast=288/slow=480；先不加 vol 过滤（纯参数网格），排序以复利为准。

输出：feature_report/5m_matrix_report.md
"""
from __future__ import annotations

import csv
import math
import os
from datetime import datetime
from itertools import product
from typing import List, Dict, Any

import data_config as dc
from ma_trend_pullback import KlineBar, Params

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

HARD_STOPS = [0.5, 1.0, 1.5]
ACTIVATES = [1.0, 2.0, 3.0, 4.0]
CALLBACKS = [0.5, 1.0, 1.5]


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


def make_params(base: Params, hs, act, cb):
    return Params(
        fast_ma_period=base.fast_ma_period, slow_ma_period=base.slow_ma_period,
        stop_mode="ma288", hard_stop_pct=hs,
        take_profit_mode="trailing", trailing_activate_pct=act, trailing_callback_pct=cb,
        slope_threshold=0.0, bbw_threshold=0.0, vol_threshold=0.0, realized_vol_threshold=0.0,
        use_30m_expanding=False, use_5m_expanding=False, min_angle_5m=0.0, entry_timeframe="30m",
    )


def backtest(symbol, params, bars):
    fast = params.fast_ma_period
    slow = params.slow_ma_period
    n = len(bars)
    closes = [b.close for b in bars]
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
    add("# 5m 参数矩阵（硬止损 × activate × callback）")
    add("")
    add("- 网格：硬止损 {0.5,1.0,1.5}%、activate {1,2,3,4}%、callback {0.5,1,1.5}%；fast=288/slow=480；不加 vol 过滤。")
    add("")

    for sym in dc.SYMBOLS:
        base = dc.SYMBOL_PARAMS[sym]
        bars = load_klines_5m(sym)
        add(f"## {sym}")
        add("")
        grid = []
        for hs, act, cb in product(HARD_STOPS, ACTIVATES, CALLBACKS):
            params = make_params(base, hs, act, cb)
            rets = backtest(sym, params, bars)
            m = metrics(rets)
            grid.append((hs, act, cb, m))
        grid.sort(key=lambda x: -x[3]["comp"])
        add("### Top 10（按复利）")
        add("")
        add("| 硬止损 | activate | callback | 笔数 | 胜率 | 简单 | 复利 |")
        add("|---|---|---|---|---|---|---|")
        for hs, act, cb, m in grid[:10]:
            add(f"| {hs}% | {act}% | {cb}% | {m['n']} | {m['wr']:.1f}% | {m['simple']:+.2f}% | {m['comp']:+.2f}% |")
        add("")
        # 30m 生产参数对照（同网格内的 hs/act/cb）
        prod = (base.hard_stop_pct, base.trailing_activate_pct, base.trailing_callback_pct)
        for hs, act, cb, m in grid:
            if (hs, act, cb) == prod:
                add(f"**30m生产参数对照（{hs}/{act}/{cb}）：复利 {m['comp']:+.2f}%，胜率 {m['wr']:.1f}%**")
                add("")
        # 最优
        best = grid[0]
        add(f"**最优：硬止损{best[0]}% / activate{best[1]}% / callback{best[2]}% → 复利 {best[3]['comp']:+.2f}%**")
        add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "5m_matrix_report.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
