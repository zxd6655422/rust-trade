"""多时间框架持有 时间切分验证 + 回撤分析。

1. 时间切分：前半训练选 4h 周期 → 后半验证（检验 4h MA40 是否稳健，非过拟合）
2. 回撤：全样本 + 分年度最大回撤（MTF 持有 vs 30m A1 基线）

输出：feature_report/mtf_validate_report.md
"""
from __future__ import annotations

import os
from datetime import datetime
from collections import defaultdict

import data_config as dc
from loader import load_klines_30m, load_klines_4h
from study_adaptive_ma_trailing import backtest_trades, precompute, comp
from study_mtf_hold import backtest_mtf_hold


def max_drawdown(rets):
    eq = 1.0
    peak = 1.0
    dd = 0.0
    for r in rets:
        eq *= (1.0 + r / 100.0)
        peak = max(peak, eq)
        dd = max(dd, (peak - eq) / peak)
    return dd * 100.0


def yearly_dd(trades, bars30):
    """按入场年分组，算各年最大回撤。"""
    by_year = defaultdict(list)
    for t in trades:
        y = datetime.fromtimestamp(bars30[t["entry_idx"]].open_time / 1000).year
        by_year[y].append(t["ret_pct"])
    out = {}
    for y, rets in by_year.items():
        out[y] = max_drawdown(rets)
    return out


def main() -> int:
    md = []
    add = md.append
    add("# 多时间框架持有：时间切分 + 回撤分析")
    add("")

    for coin in ["BTCUSDT", "ETHUSDT", "SOLUSDT"]:
        params = dc.SYMBOL_PARAMS[coin]
        bars30 = load_klines_30m(coin)
        bars4 = load_klines_4h(coin)
        years = [datetime.fromtimestamp(b.open_time / 1000).year for b in bars30]
        y0, y1 = min(years), max(years)
        mid = (y0 + y1) // 2

        add(f"## {coin}（数据 {y0}-{y1}，切分点 {mid}）")
        add("")

        # 1. 时间切分：前半训练选 4h 周期
        add("### 1. 时间切分（前半训练选 4h 周期 → 后半验证）")
        add("")
        add("| 4h周期 | 训练段复利 | 验证段复利 |")
        add("|---|---|---|")
        best = None
        for ma4_p in [20, 40, 60, 90]:
            train = comp([t["ret_pct"] for t in backtest_mtf_hold(coin, params, bars30, bars4, ma4_p, 4.0, 1.0, y0, mid)])
            val = comp([t["ret_pct"] for t in backtest_mtf_hold(coin, params, bars30, bars4, ma4_p, 4.0, 1.0, mid+1, y1)])
            add(f"| MA{ma4_p} | {train:+.1f}% | {val:+.1f}% |")
            if best is None or train > best[1]:
                best = (ma4_p, train, val)
        # 验证段基线
        val_base = comp([t["ret_pct"] for t in backtest_trades(coin, params, bars30, precompute(bars30), mode="base", y0=mid+1, y1=y1)])
        add(f"| **基线(A1)验证段** | — | **{val_base:+.1f}%** |")
        add("")
        add(f"**训练最优 4h MA{best[0]} → 验证段 {best[2]:+.1f}% vs 基线 {val_base:+.1f}%**")
        add("")

        # 2. 全样本回撤（用训练最优周期）
        ma4_best = best[0]
        mtf_trades = backtest_mtf_hold(coin, params, bars30, bars4, ma4_best, 4.0, 1.0)
        a1_trades = backtest_trades(coin, params, bars30, precompute(bars30), mode="base")
        add("### 2. 全样本最大回撤")
        add("")
        add(f"- MTF持有(4h MA{ma4_best})：最大回撤 {max_drawdown([t['ret_pct'] for t in mtf_trades]):.1f}%")
        add(f"- 30m A1 基线：最大回撤 {max_drawdown([t['ret_pct'] for t in a1_trades]):.1f}%")
        add("")

        # 3. 分年度回撤
        add("### 3. 分年度最大回撤")
        add("")
        add("| 年份 | 30m A1 回撤 | MTF持有 回撤 |")
        add("|---|---|---|")
        dd_a1 = yearly_dd(a1_trades, bars30)
        dd_mtf = yearly_dd(mtf_trades, bars30)
        for y in sorted(set(list(dd_a1.keys()) + list(dd_mtf.keys()))):
            a1 = dd_a1.get(y, 0.0)
            mtf = dd_mtf.get(y, 0.0)
            add(f"| {y} | {a1:.1f}% | {mtf:.1f}% |")
        add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "mtf_validate_report.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
