"""MA480 口径下：保留「趋势反转出场」 vs 取消「趋势反转出场」 收益对比。

- slow_ma_period = 480（对齐生产配置），其余参数沿用 data_config.SYMBOL_PARAMS。
- 退出链：硬止损 > MA288止损 > 移动止盈 > [可选]趋势反转(MA288 vs MA480 交叉)。
- 输出：每币种汇总对比 + 每币种×每年 收益明细（简单收益 = 每笔 ret_pct 相加）。
"""
from __future__ import annotations

import os
from typing import List, Dict, Any, Optional

import backtest as bt
import data_config as dc
from loader import load_klines_30m
from ma_trend_pullback import Params


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


def backtest_toggle(symbol, params, bars, use_reversal: bool) -> List[Dict[str, Any]]:
    """与 backtest.backtest 一致，含可选趋势反转出场，并记录入场年份。"""
    fast = params.fast_ma_period
    slow = params.slow_ma_period
    n = len(bars)
    closes = [b.close for b in bars]

    prefix = [0.0] * (n + 1)
    for i in range(n):
        prefix[i + 1] = prefix[i] + closes[i]

    def sma_at(idx: int, period: int) -> Optional[float]:
        if idx + 1 < period:
            return None
        return (prefix[idx + 1] - prefix[idx + 1 - period]) / period

    import datetime
    trades: List[Dict[str, Any]] = []
    pos: Optional[Dict[str, Any]] = None

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
    add(f"# MA{SLOW} 口径：反转出场 ON vs OFF 收益对比")
    add("")
    add(f"- slow_ma_period = {SLOW}（对齐生产配置）；fast=288；退出链：硬止损 → MA288止损 → 移动止盈 → [ON时]趋势反转(MA288 vs MA{SLOW} 交叉)。")
    add("- 简单收益 = 每笔 ret_pct 直接相加；复利 = 资金曲线连乘。未计手续费/滑点。")
    add("")

    # 汇总
    add("## 1. 每币种汇总：反转 ON vs OFF")
    add("")
    add("| 币种 | ON简单 | ON复利 | OFF简单 | OFF复利 | 简单差(ON-OFF) | 复利差(ON-OFF) | ON笔数 | OFF笔数 | ON反转出场笔数 |")
    add("|---|---|---|---|---|---|---|---|---|---|")
    summary = {}
    for sym in dc.SYMBOLS:
        base = dc.SYMBOL_PARAMS[sym]
        params = make_params(base, SLOW)
        bars = load_klines_30m(sym)
        on_t = backtest_toggle(sym, params, bars, True)
        off_t = backtest_toggle(sym, params, bars, False)
        on_m = bt.compute_metrics(on_t)
        off_m = bt.compute_metrics(off_t)
        n_rev = on_m["reason_cnt"].get("趋势反转", 0)
        summary[sym] = (on_t, off_t, on_m, off_m)
        add(f"| {sym} | {on_m['total_ret']:+.2f}% | {on_m['compound_ret']:+.2f}% | "
            f"{off_m['total_ret']:+.2f}% | {off_m['compound_ret']:+.2f}% | "
            f"{on_m['total_ret']-off_m['total_ret']:+.2f}% | {on_m['compound_ret']-off_m['compound_ret']:+.2f}% | "
            f"{on_m['n']} | {off_m['n']} | {n_rev} |")
    add("")

    # 每币种 × 每年明细
    add("## 2. 每币种 × 每年 收益明细（简单 %）")
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

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "compare_reversal_ma480_report.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))

    # 控制台汇总
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
