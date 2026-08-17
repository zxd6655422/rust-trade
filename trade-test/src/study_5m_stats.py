"""5m 策略多维度统计（最优口径）。

用 5m 最优参数 + 最优 vol 阈值，逐币输出：
  1) 汇总（胜率、简单/复利、最大回撤、盈亏比、利润因子、最大盈利/亏损）
  2) 平仓原因分布
  3) 盈利 vs 亏损 持仓画像（MFE/MAE/持仓bar）
  4) 分年度收益

输出：feature_report/5m_stats_report.md
"""
from __future__ import annotations

import csv
import math
import os
from collections import defaultdict
from datetime import datetime
from typing import List, Dict, Any

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

BEST = {
    "BTCUSDT": (0.5, 1.0, 1.5, 0.143),
    "ETHUSDT": (1.0, 1.0, 1.5, 0.356),
    "SOLUSDT": (0.5, 1.0, 1.0, 0.369),
    "BNBUSDT": (0.5, 1.0, 1.5, 0.0),
    "SUIUSDT": (1.0, 2.0, 1.5, 0.568),
    "HYPEUSDT": (0.5, 3.0, 1.0, 0.0),
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
    return Params(fast_ma_period=288, slow_ma_period=480, stop_mode="ma288", hard_stop_pct=hs,
                  take_profit_mode="trailing", trailing_activate_pct=act, trailing_callback_pct=cb,
                  slope_threshold=0.0, bbw_threshold=0.0, vol_threshold=0.0, realized_vol_threshold=0.0,
                  use_30m_expanding=False, use_5m_expanding=False, min_angle_5m=0.0, entry_timeframe="30m")


def backtest_full(symbol, params, bars, vol_threshold):
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
            pos["min_profit"] = min(pos["min_profit"], pnl)
            exit_price = None
            reason = ""
            if params.hard_stop_pct > 0.0:
                if side == "LONG" and bar.low <= pos["hard_stop_price"]:
                    exit_price, reason = pos["hard_stop_price"], "硬止损"
                elif side == "SHORT" and bar.high >= pos["hard_stop_price"]:
                    exit_price, reason = pos["hard_stop_price"], "硬止损"
            if exit_price is None and prev_fast_ma is not None:
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
                year = datetime.fromtimestamp(pos["entry_time"] / 1000).year
                trades.append({"side": side, "ret": ret, "ret_pct": ret * 100.0, "reason": reason,
                               "bars": i - pos["entry_idx"], "mfe": pos["max_profit"], "mae": pos["min_profit"],
                               "year": year})
                pos = None
                continue

        if pos is None and fast_ma is not None and slow_ma is not None and prev_fast_ma is not None:
            if vol_threshold > 0.0 and vol48[i] is not None and vol48[i] >= vol_threshold:
                continue
            if fast_ma > slow_ma and prev_close < prev_fast_ma and close > fast_ma:
                hs = close * (1.0 - params.hard_stop_pct / 100.0)
                pos = {"side": "LONG", "entry_price": close, "entry_idx": i, "entry_time": bars[i].open_time,
                       "hard_stop_price": hs, "max_profit": 0.0, "min_profit": 0.0}
            elif fast_ma < slow_ma and prev_close > prev_fast_ma and close < fast_ma:
                hs = close * (1.0 + params.hard_stop_pct / 100.0)
                pos = {"side": "SHORT", "entry_price": close, "entry_idx": i, "entry_time": bars[i].open_time,
                       "hard_stop_price": hs, "max_profit": 0.0, "min_profit": 0.0}

    if pos is not None:
        ret = (closes[-1] - pos["entry_price"]) / pos["entry_price"] if pos["side"] == "LONG" else (pos["entry_price"] - closes[-1]) / pos["entry_price"]
        year = datetime.fromtimestamp(pos["entry_time"] / 1000).year
        trades.append({"side": pos["side"], "ret": ret, "ret_pct": ret * 100.0, "reason": "持仓到结束",
                       "bars": n - 1 - pos["entry_idx"], "mfe": pos["max_profit"], "mae": pos["min_profit"], "year": year})
    return trades


def mean(xs):
    return sum(xs) / len(xs) if xs else 0.0


def main() -> int:
    md: List[str] = []
    add = md.append
    add("# 5m 策略多维度统计（最优口径）")
    add("")

    # 汇总表
    add("## 1. 汇总")
    add("")
    add("| 币种 | 交易数 | 胜率 | 简单收益 | 复利收益 | 最大回撤 | 盈亏比 | 利润因子 | 最大盈利 | 最大亏损 | 平均持仓bar |")
    add("|---|---|---|---|---|---|---|---|---|---|---|")
    all_trades = []
    for sym in ["BTCUSDT", "ETHUSDT", "SOLUSDT", "BNBUSDT", "SUIUSDT", "HYPEUSDT"]:
        hs, act, cb, thr = BEST[sym]
        params = make_params(hs, act, cb)
        bars = load_klines_5m(sym)
        ts = backtest_full(sym, params, bars, thr)
        for t in ts:
            t["symbol"] = sym
        all_trades.extend(ts)
        n = len(ts)
        wins = [t for t in ts if t["ret_pct"] > 0]
        losses = [t for t in ts if t["ret_pct"] <= 0]
        wr = len(wins) / n * 100 if n else 0
        simple = sum(t["ret_pct"] for t in ts)
        eq = 1.0
        peak = 1.0
        dd = 0.0
        for t in ts:
            eq *= (1.0 + t["ret"])
            peak = max(peak, eq)
            dd = max(dd, (peak - eq) / peak)
        comp = (eq - 1.0) * 100
        avg_win = mean([t["ret_pct"] for t in wins])
        avg_loss = mean([t["ret_pct"] for t in losses])
        payoff = avg_win / abs(avg_loss) if avg_loss != 0 else float('inf')
        pf = sum(t["ret_pct"] for t in wins) / abs(sum(t["ret_pct"] for t in losses)) if losses else float('inf')
        max_w = max(t["ret_pct"] for t in ts)
        max_l = min(t["ret_pct"] for t in ts)
        avg_bars = mean([t["bars"] for t in ts])
        pr = f"{payoff:.2f}" if payoff != float('inf') else "∞"
        pf_s = f"{pf:.2f}" if pf != float('inf') else "∞"
        add(f"| {sym} | {n} | {wr:.1f}% | {simple:+.2f}% | {comp:+.2f}% | {dd*100:.1f}% | {pr} | {pf_s} | {max_w:+.2f}% | {max_l:+.2f}% | {avg_bars:.1f} |")
    add("")

    # 平仓原因分布
    add("## 2. 平仓原因分布（全 6 币合计）")
    add("")
    add("| 平仓原因 | 笔数 | 占比 | 胜率 | 总收益 | 平均收益 |")
    add("|---|---|---|---|---|---|")
    rc = defaultdict(lambda: {"n": 0, "w": 0, "ret": 0.0})
    for t in all_trades:
        rc[t["reason"]]["n"] += 1
        if t["ret_pct"] > 0:
            rc[t["reason"]]["w"] += 1
        rc[t["reason"]]["ret"] += t["ret_pct"]
    for r, d in sorted(rc.items(), key=lambda kv: -kv[1]["n"]):
        add(f"| {r} | {d['n']} | {d['n']/len(all_trades)*100:.1f}% | {d['w']/d['n']*100:.1f}% | {d['ret']:+.2f}% | {d['ret']/d['n']:+.2f}% |")
    add("")

    # 盈利 vs 亏损画像
    add("## 3. 盈利 vs 亏损 持仓画像（全 6 币合计）")
    add("")
    add("| 分组 | 笔数 | 平均持仓bar | 平均MFE% | 平均MAE% | 平均收益% |")
    add("|---|---|---|---|---|---|")
    for name, ts in [("全部", all_trades),
                     ("盈利", [t for t in all_trades if t["ret_pct"] > 0]),
                     ("亏损", [t for t in all_trades if t["ret_pct"] <= 0]),
                     ("大幅盈利(≥+2%)", [t for t in all_trades if t["ret_pct"] >= 2]),
                     ("大幅亏损(≤-1%)", [t for t in all_trades if t["ret_pct"] <= -1])]:
        if not ts:
            continue
        add(f"| {name} | {len(ts)} | {mean([t['bars'] for t in ts]):.1f} | {mean([t['mfe'] for t in ts]):+.2f} | {mean([t['mae'] for t in ts]):+.2f} | {mean([t['ret_pct'] for t in ts]):+.2f} |")
    add("")

    # 分年度收益
    add("## 4. 分年度收益（简单 %，全 6 币）")
    add("")
    add("| 年份 | BTC | ETH | SOL | BNB | SUI | HYPE |")
    add("|---|---|---|---|---|---|---|")
    years = sorted({t["year"] for t in all_trades})
    by_sym_year = defaultdict(lambda: defaultdict(float))
    for t in all_trades:
        by_sym_year[t["symbol"]][t["year"]] += t["ret_pct"]
    for y in years:
        cells = [f"{by_sym_year[s][y]:+.1f}%" for s in ["BTCUSDT", "ETHUSDT", "SOLUSDT", "BNBUSDT", "SUIUSDT", "HYPEUSDT"]]
        add(f"| {y} | " + " | ".join(cells) + " |")
    add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "5m_stats_report.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
