r"""A12 MTF 入场条件对比实验：原始趋势条件 vs 无趋势过滤 vs MA288 斜率条件。

保持离场逻辑完全不变（硬止损 → 4h MA40 止损 → 移动止盈 4%+1%），
只替换入场条件中的方向判断：

  trend      : 原条件  MA288 > MA480 做多 / MA288 < MA480 做空（A12 现状）
  cross_only : 无趋势过滤，收盘穿越 MA288 即入场
  slope      : MA288 斜率 > +eps 做多 / < -eps 做空；走平(|斜率|<=eps) 不进场

输出：
  feature_report/mtf_slope_entry_report.md
  feature_report/mtf_slope_entry_trades.csv

运行：
  cd D:\dev-projects\rust-trade\trade-test\src
  python study_mtf_slope_entry.py
"""
from __future__ import annotations

import csv
import os
from bisect import bisect_right
from datetime import datetime, timezone, timedelta
from typing import Dict, List, Any

import data_config as dc
from loader import load_klines_30m, load_klines_4h
from study_adaptive_ma_trailing import comp, precompute

BJ = timezone(timedelta(hours=8))
BAR_30M_MS = 30 * 60 * 1000
BAR_4H_MS = 4 * 60 * 60 * 1000
SRC = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(SRC, "feature_report")


def sma_series(closes: List[float], period: int) -> List[float | None]:
    n = len(closes)
    p = [0.0] * (n + 1)
    for i in range(n):
        p[i + 1] = p[i] + closes[i]
    out = [None] * n
    for i in range(period - 1, n):
        out[i] = (p[i + 1] - p[i + 1 - period]) / period
    return out


def backtest_entry_mode(
    symbol: str,
    params: Any,
    bars30: List,
    bars4: List,
    mode: str = "trend",
    slope_lookback: int = 10,
    slope_eps: float = 0.05,
    ma4_period: int = 40,
    activate: float = 4.0,
    callback: float = 1.0,
) -> List[Dict]:
    """与 study_mtf_all_coins.backtest_mtf_hold 同构，仅入场方向判断不同。"""
    fast = params.fast_ma_period
    slow = params.slow_ma_period
    n = len(bars30)
    years = [datetime.fromtimestamp(b.open_time / 1000).year for b in bars30]
    closes = [b.close for b in bars30]
    pre = precompute(bars30)
    vol48 = pre["vol48"]
    prefix = pre["prefix"]
    ma288s = pre["ma288"]

    # 4h MA
    closes4 = [b.close for b in bars4]
    ma4 = sma_series(closes4, ma4_period)
    ts4 = [b.open_time for b in bars4]

    def fourh_bearish(et: int) -> bool:
        # 无 lookahead：用当前 30m bar 收盘(et+30m) 时已收盘的 4h bar（t+4h <= et+30m）
        j = bisect_right(ts4, et + BAR_30M_MS - BAR_4H_MS) - 1
        if j < 0 or ma4[j] is None:
            return False
        return closes4[j] < ma4[j]

    def sma_at(idx: int, period: int) -> float | None:
        if idx + 1 < period:
            return None
        return (prefix[idx + 1] - prefix[idx + 1 - period]) / period

    def ma288_slope(i: int) -> float | None:
        if i - slope_lookback < 0:
            return None
        a = ma288s[i]
        b = ma288s[i - slope_lookback]
        if a is None or b is None or b == 0.0:
            return None
        return (a - b) / b * 100.0

    def entry_ok(i: int, fast_ma: float | None, slow_ma: float | None,
                  prev_fast_ma: float | None) -> tuple[bool, bool]:
        """返回 (long_ok, short_ok)。"""
        if mode == "trend":
            if fast_ma is None or slow_ma is None:
                return False, False
            return fast_ma > slow_ma, fast_ma < slow_ma
        if mode == "cross_only":
            return True, True
        if mode == "slope":
            sl = ma288_slope(i)
            if sl is None:
                return False, False
            return sl > slope_eps, sl < -slope_eps
        raise ValueError(f"unknown mode: {mode}")

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

        # 持仓中
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
                entry_time = datetime.fromtimestamp(bars30[pos["entry_idx"]].open_time / 1000, tz=BJ)
                exit_time = datetime.fromtimestamp(bars30[i].open_time / 1000, tz=BJ)
                trades.append({
                    "symbol": symbol,
                    "mode": mode,
                    "ret_pct": ret * 100.0,
                    "reason": reason,
                    "side": side,
                    "entry_idx": pos["entry_idx"],
                    "exit_idx": i,
                    "entry_time": entry_time.strftime("%Y-%m-%d %H:%M"),
                    "exit_time": exit_time.strftime("%Y-%m-%d %H:%M"),
                    "mfe_pct": pos["max_profit"],
                    "hold_bars": i - pos["entry_idx"],
                    "year": years[i],
                })
                pos = None
                continue

        # 开仓
        if pos is None and fast_ma is not None and slow_ma is not None and prev_fast_ma is not None:
            if vol48[i] is not None and vol48[i] >= params.realized_vol_threshold:
                continue
            long_ok, short_ok = entry_ok(i, fast_ma, slow_ma, prev_fast_ma)
            if long_ok and prev_close < prev_fast_ma and close > fast_ma:
                hs = close * (1.0 - params.hard_stop_pct / 100.0)
                pos = {"side": "LONG", "entry": close, "entry_idx": i, "hard_stop": hs, "max_profit": 0.0}
            elif short_ok and prev_close > prev_fast_ma and close < fast_ma:
                hs = close * (1.0 + params.hard_stop_pct / 100.0)
                pos = {"side": "SHORT", "entry": close, "entry_idx": i, "hard_stop": hs, "max_profit": 0.0}

    if pos is not None:
        ret = (closes[-1] - pos["entry"]) / pos["entry"] if pos["side"] == "LONG" else (pos["entry"] - closes[-1]) / pos["entry"]
        entry_time = datetime.fromtimestamp(bars30[pos["entry_idx"]].open_time / 1000, tz=BJ)
        exit_time = datetime.fromtimestamp(bars30[-1].open_time / 1000, tz=BJ)
        trades.append({
            "symbol": symbol,
            "mode": mode,
            "ret_pct": ret * 100.0,
            "reason": "持仓到结束",
            "side": pos["side"],
            "entry_idx": pos["entry_idx"],
            "exit_idx": n - 1,
            "entry_time": entry_time.strftime("%Y-%m-%d %H:%M"),
            "exit_time": exit_time.strftime("%Y-%m-%d %H:%M"),
            "mfe_pct": pos["max_profit"],
            "hold_bars": n - 1 - pos["entry_idx"],
            "year": years[-1],
        })

    return trades


def max_drawdown(rets: List[float]) -> float:
    eq = 1.0
    peak = 1.0
    dd = 0.0
    for r in rets:
        eq *= (1.0 + r / 100.0)
        peak = max(peak, eq)
        dd = max(dd, (peak - eq) / peak)
    return dd * 100.0


def summarize(trades: List[Dict]) -> Dict[str, Any]:
    n = len(trades)
    if n == 0:
        return {"n": 0, "win_rate": 0.0, "simple": 0.0, "compound": 0.0,
                "max_dd": 0.0, "avg_win": 0.0, "avg_loss": 0.0,
                "mfe_ge_4": 0, "mfe_ge_4_loss": 0}
    rets = [t["ret_pct"] for t in trades]
    wins = [t["ret_pct"] for t in trades if t["ret_pct"] > 0]
    losses = [t["ret_pct"] for t in trades if t["ret_pct"] <= 0]
    mfe_ge = [t for t in trades if t["mfe_pct"] >= 4.0]
    mfe_ge_loss = [t for t in mfe_ge if t["ret_pct"] <= 0]
    return {
        "n": n,
        "win_rate": len(wins) / n * 100.0,
        "simple": sum(rets),
        "compound": comp(rets),
        "max_dd": max_drawdown(rets),
        "avg_win": sum(wins) / len(wins) if wins else 0.0,
        "avg_loss": sum(losses) / len(losses) if losses else 0.0,
        "mfe_ge_4": len(mfe_ge),
        "mfe_ge_4_loss": len(mfe_ge_loss),
    }


VARIANTS = [
    ("trend", "原始趋势 MA288>MA480", {}),
    ("cross_only", "无趋势过滤(纯穿越MA288)", {}),
    ("slope_L5_e0", "MA288斜率5bar, eps=0.00%", {"slope_lookback": 5, "slope_eps": 0.0}),
    ("slope_L5_e05", "MA288斜率5bar, eps=0.05%", {"slope_lookback": 5, "slope_eps": 0.05}),
    ("slope_L10_e0", "MA288斜率10bar, eps=0.00%", {"slope_lookback": 10, "slope_eps": 0.0}),
    ("slope_L10_e05", "MA288斜率10bar, eps=0.05%", {"slope_lookback": 10, "slope_eps": 0.05}),
    ("slope_L10_e10", "MA288斜率10bar, eps=0.10%", {"slope_lookback": 10, "slope_eps": 0.10}),
    ("slope_L20_e0", "MA288斜率20bar, eps=0.00%", {"slope_lookback": 20, "slope_eps": 0.0}),
    ("slope_L20_e05", "MA288斜率20bar, eps=0.05%", {"slope_lookback": 20, "slope_eps": 0.05}),
]


def main() -> int:
    os.makedirs(OUT, exist_ok=True)
    md: List[str] = []
    add = md.append
    add("# A12 MTF 入场条件对比：趋势 vs 无过滤 vs MA288 斜率")
    add("")
    add("> 离场逻辑全部保持一致：硬止损 → 4h MA40 止损 → 移动止盈 4%+1%。")
    add("> 未计手续费/滑点。`mfe>=4` 表示移动止盈激活线达到过的笔数。")
    add("")

    all_trades: List[Dict] = []

    for coin in dc.SYMBOLS:
        params = dc.SYMBOL_PARAMS[coin]
        bars30 = load_klines_30m(coin)
        bars4 = load_klines_4h(coin)
        add(f"## {coin}")
        add("")
        add("| 入场条件 | 交易数 | 胜率 | 简单% | 复利% | 最大回撤% | 平均盈利% | 平均亏损% | mfe>=4 | 其中亏损出场 |")
        add("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|")

        for key, name, kw in VARIANTS:
            mode = "slope" if key.startswith("slope") else key
            trades = backtest_entry_mode(coin, params, bars30, bars4, mode=mode, **kw)
            for t in trades:
                t["mode"] = key
            s = summarize(trades)
            add(f"| {name} | {s['n']} | {s['win_rate']:.1f} | {s['simple']:+.1f} | {s['compound']:+.1f} | "
                f"{s['max_dd']:.1f} | {s['avg_win']:+.2f} | {s['avg_loss']:+.2f} | {s['mfe_ge_4']} | {s['mfe_ge_4_loss']} |")
            all_trades.extend(trades)
        add("")

    add("---")
    add("")
    add("## 说明")
    add("- `slope` 条件：MA288 在最近 L 根 30m 的变化幅度（%）。`slope > +eps` 只做多；`slope < -eps` 只做空；`|slope|<=eps` 视为走平，不进场。")
    add("- 穿越入场条件（收盘上穿/下穿 MA288）在所有变体中保持不变。")
    add("- 建议先看 `cross_only` 与 `trend` 的差距，再比较 `slope_L10_e05` 与原始趋势。")

    md_path = os.path.join(OUT, "mtf_slope_entry_report.md")
    with open(md_path, "w", encoding="utf-8") as f:
        f.write("\n".join(md))

    csv_path = os.path.join(OUT, "mtf_slope_entry_trades.csv")
    fields = ["symbol", "mode", "year", "side", "entry_time", "exit_time", "entry_idx", "exit_idx",
              "ret_pct", "reason", "mfe_pct", "hold_bars"]
    with open(csv_path, "w", encoding="utf-8", newline="") as f:
        w = csv.DictWriter(f, fieldnames=fields, extrasaction="ignore")
        w.writeheader()
        for t in all_trades:
            w.writerow({k: t.get(k) for k in fields})

    print(f"[written] {md_path}")
    print(f"[written] {csv_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
