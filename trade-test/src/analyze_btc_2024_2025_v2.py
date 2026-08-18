"""BTC 2024/2025 年四种方案对比：A1 / A9 / A10 / A11。

验证：A10/A11 是否解决了 A9 在 2024/2025 年「小波段锁不住」的问题。
输出：feature_report/btc_2024_2025_v2.md
"""
from __future__ import annotations

import os
from collections import defaultdict

import data_config as dc
from loader import load_klines_30m
from study_adaptive_ma_trailing import backtest_trades, precompute, comp
from study_hybrid_trailing import backtest_hybrid
from study_tiered_hybrid import backtest_tiered3


def reason_summary(trades):
    rc = defaultdict(lambda: [0, 0.0])
    for t in trades:
        rc[t["reason"]][0] += 1
        rc[t["reason"]][1] += t["ret_pct"]
    return rc


def main() -> int:
    md = []
    add = md.append
    add("# BTC 2024/2025 年四种方案对比")
    add("")

    coin = "BTCUSDT"
    params = dc.SYMBOL_PARAMS[coin]
    bars = load_klines_30m(coin)
    pre = precompute(bars)

    schemes = [
        ("A1 基线", lambda y0, y1: backtest_trades(coin, params, bars, pre, mode="base", y0=y0, y1=y1)),
        ("A9 分级衰竭", lambda y0, y1: backtest_trades(coin, params, bars, pre, mode="tiered_demote",
                                                        confirm=10, switch_at=20.0, demote_pct=10.0, activate=15.0, y0=y0, y1=y1)),
        ("A10 两段混合", lambda y0, y1: backtest_hybrid(coin, params, bars, pre, 8.0, 4.0, 1.5, y0=y0, y1=y1)),
        ("A11 三段分级", lambda y0, y1: backtest_tiered3(coin, params, bars, pre, 6.0, 12.0, y0=y0, y1=y1)),
    ]

    for y0, y1 in [(2024, 2024), (2025, 2025)]:
        add(f"## {y0} 年")
        add("")
        add("| 方案 | 笔数 | 简单收益 | 复利收益 | 移动止盈笔数 | 移动止盈收益 | MA止盈笔数 |")
        add("|---|---|---|---|---|---|---|")
        for name, fn in schemes:
            trades = fn(y0, y1)
            rc = reason_summary(trades)
            tp_cnt = rc.get("移动止盈", [0, 0.0])[0]
            tp_ret = rc.get("移动止盈", [0, 0.0])[1]
            ma_cnt = sum(v[0] for k, v in rc.items() if "止盈" in k and k != "移动止盈")
            add(f"| {name} | {len(trades)} | {sum(t['ret_pct'] for t in trades):+.2f}% | "
                f"{comp([t['ret_pct'] for t in trades]):+.2f}% | {tp_cnt} | {tp_ret:+.2f}% | {ma_cnt} |")
        add("")
        add("> 「移动止盈笔数」是判断小波段是否被锁住的关键：A9 无移动止盈（activate15%太高），")
        add("> A10/A11 恢复了移动止盈，应能锁住 4~15% 的小波段。")
        add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "btc_2024_2025_v2.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
