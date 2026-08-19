r"""A13 审查脚本：复现 + 4h lookahead 检查 + 时间切分 + 手续费敏感性。

回答三个问题：
  1. A13 的 trend / cross_only 数字能否复现（对照 mtf_slope_entry_report.md）
  2. 4h MA40 离场是否存在 lookahead（用了尚未收盘的 4h bar 的 future close）
  3. cross_only 是否稳健（时间切分 + 手续费）

运行：
  cd D:\dev-projects\rust-trade\trade-test\src
  python verify_a13.py
"""
from __future__ import annotations

import os
from bisect import bisect_left, bisect_right
from datetime import datetime, timezone, timedelta

import data_config as dc
from loader import load_klines_30m, load_klines_4h
from study_adaptive_ma_trailing import comp, precompute

BJ = timezone(timedelta(hours=8))
SRC = os.path.dirname(os.path.abspath(__file__))
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


def backtest(symbol, params, bars30, bars4, mode="trend", fix_lookahead=False,
             y0=None, y1=None, ma4_period=40, activate=4.0, callback=1.0):
    """与 study_mtf_slope_entry.backtest_entry_mode 同构。
    fix_lookahead=True 时，4h 离场只用「已收盘」的 4h bar（排除未来 close）。
    """
    fast = params.fast_ma_period
    slow = params.slow_ma_period
    n = len(bars30)
    years = [datetime.fromtimestamp(b.open_time / 1000).year for b in bars30]
    closes = [b.close for b in bars30]
    pre = precompute(bars30)
    vol48 = pre["vol48"]
    prefix = pre["prefix"]

    closes4 = [b.close for b in bars4]
    ma4 = sma_series(closes4, ma4_period)
    ts4 = [b.open_time for b in bars4]

    def fourh_bearish(et):
        if fix_lookahead:
            # 在 30m bar 收盘(et+30m)时刻，已收盘的 4h bar 满足 open_time + 4h <= et+30m
            # 即 open_time <= et + 30m - 4h = et - 210m
            j = bisect_right(ts4, et + BAR_30M_MS - BAR_4H_MS) - 1
        else:
            j = bisect_left(ts4, et) - 1
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
            if params.hard_stop_pct > 0.0:
                if side == "LONG" and bar.low <= pos["hard_stop"]:
                    exit_price, reason = pos["hard_stop"], "硬止损"
                elif side == "SHORT" and bar.high >= pos["hard_stop"]:
                    exit_price, reason = pos["hard_stop"], "硬止损"
            if exit_price is None:
                if side == "LONG" and fourh_bearish(bars30[i].open_time):
                    exit_price, reason = close, "4h转空"
                elif side == "SHORT" and not fourh_bearish(bars30[i].open_time):
                    exit_price, reason = close, "4h转多"
            if exit_price is None and pos["max_profit"] >= activate and pos["max_profit"] - pnl >= callback:
                exit_price, reason = close, "移动止盈"

            if exit_price is not None:
                ret = (exit_price - entry) / entry if side == "LONG" else (entry - exit_price) / entry
                trades.append({"ret_pct": ret * 100.0, "reason": reason, "side": side,
                               "entry_idx": pos["entry_idx"], "exit_idx": i,
                               "mfe_pct": pos["max_profit"], "year": years[i]})
                pos = None
                continue

        if pos is None and fast_ma is not None and slow_ma is not None and prev_fast_ma is not None:
            if vol48[i] is not None and vol48[i] >= params.realized_vol_threshold:
                continue
            if mode == "trend":
                long_ok, short_ok = fast_ma > slow_ma, fast_ma < slow_ma
            else:  # cross_only
                long_ok, short_ok = True, True
            if long_ok and prev_close < prev_fast_ma and close > fast_ma:
                pos = {"side": "LONG", "entry": close, "entry_idx": i,
                       "hard_stop": close * (1.0 - params.hard_stop_pct / 100.0), "max_profit": 0.0}
            elif short_ok and prev_close > prev_fast_ma and close < fast_ma:
                pos = {"side": "SHORT", "entry": close, "entry_idx": i,
                       "hard_stop": close * (1.0 + params.hard_stop_pct / 100.0), "max_profit": 0.0}

    if pos is not None:
        ret = (closes[-1] - pos["entry"]) / pos["entry"] if pos["side"] == "LONG" else (pos["entry"] - closes[-1]) / pos["entry"]
        trades.append({"ret_pct": ret * 100.0, "reason": "持仓到结束", "side": pos["side"],
                       "entry_idx": pos["entry_idx"], "exit_idx": n - 1,
                       "mfe_pct": pos["max_profit"], "year": years[-1]})
    return trades


def stat(trades, fee=0.0):
    rets = [t["ret_pct"] - fee * 100 for t in trades]
    wins = [r for r in rets if r > 0]
    losses = [r for r in rets if r <= 0]
    return {
        "n": len(rets),
        "win": len(wins) / len(rets) * 100 if rets else 0.0,
        "simple": sum(rets),
        "compound": comp(rets),
        "avg_win": sum(wins) / len(wins) if wins else 0.0,
        "avg_loss": sum(losses) / len(losses) if losses else 0.0,
    }


def lookahead_probe(symbol):
    """量化 4h 离场选用的 bar 是否「尚未收盘」（lookahead）。"""
    bars30 = load_klines_30m(symbol)
    bars4 = load_klines_4h(symbol)
    ts4 = [b.open_time for b in bars4]
    off = {"in_progress": 0, "closed": 0, "gap_min_max": 0}
    gaps = []
    for b in bars30:
        et = b.open_time
        j = bisect_left(ts4, et) - 1  # 当前代码
        if j < 0:
            continue
        t4 = ts4[j]
        # 该 4h bar 的收盘时刻 = t4 + 4h；相对 30m bar 收盘(et+30m) 的未来时长（分钟）
        gap_min = (t4 + BAR_4H_MS - (et + BAR_30M_MS)) / 60000.0
        gaps.append(gap_min)
        if t4 + BAR_4H_MS <= et + BAR_30M_MS:
            off["closed"] += 1
        else:
            off["in_progress"] += 1
    off["gap_min_max"] = max(gaps) if gaps else 0.0
    off["gap_median"] = sorted(gaps)[len(gaps) // 2] if gaps else 0.0
    return off


def main() -> int:
    md = []
    add = md.append
    add("# A13 审查结果")
    add("")

    # ---- Part 1: lookahead probe ----
    add("## 1. 4h 离场 lookahead 检查")
    add("")
    add("当前代码 `bisect_left(ts4, et)-1` 取的是「最近一根 open_time < 当前30m bar open」的 4h bar。")
    add("该 4h bar 的收盘时刻 = open_time + 4h，若它 > 30m bar 收盘时刻，则是用了尚未收盘的未来 close = lookahead。")
    add("")
    add("| 币种 | 30m bar 总数 | 选中的4h bar未收盘(占比) | 已收盘(占比) | 未来时长中位(分钟) | 最大(分钟) |")
    add("|---|---:|---:|---:|---:|---:|")
    for coin in dc.SYMBOLS:
        off = lookahead_probe(coin)
        tot = off["in_progress"] + off["closed"]
        pct = off["in_progress"] / tot * 100 if tot else 0
        add(f"| {coin} | {tot} | {off['in_progress']} ({pct:.0f}%) | {off['closed']} ({100-pct:.0f}%) | {off['gap_median']:.0f} | {off['gap_min_max']:.0f} |")
    add("")

    # ---- Part 2: 复现 + lookahead 修正对比 ----
    add("## 2. 复现 A13 数字 + lookahead 修正对比（全样本，0 手续费）")
    add("")
    add("| 币种 | 入场 | lookahead | 笔数 | 胜率 | 简单% | 复利% |")
    add("|---|---|---:|---:|---:|---:|---:|")
    for coin in dc.SYMBOLS:
        params = dc.SYMBOL_PARAMS[coin]
        bars30 = load_klines_30m(coin)
        bars4 = load_klines_4h(coin)
        for mode, label in [("trend", "趋势"), ("cross_only", "cross_only")]:
            for fix, fl in [(False, "保留"), (True, "修正")]:
                tr = backtest(coin, params, bars30, bars4, mode=mode, fix_lookahead=fix)
                s = stat(tr)
                add(f"| {coin} | {label} | {fl} | {s['n']} | {s['win']:.1f} | {s['simple']:+.1f} | {s['compound']:+.1f} |")
    add("")

    # ---- Part 3: 时间切分 cross_only vs trend ----
    add("## 3. 时间切分验证（cross_only vs trend，0 手续费）")
    add("")
    add("| 币种 | 段 | trend 复利 | cross_only 复利 | 提升 |")
    add("|---|---|---:|---:|---:|")
    for coin in dc.SYMBOLS:
        params = dc.SYMBOL_PARAMS[coin]
        bars30 = load_klines_30m(coin)
        bars4 = load_klines_4h(coin)
        years = [datetime.fromtimestamp(b.open_time / 1000).year for b in bars30]
        y0, y1 = min(years), max(years)
        mid = (y0 + y1) // 2
        segs = [("全样本", None, None), (f"前半 {y0}-{mid}", y0, mid), (f"后半 {mid+1}-{y1}", mid + 1, y1)]
        for seg, a, b in segs:
            t = stat(backtest(coin, params, bars30, bars4, mode="trend", y0=a, y1=b))
            c = stat(backtest(coin, params, bars30, bars4, mode="cross_only", y0=a, y1=b))
            add(f"| {coin} | {seg} | {t['compound']:+.1f} | {c['compound']:+.1f} | {c['compound']-t['compound']:+.1f}pp |")
    add("")

    # ---- Part 4: 手续费敏感性（cross_only vs trend）----
    add("## 4. 手续费敏感性（全样本复利）")
    add("")
    add("| 币种 | 入场 | 0.0% | 0.1% | 0.2% | 0.4% |")
    add("|---|---|---:|---:|---:|---:|")
    for coin in ["BTCUSDT", "ETHUSDT", "SOLUSDT"]:
        params = dc.SYMBOL_PARAMS[coin]
        bars30 = load_klines_30m(coin)
        bars4 = load_klines_4h(coin)
        for mode, label in [("trend", "趋势"), ("cross_only", "cross_only")]:
            cells = []
            for fee in [0.0, 0.001, 0.002, 0.004]:
                tr = backtest(coin, params, bars30, bars4, mode=mode)
                cells.append(f"{stat(tr, fee=fee)['compound']:+.1f}")
            add(f"| {coin} | {label} | " + " | ".join(cells) + " |")
    add("")

    out = os.path.join(SRC, "feature_report", "a13_verify_report.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print("\n".join(md))
    print(f"\n[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
