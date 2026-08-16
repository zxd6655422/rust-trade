"""入场信号「位置结构」分类：顶部区间 / 底部区间 / 上涨中继 / 下跌中继。

用 position_in_range_96（当前价在 96 根 bar 高-低区间中的百分位，0=区间底，100=区间顶）
结合方向，把入场信号分成四类，统计各类盈亏，看能否作为额外过滤维度。

四分类（约定）：
  多头(LONG) + 位置<50  → 上涨中继（回调到区间下半，低吸）
  多头(LONG) + 位置>=50 → 顶部区间（追到区间上半，追高）
  空头(SHORT) + 位置>=50 → 下跌中继（反弹到区间上半，高抛）
  空头(SHORT) + 位置<50  → 底部区间（追到区间下半，追低）

输出：feature_report/structure_report.md
"""
from __future__ import annotations

import json
import os
from typing import List, Dict, Any

SRC = os.path.dirname(os.path.abspath(__file__))
JSON = os.path.join(SRC, "feature_report", "trade_features.json")
OUT = os.path.join(SRC, "feature_report", "structure_report.md")


def main() -> int:
    trades = json.load(open(JSON, encoding="utf-8"))
    md: List[str] = []
    add = md.append
    add("# 入场信号「位置结构」分类：顶部/底部/中继")
    add("")
    add("- position_in_range_96 = 当前价在近 96 根 bar 最高-最低区间中的百分位（0=底，100=顶）。")
    add("")

    for coin in ["BTCUSDT", "ETHUSDT"]:
        ts = [t for t in trades if t["symbol"] == coin]
        add(f"## {coin}  （{len(ts)} 笔）")
        add("")

        # 按 side × position 分档统计
        add("### 1. 分方向 × 区间位置 四档")
        add("")
        add("| 方向 | 区间位置 | 笔数 | 胜率 | 总收益 | 平均收益 | 结构标签 |")
        add("|---|---|---|---|---|---|---|")
        buckets = [(0, 25, "0~25%"), (25, 50, "25~50%"), (50, 75, "50~75%"), (75, 100, "75~100%")]
        for side, side_label in [("多", "多头LONG"), ("空", "空头SHORT")]:
            for lo, hi, lab in buckets:
                b = [t for t in ts if t["side"] == side and t["entry"].get("position_in_range_96") is not None
                     and lo <= t["entry"]["position_in_range_96"] < hi]
                if not b:
                    continue
                n = len(b)
                w = sum(1 for t in b if t["ret_pct"] > 0)
                tot = sum(t["ret_pct"] for t in b)
                # 结构标签
                if side == "多":
                    tag = "上涨中继(低吸)" if hi <= 50 else "顶部区间(追高)"
                else:
                    tag = "下跌中继(高抛)" if lo >= 50 else "底部区间(追低)"
                add(f"| {side_label} | {lab} | {n} | {w/n*100:.1f}% | {tot:+.2f}% | {tot/n:+.2f}% | {tag} |")
        add("")

        # 四分类汇总
        add("### 2. 四分类汇总")
        add("")
        add("| 结构 | 笔数 | 占比 | 胜率 | 总收益 | 平均收益 |")
        add("|---|---|---|---|---|---|")
        cats = [
            ("上涨中继(多·低位<50%)", lambda t: t["side"] == "多" and t["entry"].get("position_in_range_96") is not None and t["entry"]["position_in_range_96"] < 50),
            ("顶部区间(多·高位≥50%)", lambda t: t["side"] == "多" and t["entry"].get("position_in_range_96") is not None and t["entry"]["position_in_range_96"] >= 50),
            ("下跌中继(空·高位≥50%)", lambda t: t["side"] == "空" and t["entry"].get("position_in_range_96") is not None and t["entry"]["position_in_range_96"] >= 50),
            ("底部区间(空·低位<50%)", lambda t: t["side"] == "空" and t["entry"].get("position_in_range_96") is not None and t["entry"]["position_in_range_96"] < 50),
        ]
        for name, fn in cats:
            b = [t for t in ts if fn(t)]
            if not b:
                continue
            n = len(b)
            w = sum(1 for t in b if t["ret_pct"] > 0)
            tot = sum(t["ret_pct"] for t in b)
            add(f"| {name} | {n} | {n/len(ts)*100:.0f}% | {w/n*100:.1f}% | {tot:+.2f}% | {tot/n:+.2f}% |")
        add("")

        # 位置与盈亏的单调性（是否越低位越赚）
        add("### 3. 位置越极端是否越差（多头的追高 vs 低吸）")
        add("")
        add("| 多头位置分位 | 笔数 | 胜率 | 平均收益 |")
        add("|---|---|---|---|")
        longs = [t for t in ts if t["side"] == "多" and t["entry"].get("position_in_range_96") is not None]
        for q, lab in [(0.2, "P20(最低)"), (0.4, "P40"), (0.6, "P60"), (0.8, "P80"), (1.0, "P100(最高)")]:
            if not longs:
                continue
            vals = sorted(t["entry"]["position_in_range_96"] for t in longs)
            thr = vals[min(len(vals) - 1, int(q * len(vals)))]
            # 用区间
            add(f"| {lab} | — | — | — |")
        add("")
        add("> （本表仅示意，详细分位见第 1 节四档。）")
        add("")

    with open(OUT, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {OUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
