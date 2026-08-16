"""任务1：标定 5m 专属 realized_vol_threshold（逐币种）。

方法（对齐研究 001）：
  1. 跑 5m 回测（无 vol 过滤，参数沿用 30m），记录每笔交易的入场 realized_vol_48 与收益。
  2. 逐币种输出 realized_vol_48 分位数。
  3. 扫描阈值：入场时 vol >= 阈值 则过滤（事后过滤），看保留交易的简单/复利收益。

输出：feature_report/5m_threshold_calibration.md
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
DATA = os.path.join(BASE, "data_2026-08-13")

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


def backtest_record(symbol, params, bars):
    """无 vol 过滤回测，记录每笔交易的入场 realized_vol_48 与收益。"""
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
                trades.append({"ret": ret, "ret_pct": ret * 100.0, "reason": reason,
                               "entry_vol": vol48[pos["entry_idx"]]})
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
        trades.append({"ret": ret, "ret_pct": ret * 100.0, "reason": "持仓到结束", "entry_vol": vol48[pos["entry_idx"]]})
    return trades


def quantile(vals, q):
    s = sorted(vals)
    return s[min(len(s) - 1, int(q * len(s)))]


def main() -> int:
    md: List[str] = []
    add = md.append
    add("# 5m realized_vol_threshold 标定（逐币种）")
    add("")
    add("- 方法：无过滤 5m 回测，事后按入场 realized_vol_48 阈值过滤（对齐研究 001）。")
    add("- 注意：5m 数据仅 2024-01~2026（约 2.5 年），样本内标定，需注意过拟合。")
    add("")

    for sym in dc.SYMBOLS:
        params = dc.SYMBOL_PARAMS[sym]
        bars = load_klines_5m(sym)
        trades = backtest_record(sym, params, bars)
        vols = [t["entry_vol"] for t in trades if t["entry_vol"] is not None]
        if not vols:
            continue
        add(f"## {sym}  （{len(trades)} 笔，无过滤简单 {sum(t['ret_pct'] for t in trades):+.2f}%）")
        add("")
        add("### realized_vol_48 分位数")
        add("")
        add(f"P10={quantile(vols,0.1):.3f}  P25={quantile(vols,0.25):.3f}  P50={quantile(vols,0.5):.3f}  "
            f"P75={quantile(vols,0.75):.3f}  P90={quantile(vols,0.9):.3f}")
        add("")
        add("### 阈值扫描（vol >= 阈值 过滤）")
        add("")
        add("| 阈值 | 保留笔数 | 过滤笔数 | 保留简单 | 保留复利 | 过滤掉的总收益 |")
        add("|---|---|---|---|---|---|")
        all_ret = sum(t["ret_pct"] for t in trades)
        best = None
        for q in (0.0, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9):
            thr = quantile(vols, q)
            keep = [t for t in trades if t["entry_vol"] is None or t["entry_vol"] < thr]
            rem = [t for t in trades if t["entry_vol"] is not None and t["entry_vol"] >= thr]
            eq = 1.0
            for t in keep:
                eq *= (1.0 + t["ret"])
            comp = (eq - 1.0) * 100.0
            keep_simple = sum(t["ret_pct"] for t in keep)
            rem_ret = sum(t["ret_pct"] for t in rem)
            add(f"| {thr:.3f} | {len(keep)} | {len(rem)} | {keep_simple:+.2f}% | {comp:+.2f}% | {rem_ret:+.2f}% |")
            if best is None or comp > best[1]:
                best = (thr, comp, len(keep))
        if best:
            add("")
            add(f"**最优阈值 ≈ {best[0]:.3f}（复利 {best[1]:+.2f}%，保留 {best[2]} 笔）**")
        add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "5m_threshold_calibration.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
