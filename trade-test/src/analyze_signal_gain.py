"""信号价 → 当前价 涨幅维度：盈利单 vs 亏损单 的区分度统计（BTC/ETH）。

- 立即入场时 entry = 信号触发价，故 mfe_pct = 信号后价格相对信号价的最大涨幅。
- 问题：如果「等价格相对信号价涨到 X% 才入场」（不锁死 K 线根数），
  能保留多少盈利单、过滤多少亏损单？

输出：feature_report/signal_gain_report.md
"""
from __future__ import annotations

import json
import os
from typing import List, Dict, Any

SRC = os.path.dirname(os.path.abspath(__file__))
JSON = os.path.join(SRC, "feature_report", "trade_features.json")
OUT = os.path.join(SRC, "feature_report", "signal_gain_report.md")


def median(xs):
    if not xs:
        return float('nan')
    s = sorted(xs)
    m = len(s) // 2
    return s[m] if len(s) % 2 else (s[m - 1] + s[m]) / 2.0


def quantile(xs, q):
    if not xs:
        return float('nan')
    s = sorted(xs)
    return s[min(len(s) - 1, int(q * len(s)))]


def main() -> int:
    trades = json.load(open(JSON, encoding="utf-8"))
    md: List[str] = []
    add = md.append
    add("# 信号价 → 当前价 涨幅维度：盈利单 vs 亏损单 区分度")
    add("")
    add("- 立即入场 entry = 信号价，故 mfe_pct = 信号后价格相对信号价的最大涨幅。")
    add("- 盈利单 = 移动止盈；亏损单 = MA288 止损。")
    add("")

    for coin in ["BTCUSDT", "ETHUSDT"]:
        ts = [t for t in trades if t["symbol"] == coin]
        wins = [t for t in ts if t["reason"] == "移动止盈"]
        losses = [t for t in ts if t["reason"] == "MA288止损"]
        add(f"## {coin}  （盈利 {len(wins)} / 止损 {len(losses)}）")
        add("")

        # 1. MFE 分布
        add("### 1. 信号后最大涨幅（MFE）分布")
        add("")
        add("| 分位 | 盈利单 MFE | 止损单 MFE |")
        add("|---|---|---|")
        for q, lab in [(0.1, "P10"), (0.25, "P25"), (0.5, "中位"), (0.75, "P75"), (0.9, "P90")]:
            add(f"| {lab} | {quantile([t['mfe_pct'] for t in wins], q):+.2f}% | "
                f"{quantile([t['mfe_pct'] for t in losses], q):+.2f}% |")
        add("")

        # 2. 关键：等价格涨到 X% 才入场，保留/过滤
        add("### 2. 「等价格涨到 X% 才入场」的保留率 vs 过滤率")
        add("")
        add("| 涨幅阈值 X | 盈利单保留(MFE≥X) | 盈利单保留占比 | 止损单过滤(MFE<X) | 止损单过滤占比 |")
        add("|---|---|---|---|---|")
        for x in (0.0, 0.3, 0.5, 0.8, 1.0, 1.5, 2.0, 3.0):
            w_keep = sum(1 for t in wins if t["mfe_pct"] >= x)
            l_cut = sum(1 for t in losses if t["mfe_pct"] < x)
            add(f"| {x:.1f}% | {w_keep} | {w_keep/len(wins)*100:.0f}% | {l_cut} | {l_cut/len(losses)*100:.0f}% |")
        add("")

        # 3. 区间命中统计：亏损单的 MFE 主要集中在哪
        add("### 3. 亏损单 MFE 分布（能否用「涨幅不足」过滤）")
        add("")
        add("| 亏损单 MFE 区间 | 笔数 | 占比 |")
        add("|---|---|---|")
        buckets = [(-1e9, 0.5, "<0.5%"), (0.5, 1.0, "0.5~1%"), (1.0, 1.5, "1~1.5%"),
                   (1.5, 2.0, "1.5~2%"), (2.0, 3.0, "2~3%"), (3.0, 5.0, "3~5%"), (5.0, 1e9, "≥5%")]
        for lo, hi, lab in buckets:
            b = [t for t in losses if lo <= t["mfe_pct"] < hi]
            add(f"| {lab} | {len(b)} | {len(b)/len(losses)*100:.0f}% |")
        add("")

    with open(OUT, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {OUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
