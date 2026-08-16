"""样本外(OOS)验证：过滤阈值的稳健性检验。

背景：feature_analysis_report.md 里的最优阈值（如 realized_vol_48 >= 0.522 跳过）
是在 2020-2026 全样本上选出来的（样本内），可能过拟合。

本脚本用 4 种方式检验阈值是否稳健：
  1) 逐年最优阈值稳定性：每年单独选最优阈值，看是否跨年稳定。
  2) 时间切分 OOS：前半段选阈值 → 后半段验证（真正的样本外），并反向验证。
  3) 滚动 walk-forward：过去 12 个月选阈值 → 只应用到下一个月，滚动汇总样本外表现。
  4) 跨币种：BTC 上选阈值 → ETH/SOL 验证（同时间段，仅作补充参考）。

数据：feature_report/trade_features.json
输出：feature_report/oos_validation_report.md
"""
from __future__ import annotations

import json
import os
from typing import List, Dict, Any, Tuple, Optional
from collections import defaultdict

SRC_DIR = os.path.dirname(os.path.abspath(__file__))
OUT_DIR = os.path.join(SRC_DIR, "feature_report")
JSON_PATH = os.path.join(OUT_DIR, "trade_features.json")
MD_PATH = os.path.join(OUT_DIR, "oos_validation_report.md")

# 待检验的指标（按 feature_analysis_report 里表现最好的排序）
FEATS = ["realized_vol_48", "bbw_100", "donchian_width_96", "mean_spread_288",
         "atr_pct_14", "efficiency_ratio_96", "cross_count_96"]
MAIN_FEAT = "realized_vol_48"

MIN_KEEP_RATIO = 0.30  # 选阈值时至少保留 30% 交易，避免退化成"几乎不交易"


def load() -> List[Dict[str, Any]]:
    with open(JSON_PATH, "r", encoding="utf-8") as f:
        trades = json.load(f)
    trades.sort(key=lambda t: t["entry_time"])
    return trades


def _kept(trades, feat, thr, direction="high"):
    """direction=high: 跳过 feature>=thr（保留 feature<thr）。"""
    kept = []
    removed = []
    for t in trades:
        v = t["entry"].get(feat)
        if v is None:
            kept.append(t)
            continue
        skip = (direction == "high" and v >= thr) or (direction == "low" and v <= thr)
        (removed if skip else kept).append(t)
    return kept, removed


def _total(ts):
    return sum(t["ret_pct"] for t in ts)


def _win_rate(ts):
    return (sum(1 for t in ts if t["ret_pct"] > 0) / len(ts) * 100.0) if ts else 0.0


def pick_threshold(trades, feat, direction="high", min_keep_ratio=MIN_KEEP_RATIO):
    """在给定交易集上扫描阈值，返回 (最优阈值, 保留总收益, 保留笔数, 基线总收益, 基线笔数)。"""
    vals = sorted(t["entry"][feat] for t in trades if t["entry"].get(feat) is not None)
    if len(vals) < 50:
        return None
    baseline = _total(trades)
    best = None
    for q in range(5, 96, 5):
        thr = vals[min(len(vals) - 1, int(q / 100.0 * len(vals)))]
        kept, _ = _kept(trades, feat, thr, direction)
        if len(kept) < len(trades) * min_keep_ratio:
            continue
        kret = _total(kept)
        if best is None or kret > best["kret"]:
            best = {"thr": thr, "kret": kret, "kn": len(kept)}
    if best is None:
        return None
    return (best["thr"], best["kret"], best["kn"], baseline, len(trades))


def fmt_ret(x):
    return f"{x:+.2f}%"


def main() -> int:
    trades = load()
    md: List[str] = []
    add = md.append
    add("# 样本外(OOS)验证报告")
    add("")
    add(f"- 数据：`feature_report/trade_features.json`（{len(trades)} 笔，含每笔入场指标快照与收益）")
    add(f"- 选阈值约束：至少保留 {int(MIN_KEEP_RATIO * 100)}% 交易，避免退化成「几乎不交易」")
    add("- 说明：第 2、3 节是**真正的样本外**（选阈值时没见过验证段的数据）；第 4 节是跨币种参考。")
    add("")

    # ================= 1. 逐年最优阈值稳定性 =================
    add("## 1. 逐年最优阈值稳定性（同一指标每年单独选阈值）")
    add("")
    add("若阈值跨年稳定在相近数值，说明是真实结构；若每年漂移很大，说明是样本内拟合。")
    add("")
    by_year: Dict[str, List[Dict[str, Any]]] = defaultdict(list)
    for t in trades:
        by_year[t["entry_year"]].append(t)
    years = sorted(by_year)

    for feat in FEATS:
        add(f"### {feat}（direction=high：跳过 >= 阈值）")
        add("")
        add("| 年份 | 笔数 | 该年基线收益 | 该年最优阈值 | 过滤后该年收益 | 保留笔数 |")
        add("|---|---|---|---|---|---|")
        for y in years:
            r = pick_threshold(by_year[y], feat, "high")
            if r is None:
                continue
            thr, kret, kn, base, n = r
            add(f"| {y} | {n} | {fmt_ret(base)} | {thr:.3f} | {fmt_ret(kret)} | {kn} |")
        add("")

    # ================= 2. 时间切分 OOS =================
    add("## 2. 时间切分样本外（前段选阈值 → 后段验证）")
    add("")
    add("训练段用来选阈值，测试段从未参与选阈值。关键看「测试段」过滤后是否仍优于基线。")
    add("")

    def split_by_year(lo, hi):
        return [t for t in trades if lo <= int(t["entry_year"]) <= hi]

    train_a = split_by_year(2020, 2022)
    test_a = split_by_year(2023, 2026)
    train_b = split_by_year(2023, 2026)
    test_b = split_by_year(2020, 2022)

    add("### 切法 A：训练 2020-2022 → 测试 2023-2026")
    add("")
    add("| 指标 | 训练段最优阈值 | 训练段基线→过滤后 | 测试段基线→过滤后 | 测试段保留笔数 |")
    add("|---|---|---|---|---|")
    for feat in FEATS:
        r = pick_threshold(train_a, feat, "high")
        if r is None:
            continue
        thr, _, _, _, _ = r
        kept_test, rem_test = _kept(test_a, feat, thr, "high")
        add(f"| {feat} | {thr:.3f} | {fmt_ret(_total(train_a))}→{fmt_ret(r[1])} | "
            f"{fmt_ret(_total(test_a))}→{fmt_ret(_total(kept_test))} | {len(kept_test)}/{len(test_a)} |")
    add("")

    add("### 切法 B：训练 2023-2026 → 测试 2020-2022")
    add("")
    add("| 指标 | 训练段最优阈值 | 训练段基线→过滤后 | 测试段基线→过滤后 | 测试段保留笔数 |")
    add("|---|---|---|---|---|")
    for feat in FEATS:
        r = pick_threshold(train_b, feat, "high")
        if r is None:
            continue
        thr, _, _, _, _ = r
        kept_test, rem_test = _kept(test_b, feat, thr, "high")
        add(f"| {feat} | {thr:.3f} | {fmt_ret(_total(train_b))}→{fmt_ret(r[1])} | "
            f"{fmt_ret(_total(test_b))}→{fmt_ret(_total(kept_test))} | {len(kept_test)}/{len(test_b)} |")
    add("")

    # ================= 3. 滚动 walk-forward =================
    add("## 3. 滚动 walk-forward（过去 12 个月选阈值 → 下一个月）")
    add("")
    add("每个月只用它之前 12 个月的交易选阈值，再应用到当月。汇总所有月份的样本外表现。")
    add("")
    months = sorted({t["entry_time"][:7] for t in trades})
    add(f"共 {len(months)} 个月。")
    add("")
    add("| 指标 | 样本外月份数 | 这些月基线总收益 | 过滤后总收益 | 保留笔数/总笔数 |")
    add("|---|---|---|---|---|")
    for feat in FEATS:
        oos_base = 0.0
        oos_keep = 0.0
        oos_kn = 0
        oos_n = 0
        for mi in range(12, len(months)):
            m = months[mi]
            # 取该月之前 12 个月窗口内的交易
            win_start = months[mi - 12]
            window = [t for t in trades if win_start <= t["entry_time"][:7] < m]
            r = pick_threshold(window, feat, "high")
            if r is None:
                continue
            thr = r[0]
            month_trades = [t for t in trades if t["entry_time"][:7] == m]
            kept, _ = _kept(month_trades, feat, thr, "high")
            oos_base += _total(month_trades)
            oos_keep += _total(kept)
            oos_kn += len(kept)
            oos_n += len(month_trades)
        if oos_n == 0:
            continue
        add(f"| {feat} | {len(months) - 12} | {fmt_ret(oos_base)} | {fmt_ret(oos_keep)} | {oos_kn}/{oos_n} |")
    add("")

    # ================= 4. 跨币种 =================
    add("## 4. 跨币种参考（BTC 选阈值 → ETH/SOL 验证）")
    add("")
    add("注意：三币种同时间段且高度相关，此节仅供参考，不是严格独立样本外。")
    add("")
    btc = [t for t in trades if t["symbol"] == "BTCUSDT"]
    eth = [t for t in trades if t["symbol"] == "ETHUSDT"]
    sol = [t for t in trades if t["symbol"] == "SOLUSDT"]
    add("| 指标 | BTC选阈值 | BTC基线→过滤后 | ETH基线→过滤后 | SOL基线→过滤后 |")
    add("|---|---|---|---|---|")
    for feat in FEATS:
        r = pick_threshold(btc, feat, "high")
        if r is None:
            continue
        thr = r[0]
        eth_kept, _ = _kept(eth, feat, thr, "high")
        sol_kept, _ = _kept(sol, feat, thr, "high")
        add(f"| {feat} | {thr:.3f} | {fmt_ret(_total(btc))}→{fmt_ret(r[1])} | "
            f"{fmt_ret(_total(eth))}→{fmt_ret(_total(eth_kept))} | "
            f"{fmt_ret(_total(sol))}→{fmt_ret(_total(sol_kept))} |")
    add("")

    with open(MD_PATH, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {MD_PATH}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
