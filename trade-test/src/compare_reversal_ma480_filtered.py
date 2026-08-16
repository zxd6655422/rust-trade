"""MA480 + realized_vol_48 过滤（最新结论）基础上：反转出场 ON vs OFF 收益对比。

- slow_ma_period = 480（对齐生产配置）；fast=288。
- 过滤：入场时 realized_vol_48 >= 逐币种阈值 则跳过（循环内跳过，对齐 Rust analyze / JS 第十六次）。
  阈值 = studies/001 标定 + 生产 SQL：BTC 0.426, ETH 0.445, SOL 0.790, BNB 0.488, SUI 0.788, HYPE 0.646。
- 退出链：硬止损 > MA288止损 > 移动止盈 > [ON时]趋势反转(MA288 vs MA480 交叉)。
- 输出：每币种汇总 + 每币种×每年 收益明细（简单 %）。
"""
from __future__ import annotations

import math
import os
from typing import List, Dict, Any, Optional

import backtest as bt
import data_config as dc
from loader import load_klines_30m
from ma_trend_pullback import Params

VOL_THRESHOLDS = {
    "BTCUSDT": 0.426, "ETHUSDT": 0.445, "SOLUSDT": 0.790,
    "BNBUSDT": 0.488, "SUIUSDT": 0.788, "HYPEUSDT": 0.646,
}


def make_params(base: Params, slow: int) -> Params:
    return Params(
        fast_ma_period=base.fast_ma_period, slow_ma_period=slow,
        stop_mode=base.stop_mode, hard_stop_pct=base.hard_stop_pct,
        take_profit_mode=base.take_profit_mode,
        trailing_activate_pct=base.trailing_activate_pct,
        trailing_callback_pct=base.trailing_callback_pct,
        slope_threshold=0.0, bbw_threshold=0.0, vol_threshold=0.0,
        use_30m_expanding=False, use_5m_expanding=False, min_angle_5m=0.0,
        entry_timeframe="30m",
    )


def realized_vol_48_series(closes: List[float]) -> List[Optional[float]]:
    """48 周期收益率 population std * 100（对齐 Rust calculate_realized_vol_48 / indicators.py）。"""
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
    out: List[Optional[float]] = [None] * n
    for i in range(W, n):
        mean = (p[i + 1] - p[i + 1 - W]) / W
        msq = (p2[i + 1] - p2[i + 1 - W]) / W
        var = msq - mean * mean
        if var < 0.0:
            var = 0.0
        out[i] = math.sqrt(var) * 100.0
    return out


def backtest_toggle(symbol, params, bars, vol_threshold: float, use_reversal: bool) -> List[Dict[str, Any]]:
    """回测（含 vol 过滤 + 可选趋势反转出场），记录入场年份。"""
    import datetime
    fast = params.fast_ma_period
    slow = params.slow_ma_period
    n = len(bars)
    closes = [b.close for b in bars]
    vol48 = realized_vol_48_series(closes)

    prefix = [0.0] * (n + 1)
    for i in range(n):
        prefix[i + 1] = prefix[i] + closes[i]

    def sma_at(idx: int, period: int) -> Optional[float]:
        if idx + 1 < period:
            return None
        return (prefix[idx + 1] - prefix[idx + 1 - period]) / period

    trades: List[Dict[str, Any]] = []
    pos: Optional[Dict[str, Any]] = None
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
            exit_price: Optional[float] = None
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

            if use_reversal and exit_price is None:
                if side == "LONG" and fast_ma < slow_ma:
                    exit_price, reason = close, "趋势反转"
                elif side == "SHORT" and fast_ma > slow_ma:
                    exit_price, reason = close, "趋势反转"

            if exit_price is not None:
                ret = (exit_price - entry) / entry if side == "LONG" else (entry - exit_price) / entry
                year = datetime.datetime.fromtimestamp(pos["entry_time"] / 1000).year
                trades.append({"symbol": symbol, "ret": ret, "ret_pct": ret * 100.0,
                               "reason": reason, "bars": i - pos["entry_idx"], "entry_year": str(year)})
                pos = None
                continue

        if pos is None and fast_ma is not None and slow_ma is not None and prev_fast_ma is not None:
            # vol 过滤：高波动跳过入场
            if vol_threshold > 0.0 and vol48[i] is not None and vol48[i] >= vol_threshold:
                vol_skipped += 1
                continue

            if fast_ma > slow_ma:
                if prev_close < prev_fast_ma and close > fast_ma:
                    hard_stop = close * (1.0 - params.hard_stop_pct / 100.0) if params.hard_stop_pct > 0.0 else fast_ma * 0.98
                    pos = {"side": "LONG", "entry_price": close, "entry_idx": i,
                           "entry_time": bars[i].open_time, "hard_stop_price": hard_stop, "max_profit": 0.0}
            elif fast_ma < slow_ma:
                if prev_close > prev_fast_ma and close < fast_ma:
                    hard_stop = close * (1.0 + params.hard_stop_pct / 100.0) if params.hard_stop_pct > 0.0 else fast_ma * 1.02
                    pos = {"side": "SHORT", "entry_price": close, "entry_idx": i,
                           "entry_time": bars[i].open_time, "hard_stop_price": hard_stop, "max_profit": 0.0}

    if pos is not None:
        import datetime
        entry = pos["entry_price"]
        side = pos["side"]
        ret = (closes[-1] - entry) / entry if side == "LONG" else (entry - closes[-1]) / entry
        year = datetime.datetime.fromtimestamp(pos["entry_time"] / 1000).year
        trades.append({"symbol": symbol, "ret": ret, "ret_pct": ret * 100.0,
                       "reason": "持仓到结束", "bars": n - 1 - pos["entry_idx"], "entry_year": str(year)})

    return trades


def yearly(ts):
    from collections import defaultdict
    d = defaultdict(float)
    c = defaultdict(int)
    for t in ts:
        d[t["entry_year"]] += t["ret_pct"]
        c[t["entry_year"]] += 1
    return d, c


def main() -> int:
    SLOW = 480
    md: List[str] = []
    add = md.append
    add(f"# MA{SLOW} + vol过滤（最新结论）基础上：反转出场 ON vs OFF")
    add("")
    add(f"- slow_ma_period = {SLOW}；fast=288；入场时 realized_vol_48 >= 逐币种阈值 跳过（循环内过滤）。")
    add("- 退出链：硬止损 → MA288止损 → 移动止盈 → [ON时]趋势反转(MA288 vs MA{SLOW} 交叉)。")
    add("- 简单收益 = 每笔 ret_pct 直接相加；复利 = 资金曲线连乘。未计手续费/滑点。")
    add("")

    add("## 1. 每币种汇总：反转 ON vs OFF（过滤后）")
    add("")
    add("| 币种 | vol阈值 | ON简单 | ON复利 | OFF简单 | OFF复利 | 简单差(ON-OFF) | 复利差(ON-OFF) | ON笔数 | OFF笔数 | ON反转出场笔数 |")
    add("|---|---|---|---|---|---|---|---|---|---|---|")
    summary = {}
    for sym in dc.SYMBOLS:
        base = dc.SYMBOL_PARAMS[sym]
        params = make_params(base, SLOW)
        thr = VOL_THRESHOLDS[sym]
        bars = load_klines_30m(sym)
        on_t = backtest_toggle(sym, params, bars, thr, True)
        off_t = backtest_toggle(sym, params, bars, thr, False)
        on_m = bt.compute_metrics(on_t)
        off_m = bt.compute_metrics(off_t)
        n_rev = on_m["reason_cnt"].get("趋势反转", 0)
        summary[sym] = (on_t, off_t, on_m, off_m)
        add(f"| {sym} | {thr} | {on_m['total_ret']:+.2f}% | {on_m['compound_ret']:+.2f}% | "
            f"{off_m['total_ret']:+.2f}% | {off_m['compound_ret']:+.2f}% | "
            f"{on_m['total_ret']-off_m['total_ret']:+.2f}% | {on_m['compound_ret']-off_m['compound_ret']:+.2f}% | "
            f"{on_m['n']} | {off_m['n']} | {n_rev} |")
    add("")

    add("## 2. 每币种 × 每年 收益明细（过滤后，简单 %）")
    add("")
    for sym in dc.SYMBOLS:
        on_t, off_t, on_m, off_m = summary[sym]
        y_on, c_on = yearly(on_t)
        y_off, c_off = yearly(off_t)
        years = sorted(set(y_on) | set(y_off))
        add(f"### {sym}")
        add("")
        add("| 年份 | ON收益 | ON笔数 | OFF收益 | OFF笔数 | 差值(ON-OFF) |")
        add("|---|---|---|---|---|---|")
        for y in years:
            ov = y_on.get(y, 0.0)
            fv = y_off.get(y, 0.0)
            add(f"| {y} | {ov:+.2f}% | {c_on.get(y, 0)} | {fv:+.2f}% | {c_off.get(y, 0)} | {ov-fv:+.2f}% |")
        add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "compare_reversal_ma480_filtered_report.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))

    print(f"{'币种':10} {'ON简单':>10} {'ON复利':>10} {'OFF简单':>10} {'OFF复利':>10} {'复利差':>9} {'反转笔数':>7}")
    print("-" * 72)
    for sym in dc.SYMBOLS:
        on_t, off_t, on_m, off_m = summary[sym]
        n_rev = on_m["reason_cnt"].get("趋势反转", 0)
        print(f"{sym:10} {on_m['total_ret']:>+10.2f}% {on_m['compound_ret']:>+10.2f}% "
              f"{off_m['total_ret']:>+10.2f}% {off_m['compound_ret']:>+10.2f}% "
              f"{on_m['compound_ret']-off_m['compound_ret']:>+9.2f}% {n_rev:>7}")

    print(f"\n[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
