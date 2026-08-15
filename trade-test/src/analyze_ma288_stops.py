"""针对 BTC/ETH 的 MA288 止损交易深挖：哪些止损可避免。

回答：
  1) MA288 止损里，有多少是「入场就没起来」(MFE 小，应避免入场)，
     有多少是「涨过又回撤」(MFE 有幅度，应提前出场)？
  2) 这两类止损的入场特征与盈利单有什么不同？
  3) 量化「入场过滤」与「提前止盈」两个方向能救回多少。

数据：feature_report/trade_features.json（对齐生产：slow=480 + vol过滤 + 反转ON）。
输出：feature_report/ma288_stop_analysis.md
"""
from __future__ import annotations

import json
import os
from collections import defaultdict
from typing import List, Dict, Any

SRC = os.path.dirname(os.path.abspath(__file__))
JSON = os.path.join(SRC, "feature_report", "trade_features.json")
OUT = os.path.join(SRC, "feature_report", "ma288_stop_analysis.md")

COINS = ["BTCUSDT", "ETHUSDT"]

FEATS = [
    "cross_count_96", "interweave_bars_96", "mean_spread_96", "efficiency_ratio_96",
    "donchian_width_96", "position_in_range_96", "close_to_ma288_pct",
    "ma288_slope_5", "adx14", "atr_pct_14", "realized_vol_48",
]


def mean(xs):
    return sum(xs) / len(xs) if xs else float('nan')


def median(xs):
    if not xs:
        return float('nan')
    s = sorted(xs)
    m = len(s) // 2
    return s[m] if len(s) % 2 else (s[m - 1] + s[m]) / 2.0


def pct_of(n, total):
    return f"{n/total*100:.1f}%" if total else "—"


def main() -> int:
    trades = json.load(open(JSON, encoding="utf-8"))
    md: List[str] = []
    add = md.append

    add("# BTC/ETH MA288 止损深挖：哪些止损可避免")
    add("")
    add("- 口径：对齐生产（slow=480 + realized_vol_48 过滤 + 退出链全开）。")
    add("- MA288 止损 = 价格反向穿越 MA288 平仓；移动止盈 = 盈利激活后回撤止盈。")
    add("")

    for coin in COINS:
        ts = [t for t in trades if t["symbol"] == coin]
        stops = [t for t in ts if t["reason"] == "MA288止损"]
        tp = [t for t in ts if t["reason"] == "移动止盈"]
        if not stops:
            continue

        add(f"## {coin}")
        add("")
        add(f"- 总交易 {len(ts)} 笔；MA288止损 {len(stops)} 笔（{pct_of(len(stops), len(ts))}）；"
            f"移动止盈 {len(tp)} 笔（{pct_of(len(tp), len(ts))}）。")
        add(f"- MA288止损 总收益 {sum(t['ret_pct'] for t in stops):+.2f}%；移动止盈 总收益 {sum(t['ret_pct'] for t in tp):+.2f}%。")
        add("")

        # ---- 1. MFE 分布：入场就没起来 vs 涨过又回撤 ----
        add("### 1. MA288 止损按 MFE（最大浮盈）分桶")
        add("")
        add("| MFE 区间 | 笔数 | 占比 | 平均收益 | 平均持仓bar | 解读 |")
        add("|---|---|---|---|---|---|")
        buckets = [(-1e9, 0.5, "<0.5%"), (0.5, 1.0, "0.5~1%"), (1.0, 2.0, "1~2%"),
                   (2.0, 3.0, "2~3%"), (3.0, 5.0, "3~5%"), (5.0, 1e9, "≥5%")]
        for lo, hi, lab in buckets:
            b = [t for t in stops if lo <= t["mfe_pct"] < hi]
            if not b:
                continue
            add(f"| {lab} | {len(b)} | {pct_of(len(b), len(stops))} | "
                f"{mean([t['ret_pct'] for t in b]):+.2f}% | {mean([t['bars'] for t in b]):.1f} | "
                f"{'入场就没起来→避免入场' if hi <= 1.0 else '涨过又回撤→提前出场可救'} |")
        add("")

        # ---- 2. 入场特征对比 ----
        add("### 2. 入场特征对比（均值）")
        add("")
        add("| 指标 | MA288止损(MFE<1%) | MA288止损(MFE≥2%) | 移动止盈(盈利) |")
        add("|---|---|---|---|")
        g_noise = [t for t in stops if t["mfe_pct"] < 1.0]
        g_rev = [t for t in stops if t["mfe_pct"] >= 2.0]
        for f in FEATS:
            a = mean([t["entry"][f] for t in g_noise if t["entry"].get(f) is not None])
            b = mean([t["entry"][f] for t in g_rev if t["entry"].get(f) is not None])
            c = mean([t["entry"][f] for t in tp if t["entry"].get(f) is not None])
            add(f"| {f} | {a:.3f} | {b:.3f} | {c:.3f} |")
        add("")

        # ---- 3. 量化：入场过滤（震荡类指标高则跳过） ----
        add("### 3. 入场过滤扫描（对 MA288 止损影响最大、误伤盈利最小的规则）")
        add("")
        add("| 指标 | 方向 | 阈值 | 移除MA288止损 | 移除移动止盈 | 移除止损总收益 | 移除止盈总收益 |")
        add("|---|---|---|---|---|---|---|")
        rows = []
        for f in FEATS:
            vals = sorted(t["entry"][f] for t in ts if t["entry"].get(f) is not None)
            if len(vals) < 100:
                continue
            for q in (0.5, 0.6, 0.7, 0.8, 0.9):
                thr = vals[min(len(vals) - 1, int(q * len(vals)))]
                for dir_ in ("high", "low"):
                    rem_stop = [t for t in stops if t["entry"].get(f) is not None and
                                ((dir_ == "high" and t["entry"][f] >= thr) or (dir_ == "low" and t["entry"][f] <= thr))]
                    rem_tp = [t for t in tp if t["entry"].get(f) is not None and
                              ((dir_ == "high" and t["entry"][f] >= thr) or (dir_ == "low" and t["entry"][f] <= thr))]
                    if len(rem_stop) < 30:
                        continue
                    rows.append((f, dir_, thr, len(rem_stop), len(rem_tp),
                                 sum(t['ret_pct'] for t in rem_stop), sum(t['ret_pct'] for t in rem_tp)))
        # 排序：移除止损总收益最负（移除越多亏损）+ 移除止盈尽量少
        rows.sort(key=lambda r: (r[5], r[4]))  # rem_stop_ret 越负越好，其次 rem_tp 越少越好
        for f, dir_, thr, n_stop, n_tp, r_stop, r_tp in rows[:12]:
            add(f"| {f} | {dir_} | {thr:.3f} | {n_stop} | {n_tp} | {r_stop:+.2f}% | {r_tp:+.2f}% |")
        add("")
        add("> 解读：`移除止损总收益`越负=过滤掉的亏损越多；`移除止盈`越少=误伤越少。理想规则是前负后小。")
        add("")

        # ---- 4. 量化：提前止盈（MFE 达标即平，而非等 MA288 止损/移动止盈） ----
        add("### 4. 提前止盈估算（MA288 止损单若 MFE 达标即平）")
        add("")
        add("对 MA288 止损单，若在 MFE 达到阈值时提前止盈，收益从实际止损收益变为止盈收益（上界估计，忽略先到更高点再回撤）。")
        add("")
        add("| 提前止盈阈值 | 能救回的止损单数 | 原止损总收益 | 改为止盈后收益 | 净改善 |")
        add("|---|---|---|---|---|")
        for tp_thr in (1.0, 1.5, 2.0, 2.5, 3.0):
            saved = [t for t in stops if t["mfe_pct"] >= tp_thr]
            if not saved:
                continue
            orig = sum(t["ret_pct"] for t in saved)
            new = sum(tp_thr for t in saved)  # 上界：全部在阈值止盈
            add(f"| {tp_thr:.1f}% | {len(saved)} | {orig:+.2f}% | {new:+.2f}% | {new - orig:+.2f}% |")
        add("")
        add("> 注意：这是「上界」——假设达到阈值当根就能以阈值价止盈；真实会打折，需在逐 bar 回测里精确验证。")
        add("")

    with open(OUT, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {OUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
