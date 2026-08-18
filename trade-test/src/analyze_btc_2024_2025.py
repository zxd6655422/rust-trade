"""BTC 2024/2025 年 A9 为什么跑输 A1 —— 逐笔对比分析。

聚焦：BTC 2024、2025 两年，A1 基线 vs A9 分级+衰竭降级 的逐笔交易差异，
回答：A9 的盈利单为什么没放大、亏损单如何亏的。

输出：feature_report/btc_2024_2025_analysis.md
"""
from __future__ import annotations

import os
from datetime import datetime, timezone, timedelta
from collections import defaultdict

import data_config as dc
from loader import load_klines_30m
from study_adaptive_ma_trailing import backtest_trades, precompute, comp

BJ = timezone(timedelta(hours=8))


def fmt_dt(ms):
    return datetime.fromtimestamp(ms / 1000, tz=BJ).strftime("%Y-%m-%d %H:%M")


def main() -> int:
    md = []
    add = md.append
    add("# BTC 2024/2025 年 A9 为什么跑输 A1")
    add("")

    coin = "BTCUSDT"
    params = dc.SYMBOL_PARAMS[coin]
    bars = load_klines_30m(coin)
    pre = precompute(bars)

    for y0, y1 in [(2024, 2024), (2025, 2025)]:
        a1 = backtest_trades(coin, params, bars, pre, mode="base", y0=y0, y1=y1)
        a9 = backtest_trades(coin, params, bars, pre, mode="tiered_demote", confirm=10,
                             switch_at=20.0, demote_pct=10.0, activate=15.0, y0=y0, y1=y1)
        add(f"## {y0} 年")
        add("")
        add(f"- A1 基线：{len(a1)} 笔，简单 {sum(t['ret_pct'] for t in a1):+.2f}%，复利 {comp([t['ret_pct'] for t in a1]):+.2f}%")
        add(f"- A9 分级衰竭：{len(a9)} 笔，简单 {sum(t['ret_pct'] for t in a9):+.2f}%，复利 {comp([t['ret_pct'] for t in a9]):+.2f}%")
        add("")

        # 离场原因汇总
        add("### 离场原因汇总")
        add("")
        add("| 方案 | 离场原因 | 笔数 | 总收益 | 平均收益 |")
        add("|---|---|---|---|---|")
        for label, trades in [("A1", a1), ("A9", a9)]:
            rc = defaultdict(lambda: [0, 0.0])
            for t in trades:
                rc[t["reason"]][0] += 1
                rc[t["reason"]][1] += t["ret_pct"]
            for r, (cnt, tot) in sorted(rc.items(), key=lambda kv: -kv[1][0]):
                add(f"| {label} | {r} | {cnt} | {tot:+.2f}% | {tot/cnt:+.2f}% |")
        add("")

        # 大单（MFE>=20%）
        big_a9 = [t for t in a9 if t["mfe_pct"] >= 20.0]
        big_a1 = [t for t in a1 if t["mfe_pct"] >= 20.0]
        add(f"### 大单（MFE≥20%）")
        add("")
        add(f"- A1 大单 {len(big_a1)} 笔；A9 大单 {len(big_a9)} 笔")
        for t in big_a9:
            add(f"- A9 大单：{fmt_dt(bars[t['entry_idx']].open_time)} 入场，{t['reason']} 离场，收益 {t['ret_pct']:+.2f}%，MFE {t['mfe_pct']:+.2f}%")
        add("")

        # 逐笔交易表（A9）
        add("### A9 逐笔交易（按入场时间）")
        add("")
        add("| 入场时间 | 方向 | 离场原因 | 收益 | MFE | 持仓bar |")
        add("|---|---|---|---|---|---|")
        a9_sorted = sorted(a9, key=lambda t: t["entry_idx"])
        for t in a9_sorted:
            add(f"| {fmt_dt(bars[t['entry_idx']].open_time)} | {t['side']} | {t['reason']} | {t['ret_pct']:+.2f}% | {t['mfe_pct']:+.2f}% | {t['hold_bars']} |")
        add("")

        # 关键差异：A1 的移动止盈单 vs A9 对应
        add("### A1 移动止盈单（A9 缺失的锁定利润）")
        add("")
        a1_tp = [t for t in a1 if t["reason"] == "移动止盈"]
        add(f"- A1 移动止盈 {len(a1_tp)} 笔，总收益 {sum(t['ret_pct'] for t in a1_tp):+.2f}%")
        add(f"- A9 的 MA192/MA480 止盈：{len([t for t in a9 if '止盈' in t['reason']])} 笔，总收益 {sum(t['ret_pct'] for t in a9 if '止盈' in t['reason']):+.2f}%")
        add("")
        add("> A9 的 activate=15%，即盈利 <15% 时 MA 止盈线不生效，只靠 MA288 止损兜底；")
        add("> 而 A1 的移动止盈在盈利 ≥4% 回撤 ≥1% 就锁定。小波段行情里 A1 锁得多、A9 锁不住。")
        add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "btc_2024_2025_analysis.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
