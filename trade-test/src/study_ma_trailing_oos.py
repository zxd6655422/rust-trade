"""MA 动态止盈线 · 时间切分样本外验证。

用现有 9 年数据（2017-2026）做两个切分：
  Split A：训练 2017-2021 → 验证 2022-2026
  Split B：训练 2022-2026 → 验证 2017-2021

每个切分：训练段扫 72 组合找复利最优，用该参数跑验证段，对比验证段基线（移动止盈）。
若两个切分里「训练最优参数在验证段」都跑赢基线，说明方向稳健；否则过拟合。

输出：feature_report/ma_trailing_oos_report.md
"""
from __future__ import annotations

import math
import os
from datetime import datetime
from itertools import product
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


def backtest(symbol, params, bars, ma_series, mode, activate_pct, confirm_bars, y0, y1):
    """mode='base' 基线移动止盈；mode='ma' MA动态止盈。仅处理 y0~y1 年份的 bar。"""
    fast = params.fast_ma_period
    slow = params.slow_ma_period
    n = len(bars)
    closes = [b.close for b in bars]
    vol48 = realized_vol_48_series(closes)
    years = [datetime.fromtimestamp(b.open_time / 1000).year for b in bars]
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
        if not (y0 <= years[i] <= y1):
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
            if exit_price is None:
                if mode == "base":
                    if pos["max_profit"] >= params.trailing_activate_pct and pos["max_profit"] - pnl >= params.trailing_callback_pct:
                        exit_price = close
                else:
                    if pos["max_profit"] >= activate_pct and ma_series[i] is not None:
                        ma_v = ma_series[i]
                        pos["below_count"] = pos["below_count"] + 1 if ((side == "LONG" and close < ma_v) or (side == "SHORT" and close > ma_v)) else 0
                        if pos["below_count"] >= confirm_bars:
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
                pos = {"side": "LONG", "entry_price": close, "entry_idx": i, "hard_stop_price": hs, "max_profit": 0.0, "below_count": 0}
            elif fast_ma < slow_ma and prev_close > prev_fast_ma and close < fast_ma:
                hs = close * (1.0 + params.hard_stop_pct / 100.0)
                pos = {"side": "SHORT", "entry_price": close, "entry_idx": i, "hard_stop_price": hs, "max_profit": 0.0, "below_count": 0}

    if pos is not None and y0 <= years[pos["entry_idx"]] <= y1:
        ret = (closes[-1] - pos["entry_price"]) / pos["entry_price"] if pos["side"] == "LONG" else (pos["entry_price"] - closes[-1]) / pos["entry_price"]
        trades.append(ret)
    return trades


def comp(rets):
    eq = 1.0
    for r in rets:
        eq *= (1.0 + r)
    return (eq - 1.0) * 100.0


def main() -> int:
    md: List[str] = []
    add = md.append
    add("# MA 动态止盈线 时间切分样本外验证")
    add("")
    add("- Split A：训练 2017-2021 → 验证 2022-2026；Split B：训练 2022-2026 → 验证 2017-2021。")
    add("")

    MA_PERIODS = [48, 96, 192]
    ACTS = [2.0, 4.0, 6.0, 8.0, 10.0, 15.0]
    CONFIRMS = [1, 3, 5, 10]
    SPLITS = [("A", 2017, 2021, 2022, 2026), ("B", 2022, 2026, 2017, 2021)]

    for coin in ["BTCUSDT", "ETHUSDT", "SOLUSDT"]:
        params = dc.SYMBOL_PARAMS[coin]
        bars = load_klines_30m(coin)
        closes = [b.close for b in bars]
        ma_map = {p: sma_series(closes, p) for p in MA_PERIODS}

        add(f"## {coin}")
        add("")
        for sp_name, tr0, tr1, va0, va1 in SPLITS:
            # 训练段基线 + 扫矩阵
            base_train = comp(backtest(coin, params, bars, None, "base", 0, 0, tr0, tr1))
            best = None
            for mp, act, cb in product(MA_PERIODS, ACTS, CONFIRMS):
                c = comp(backtest(coin, params, bars, ma_map[mp], "ma", act, cb, tr0, tr1))
                if best is None or c > best[1]:
                    best = (mp, act, cb, c)
            # 验证段：基线 vs 训练最优
            base_val = comp(backtest(coin, params, bars, None, "base", 0, 0, va0, va1))
            val = comp(backtest(coin, params, bars, ma_map[best[0]], "ma", best[1], best[2], va0, va1))
            add(f"### Split {sp_name}（训练 {tr0}-{tr1}，验证 {va0}-{va1}）")
            add("")
            add(f"- 训练段基线复利：{base_train:+.1f}%")
            add(f"- 训练段最优：MA{best[0]} + activate{best[1]}% + confirm{best[2]}根 → 训练复利 {best[3]:+.1f}%")
            add(f"- **验证段：基线 {base_val:+.1f}% vs 最优参数 {val:+.1f}% → {'✅ 跑赢' if val > base_val else '❌ 跑输'}**")
            add("")
    add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "ma_trailing_oos_report.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
