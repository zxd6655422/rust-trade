"""止盈方案的时间切分验证 + 回撤量化。

背景：exit_variants 里出现 SOL MA288 +5835% 的极端数字，明显是样本内过拟合。
本脚本用与 A7 相同的时间切分（前半训练 → 后半验证），判断：
  1. 哪些止盈方案在「前后两半」都跑赢 MA192（= 真信号，非过拟合）；
  2. 慢均线（MA288/MA480）长拿的代价——平均回撤（give-back）有多大。

输出：feature_report/exit_validate_report.md
"""
from __future__ import annotations

import os
from datetime import datetime
from typing import List

import data_config as dc
from loader import load_klines_30m
from study_exit_variants import backtest_trades, precompute, comp, mean

RULES = [
    ("MA192 c10(A7)", "ma192", {"confirm": 10}),
    ("MA192 c20", "ma192", {"confirm": 20}),
    ("MA288 c3", "ma288", {"confirm": 3}),
    ("MA480 c1", "ma480", {"confirm": 1}),
    ("MA480 c3", "ma480", {"confirm": 3}),
    ("MA480 c10", "ma480", {"confirm": 10}),
    ("分级≥20%转MA480", "tiered", {"confirm": 10, "switch_at": 20.0}),
    ("分级≥30%转MA480", "tiered", {"confirm": 10, "switch_at": 30.0}),
]


def year_range(bars):
    ys = [datetime.fromtimestamp(b.open_time / 1000).year for b in bars]
    return min(ys), max(ys)


def main() -> int:
    md: List[str] = []
    add = md.append
    add("# 止盈方案：时间切分验证 + 回撤量化")
    add("")
    add("> 方法：年份中位数切分，看每个方案在前半段 / 后半段分别的复利，是否都跑赢 MA192。")
    add("")

    coins = ["BTCUSDT", "ETHUSDT", "SOLUSDT"]

    # ============ Part 1: 时间切分 ============
    add("## Part 1. 时间切分（前半 / 后半 复利）")
    add("")
    for coin in coins:
        params = dc.SYMBOL_PARAMS[coin]
        bars = load_klines_30m(coin)
        pre = precompute(bars)
        y0, y1 = year_range(bars)
        mid = (y0 + y1) // 2
        add(f"### {coin}（{y0}-{y1}，切分点 {mid}）")
        add("")
        add("| 方案 | 全样本 | 前半 | 后半 | 两半都跑赢MA192? |")
        add("|---|---|---|---|---|")
        base_full = None
        rows = []
        for label, mode, cfg in RULES:
            f = comp([t["ret_pct"] for t in backtest_trades(coin, params, bars, pre, mode=mode, **cfg)])
            h1 = comp([t["ret_pct"] for t in backtest_trades(coin, params, bars, pre, mode=mode, y0=y0, y1=mid, **cfg)])
            h2 = comp([t["ret_pct"] for t in backtest_trades(coin, params, bars, pre, mode=mode, y0=mid + 1, y1=y1, **cfg)])
            if label.startswith("MA192 c10"):
                base_full, base_h1, base_h2 = f, h1, h2
            rows.append((label, f, h1, h2))
        for label, f, h1, h2 in rows:
            if label.startswith("MA192 c10"):
                verdict = "—"
            else:
                verdict = "✅" if (h1 > base_h1 and h2 > base_h2) else ("⚠️部分" if (h1 > base_h1 or h2 > base_h2) else "❌")
            add(f"| {label} | {f:+.1f}% | {h1:+.1f}% | {h2:+.1f}% | {verdict} |")
        add("")
    add("> ✅ = 前后两半都跑赢 MA192（较可信）；⚠️部分 = 只有一半跑赢；❌ = 两半都跑输。")
    add("")

    # ============ Part 2: 回撤量化 ============
    add("## Part 2. 长拿的回撤代价（只统计触发过 activate 的单子）")
    add("")
    add("| 币种 | 方案 | 大单数 | 平均收益 | 平均MFE | 平均回撤(MFE-收益) |")
    add("|---|---|---|---|---|---|")
    for coin in coins:
        params = dc.SYMBOL_PARAMS[coin]
        bars = load_klines_30m(coin)
        pre = precompute(bars)
        for label, mode, cfg in RULES:
            if label in ("MA192 c20", "MA288 c3"):
                continue  # 只看代表性子集
            trades = backtest_trades(coin, params, bars, pre, mode=mode, **cfg)
            big = [t for t in trades if t["mfe_pct"] >= 15.0]
            if not big:
                continue
            add(f"| {coin} | {label} | {len(big)} | {mean([t['ret_pct'] for t in big]):+.1f}% | "
                f"{mean([t['mfe_pct'] for t in big]):+.1f}% | {mean([t['mfe_pct']-t['ret_pct'] for t in big]):+.1f}% |")
        add("")
    add("> 「平均回撤」= 触发过 +15% 的单子，从最高浮盈（MFE）回撤到离场让掉了多少利润（占入场价 %）。")
    add("> 慢均线长拿的复利更高，但代价是单笔回撤更大——这是「拿更大利润」必须接受的成本。")
    add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "exit_validate_report.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
