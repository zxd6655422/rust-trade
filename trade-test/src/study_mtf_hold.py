"""30m 精确入场 + 4h 趋势作为持有/离场依据（多时间框架核心方案）。

入场：30m 趋势（MA288>MA480）+ 收盘穿越 MA288（同现有）。
离场（替代 30m MA288 止损）：
  1. 硬止损（hard_stop_pct）
  2. 4h 趋势转下降（4h close 下穿 4h MA60，做多）→ 离场
  3. 移动止盈 activate+callback（锁利，沿用 A1）

关键：持仓期间不因 30m MA288 穿越离场，而是看 4h 宏观趋势，
期望在「台阶式上涨」中扛住 30m 级别的回调，吃满大趋势。

输出：feature_report/mtf_hold_report.md
"""
from __future__ import annotations

import os
from bisect import bisect_right
from datetime import datetime

import data_config as dc
from loader import load_klines_30m, load_klines_4h
from study_adaptive_ma_trailing import precompute, comp


BAR_30M_MS = 30 * 60 * 1000
BAR_4H_MS = 4 * 60 * 60 * 1000


def sma_series(closes, period):
    n = len(closes)
    p = [0.0] * (n + 1)
    for i in range(n):
        p[i + 1] = p[i] + closes[i]
    out = [None] * n
    for i in range(period - 1, n):
        out[i] = (p[i + 1] - p[i + 1 - period]) / period
    return out


def backtest_mtf_hold(symbol, params, bars30, bars4, ma4_period, activate, callback, y0=None, y1=None):
    fast = params.fast_ma_period
    slow = params.slow_ma_period
    n = len(bars30)
    years = [datetime.fromtimestamp(b.open_time / 1000).year for b in bars30]
    closes = [b.close for b in bars30]
    pre = precompute(bars30)
    vol48 = pre["vol48"]
    prefix = pre["prefix"]

    # 4h MA
    closes4 = [b.close for b in bars4]
    ma4 = sma_series(closes4, ma4_period)
    ts4 = [b.open_time for b in bars4]

    def fourh_bearish(et):
        """当前 30m bar 收盘前，最近一根【已收盘】4h bar 是否 close < MA（趋势转空）。

        无 lookahead：4h bar open_time=t 的收盘时刻为 t+4h；在 30m bar 收盘(et+30m) 时
        已收盘 ⟺ t+4h <= et+30m ⟺ t <= et+30m-4h。用 bisect_right 定位已收盘 bar。
        """
        j = bisect_right(ts4, et + BAR_30M_MS - BAR_4H_MS) - 1
        if j < 0 or ma4[j] is None:
            return False
        return closes4[j] < ma4[j]

    def sma_at(idx, period):
        if idx + 1 < period:
            return None
        return (prefix[idx + 1] - prefix[idx + 1 - period]) / period

    trades = []
    pos = None
    for i in range(n):
        if i + 1 < slow:
            continue
        if y0 is not None and not (y0 <= years[i] <= y1):
            continue
        close = closes[i]
        prev_close = closes[i - 1]
        fast_ma = sma_at(i, fast)
        slow_ma = sma_at(i, slow)
        prev_fast_ma = sma_at(i - 1, fast)

        if pos is not None:
            bar = bars30[i]
            side = pos["side"]
            entry = pos["entry"]
            pnl = (close - entry) / entry * 100.0 if side == "LONG" else (entry - close) / entry * 100.0
            pos["max_profit"] = max(pos["max_profit"], pnl)
            exit_price = None
            reason = ""
            # 1. 硬止损
            if params.hard_stop_pct > 0.0:
                if side == "LONG" and bar.low <= pos["hard_stop"]:
                    exit_price, reason = pos["hard_stop"], "硬止损"
                elif side == "SHORT" and bar.high >= pos["hard_stop"]:
                    exit_price, reason = pos["hard_stop"], "硬止损"
            # 2. 4h 趋势转空（替代 MA288 止损）
            if exit_price is None:
                if side == "LONG" and fourh_bearish(bars30[i].open_time):
                    exit_price, reason = close, "4h转空"
                elif side == "SHORT" and not fourh_bearish(bars30[i].open_time):
                    exit_price, reason = close, "4h转多"
            # 3. 移动止盈
            if exit_price is None and pos["max_profit"] >= activate and pos["max_profit"] - pnl >= callback:
                exit_price, reason = close, "移动止盈"
            if exit_price is not None:
                ret = (exit_price - entry) / entry if side == "LONG" else (entry - exit_price) / entry
                trades.append({"ret_pct": ret * 100.0, "reason": reason,
                               "entry_idx": pos["entry_idx"], "exit_idx": i,
                               "mfe_pct": pos["max_profit"], "hold_bars": i - pos["entry_idx"]})
                pos = None
                continue

        if pos is None and fast_ma is not None and slow_ma is not None and prev_fast_ma is not None:
            if vol48[i] is not None and vol48[i] >= params.realized_vol_threshold:
                continue
            if fast_ma > slow_ma and prev_close < prev_fast_ma and close > fast_ma:
                hs = close * (1.0 - params.hard_stop_pct / 100.0)
                pos = {"side": "LONG", "entry": close, "entry_idx": i, "hard_stop": hs, "max_profit": 0.0}
            elif fast_ma < slow_ma and prev_close > prev_fast_ma and close < fast_ma:
                hs = close * (1.0 + params.hard_stop_pct / 100.0)
                pos = {"side": "SHORT", "entry": close, "entry_idx": i, "hard_stop": hs, "max_profit": 0.0}

    if pos is not None:
        ret = (closes[-1] - pos["entry"]) / pos["entry"] if pos["side"] == "LONG" else (pos["entry"] - closes[-1]) / pos["entry"]
        trades.append({"ret_pct": ret * 100.0, "reason": "持仓到结束", "entry_idx": pos["entry_idx"], "exit_idx": n - 1,
                       "mfe_pct": pos["max_profit"], "hold_bars": n - 1 - pos["entry_idx"]})
    return trades


def main() -> int:
    md = []
    add = md.append
    add("# 30m 入场 + 4h 趋势持有（替代 30m MA288 止损）")
    add("")
    add("- 离场：硬止损 → 4h close 转空(MA60) → 移动止盈。持仓期间不因 30m MA288 穿越离场。")
    add("")

    for coin in ["BTCUSDT", "ETHUSDT", "SOLUSDT"]:
        params = dc.SYMBOL_PARAMS[coin]
        bars30 = load_klines_30m(coin)
        bars4 = load_klines_4h(coin)
        years = [datetime.fromtimestamp(b.open_time / 1000).year for b in bars30]
        y0, y1 = min(years), max(years)

        add(f"## {coin}")
        add("")
        add("| ma4周期 | activate | callback | 全样本复利 | 2024复利 | 2025复利 |")
        add("|---|---|---|---|---|---|")
        for ma4_p in [40, 60, 90]:
            for act, cb in [(4.0, 1.0), (4.0, 1.5)]:
                full = comp([t["ret_pct"] for t in backtest_mtf_hold(coin, params, bars30, bars4, ma4_p, act, cb)])
                r24 = comp([t["ret_pct"] for t in backtest_mtf_hold(coin, params, bars30, bars4, ma4_p, act, cb, 2024, 2024)])
                r25 = comp([t["ret_pct"] for t in backtest_mtf_hold(coin, params, bars30, bars4, ma4_p, act, cb, 2025, 2025)])
                add(f"| MA{ma4_p} | {act}% | {cb}% | {full:+.1f}% | {r24:+.1f}% | {r25:+.1f}% |")
        add("")
        add("> 对照：30m A1 基线 2024/2025 复利、A10/A11 见前报告。")
        add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "mtf_hold_report.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
