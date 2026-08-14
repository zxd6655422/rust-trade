"""多维度监控指标分析（读取 feature_report/trade_features.json）。

回答四个问题：
  1) 哪些指标影响盈利 / 与盈亏相关？
  2) 盈利交易、大幅盈利交易有什么共同特征？
  3) 亏损交易是否有可识别的"震荡/交织/箱体"特征？
  4) 用什么检测手段可以规避亏损入场，且尽量不伤及盈利交易？

输出：feature_report/feature_analysis_report.md

运行：cd F:/rust-projects/trade-test/src && python analyze_features.py
"""
from __future__ import annotations

import json
import os
from typing import List, Dict, Any, Tuple, Optional
from collections import defaultdict

from indicators import IndicatorSet

SRC_DIR = os.path.dirname(os.path.abspath(__file__))
OUT_DIR = os.path.join(SRC_DIR, "feature_report")
JSON_PATH = os.path.join(OUT_DIR, "trade_features.json")
MD_PATH = os.path.join(OUT_DIR, "feature_analysis_report.md")

FEATURES = IndicatorSet.FEATURE_NAMES
BIG_WIN = 5.0      # 大幅盈利阈值（ret_pct >= 5%）
BIG_LOSS = -1.4    # 大幅亏损阈值（ret_pct <= -1.4%，基本对应硬止损）


# ----------------------------------------------------------------------
# 基础统计工具
# ----------------------------------------------------------------------

def mean(xs: List[float]) -> Optional[float]:
    return sum(xs) / len(xs) if xs else None


def median(xs: List[float]) -> Optional[float]:
    if not xs:
        return None
    s = sorted(xs)
    m = len(s) // 2
    return s[m] if len(s) % 2 else (s[m - 1] + s[m]) / 2.0


def pct(f: Optional[float], nd=2) -> str:
    return "—" if f is None else f"{f:+.{nd}f}"


def pearson(xs: List[float], ys: List[float]) -> Optional[float]:
    n = len(xs)
    if n < 3:
        return None
    mx = sum(xs) / n
    my = sum(ys) / n
    sxy = sum((x - mx) * (y - my) for x, y in zip(xs, ys))
    sxx = sum((x - mx) ** 2 for x in xs)
    syy = sum((y - my) ** 2 for y in ys)
    if sxx == 0.0 or syy == 0.0:
        return None
    return sxy / (sxx ** 0.5 * syy ** 0.5)


def std(xs: List[float]) -> float:
    n = len(xs)
    if n < 2:
        return 0.0
    m = sum(xs) / n
    return (sum((x - m) ** 2 for x in xs) / n) ** 0.5


def cohen_d(a: List[float], b: List[float]) -> Optional[float]:
    if not a or not b:
        return None
    ma, mb = sum(a) / len(a), sum(b) / len(b)
    pooled = ((std(a) ** 2 * (len(a) - 1) + std(b) ** 2 * (len(b) - 1)) / (len(a) + len(b) - 2)) ** 0.5
    if pooled == 0.0:
        return None
    return (ma - mb) / pooled


def valid_pairs(trades: List[Dict[str, Any]], feat: str) -> List[Tuple[float, float]]:
    return [(t["entry"][feat], t["ret_pct"]) for t in trades if t["entry"].get(feat) is not None]


# ----------------------------------------------------------------------
# 数据加载
# ----------------------------------------------------------------------

def load() -> List[Dict[str, Any]]:
    with open(JSON_PATH, "r", encoding="utf-8") as f:
        return json.load(f)


# ----------------------------------------------------------------------
# 分位数
# ----------------------------------------------------------------------

def quantiles(vals: List[float], qs=(0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9)):
    s = sorted(vals)
    n = len(s)
    out = []
    for q in qs:
        idx = min(n - 1, int(q * n))
        out.append(s[idx])
    return out


# ----------------------------------------------------------------------
# 分析主函数
# ----------------------------------------------------------------------

def main() -> int:
    trades = load()
    md: List[str] = []
    add = md.append

    add("# 多维度监控指标分析报告")
    add("")
    add("- 数据：`feature_report/trade_features.json`（由 `backtest_features.py` 生成，交易逻辑与基线 `backtest.py` 完全一致）")
    add("- 指标快照：每笔交易在**入场**与**出场**时刻各记录 27 个监控指标；另记录持仓路径指标 MFE/MAE")
    add(f"- 大幅盈利阈值 `ret_pct >= {BIG_WIN}%`；大幅亏损阈值 `ret_pct <= {BIG_LOSS}%`（基本对应硬止损）")
    add(f"- 交织判定：`|MA288 - MA488| / MA488 * 100 < 0.5%`（两均线贴合）")
    add("")

    # ---------------- 1. 基线概览 ----------------
    add("## 1. 基线概览")
    add("")
    add("| 币种 | 交易数 | 胜率 | 总收益(简单) | 大盈笔数(≥+5%) | 大亏笔数(≤-1.4%) |")
    add("|---|---|---|---|---|---|")
    by_symbol = defaultdict(list)
    for t in trades:
        by_symbol[t["symbol"]].append(t)
    for sym in sorted(by_symbol.keys()):
        ts = by_symbol[sym]
        n = len(ts)
        wins = sum(1 for t in ts if t["ret_pct"] > 0)
        big_w = sum(1 for t in ts if t["ret_pct"] >= BIG_WIN)
        big_l = sum(1 for t in ts if t["ret_pct"] <= BIG_LOSS)
        tot = sum(t["ret_pct"] for t in ts)
        add(f"| {sym} | {n} | {wins / n * 100:.1f}% | {tot:+.2f}% | {big_w} | {big_l} |")
    n_all = len(trades)
    wins_all = sum(1 for t in trades if t["ret_pct"] > 0)
    tot_all = sum(t["ret_pct"] for t in trades)
    big_w_all = sum(1 for t in trades if t["ret_pct"] >= BIG_WIN)
    big_l_all = sum(1 for t in trades if t["ret_pct"] <= BIG_LOSS)
    add(f"| **合计** | {n_all} | {wins_all / n_all * 100:.1f}% | {tot_all:+.2f}% | {big_w_all} | {big_l_all} |")
    add("")

    # ---------------- 2. 平仓原因 ----------------
    add("## 2. 平仓原因与盈亏")
    add("")
    add("| 平仓原因 | 笔数 | 占比 | 胜率 | 总收益 | 平均收益 | 平均MFE | 平均MAE |")
    add("|---|---|---|---|---|---|---|---|")
    reason_stat: Dict[str, Dict[str, Any]] = {}
    for t in trades:
        r = t["reason"]
        d = reason_stat.setdefault(r, {"n": 0, "wins": 0, "ret": 0.0, "mfe": [], "mae": []})
        d["n"] += 1
        if t["ret_pct"] > 0:
            d["wins"] += 1
        d["ret"] += t["ret_pct"]
        d["mfe"].append(t["mfe_pct"])
        d["mae"].append(t["mae_pct"])
    for r, d in sorted(reason_stat.items(), key=lambda kv: -kv[1]["n"]):
        n = d["n"]
        add(f"| {r} | {n} | {n / n_all * 100:.1f}% | {d['wins'] / n * 100:.1f}% | {d['ret']:+.2f}% | "
            f"{d['ret'] / n:+.2f}% | {mean(d['mfe']):+.2f}% | {mean(d['mae']):+.2f}% |")
    add("")

    # ---------------- 3. MFE/MAE 路径特征 ----------------
    add("## 3. 持仓路径指标（MFE/MAE）：赢家与输家的差异")
    add("")
    groups = {
        "全部": trades,
        "盈利": [t for t in trades if t["ret_pct"] > 0],
        "亏损": [t for t in trades if t["ret_pct"] <= 0],
        "大幅盈利(≥+5%)": [t for t in trades if t["ret_pct"] >= BIG_WIN],
        "大幅亏损(≤-1.4%)": [t for t in trades if t["ret_pct"] <= BIG_LOSS],
    }
    add("| 分组 | 笔数 | 平均持仓bar | 平均MFE% | 平均MAE% | 平均收益% |")
    add("|---|---|---|---|---|---|")
    for name, ts in groups.items():
        if not ts:
            continue
        add(f"| {name} | {len(ts)} | {mean([t['bars'] for t in ts]):.1f} | "
            f"{mean([t['mfe_pct'] for t in ts]):+.2f} | {mean([t['mae_pct'] for t in ts]):+.2f} | "
            f"{mean([t['ret_pct'] for t in ts]):+.2f} |")
    add("")

    # ---------------- 4. 单指标相关性 ----------------
    add("## 4. 入场指标与盈亏的相关性（27 个指标，按 |相关系数| 排序）")
    add("")
    add("`d` = 盈利组与亏损组的标准化均值差(Cohen's d)，正号表示该指标越大越倾向盈利。")
    add("")
    add("| 指标 | 相关系数 | 盈利组均值 | 亏损组均值 | d | 大盈组均值 | 大亏组均值 |")
    add("|---|---|---|---|---|---|---|")
    rows = []
    for feat in FEATURES:
        pairs = valid_pairs(trades, feat)
        if not pairs:
            continue
        xs = [p[0] for p in pairs]
        ys = [p[1] for p in pairs]
        corr = pearson(xs, ys)
        w = [t["entry"][feat] for t in trades if t["ret_pct"] > 0 and t["entry"].get(feat) is not None]
        l = [t["entry"][feat] for t in trades if t["ret_pct"] <= 0 and t["entry"].get(feat) is not None]
        bw = [t["entry"][feat] for t in trades if t["ret_pct"] >= BIG_WIN and t["entry"].get(feat) is not None]
        bl = [t["entry"][feat] for t in trades if t["ret_pct"] <= BIG_LOSS and t["entry"].get(feat) is not None]
        d = cohen_d(w, l)
        rows.append((feat, corr, mean(w), mean(l), d, mean(bw), mean(bl), len(pairs)))
    rows.sort(key=lambda r: (abs(r[1]) if r[1] is not None else 0), reverse=True)
    for feat, corr, mw, ml, d, mbw, mbl, n in rows:
        add(f"| {feat} | {corr:+.3f} | {mw:.3f} | {ml:.3f} | {d:+.2f} | {mbw:.3f} | {mbl:.3f} |")
    add("")

    # ---------------- 5. 分位数敏感性 ----------------
    add("## 5. 关键指标分位数敏感性（5 分位）")
    add("")
    top_feats = [r[0] for r in rows[:10]]
    for feat in top_feats:
        pairs = valid_pairs(trades, feat)
        if not pairs:
            continue
        vals = [p[0] for p in pairs]
        qs = quantiles(vals, (0.2, 0.4, 0.6, 0.8))
        add(f"### {feat}")
        add("")
        add("| 分位区间 | 笔数 | 胜率 | 总收益 | 平均收益 |")
        add("|---|---|---|---|---|")
        bounds = [None] + qs + [None]
        labels = [f"<{qs[0]:.2f}"] + [f"[{qs[i]:.2f},{qs[i+1]:.2f})" for i in range(len(qs) - 1)] + [f">={qs[-1]:.2f}"]
        for b_i in range(5):
            lo, hi = bounds[b_i], bounds[b_i + 1]
            label = labels[b_i]
            bucket = [t for t in trades if t["entry"].get(feat) is not None and
                      (lo is None or t["entry"][feat] >= lo) and (hi is None or t["entry"][feat] < hi)]
            if not bucket:
                continue
            n = len(bucket)
            wins = sum(1 for t in bucket if t["ret_pct"] > 0)
            tot = sum(t["ret_pct"] for t in bucket)
            add(f"| {label} | {n} | {wins / n * 100:.1f}% | {tot:+.2f}% | {tot / n:+.2f}% |")
        add("")

    # ---------------- 6. 震荡/交织行情检测 ----------------
    add("## 6. 震荡/交织/箱体行情检测（核心假设验证）")
    add("")
    add("假设：MA288 与 MA488 反复交叉、长期贴合（交织）的震荡行情里，入场大多亏损。")
    add("")

    def regime_buckets(feat, bins, labels):
        add(f"### 按 {feat} 分组")
        add("")
        add("| 区间 | 笔数 | 胜率 | 总收益 | 平均收益 |")
        add("|---|---|---|---|---|")
        for lo, hi, lab in bins:
            bucket = [t for t in trades if t["entry"].get(feat) is not None and
                      (lo is None or t["entry"][feat] >= lo) and (hi is None or t["entry"][feat] < hi)]
            if not bucket:
                continue
            n = len(bucket)
            wins = sum(1 for t in bucket if t["ret_pct"] > 0)
            tot = sum(t["ret_pct"] for t in bucket)
            add(f"| {lab} | {n} | {wins / n * 100:.1f}% | {tot:+.2f}% | {tot / n:+.2f}% |")
        add("")

    regime_buckets("cross_count_96",
                   [(None, 1, "0 次"), (1, 2, "1 次"), (2, 3, "2 次"), (3, 4, "3 次"), (4, 6, "4~5 次"), (6, None, "≥6 次")],
                   None)
    regime_buckets("interweave_bars_96",
                   [(None, 5, "0~4 根"), (5, 15, "5~14 根"), (15, 30, "15~29 根"), (30, 60, "30~59 根"), (60, None, "≥60 根")],
                   None)
    regime_buckets("mean_spread_96",
                   [(None, 0.5, "<0.5%"), (0.5, 1.0, "0.5~1%"), (1.0, 2.0, "1~2%"), (2.0, 4.0, "2~4%"), (4.0, None, "≥4%")],
                   None)
    regime_buckets("efficiency_ratio_96",
                   [(None, 0.1, "<0.1"), (0.1, 0.2, "0.1~0.2"), (0.2, 0.3, "0.2~0.3"), (0.3, 0.5, "0.3~0.5"), (0.5, None, "≥0.5")],
                   None)
    regime_buckets("donchian_width_96",
                   [(None, 8, "<8%"), (8, 15, "8~15%"), (15, 25, "15~25%"), (25, 40, "25~40%"), (40, None, "≥40%")],
                   None)

    # ---------------- 7. 大幅盈利共同特征 ----------------
    add("## 7. 大幅盈利交易（≥+5%）的共同特征")
    add("")
    big = [t for t in trades if t["ret_pct"] >= BIG_WIN]
    rest = [t for t in trades if t["ret_pct"] < BIG_WIN]
    add(f"大幅盈利 {len(big)} 笔，占 {len(big) / n_all * 100:.1f}%；其总收益 {sum(t['ret_pct'] for t in big):+.2f}%（贡献了全部盈利的绝大部分）。")
    add("")
    add("| 指标 | 大盈组均值 | 其余组均值 | 差值 |")
    add("|---|---|---|---|")
    diffs = []
    for feat in FEATURES:
        a = [t["entry"][feat] for t in big if t["entry"].get(feat) is not None]
        b = [t["entry"][feat] for t in rest if t["entry"].get(feat) is not None]
        if not a or not b:
            continue
        ma_, mb_ = mean(a), mean(b)
        diffs.append((feat, ma_, mb_, ma_ - mb_))
    diffs.sort(key=lambda r: -abs(r[3]))
    for feat, ma_, mb_, diff in diffs[:15]:
        add(f"| {feat} | {ma_:.3f} | {mb_:.3f} | {diff:+.3f} |")
    add("")

    # ---------------- 8. 亏损规避 filter 扫描 ----------------
    add("## 8. 亏损规避过滤规则扫描（单规则）")
    add("")
    add("对每个指标，分别尝试『入场时该指标过高则跳过』与『过低则跳过』两种方向，在分位阈值上扫描；")
    add("`移除总收益`越负，说明被过滤掉的亏损越多；`移除胜率`越低，说明误伤盈利越少。")
    add("")
    candidates = _sweep(trades)
    add("| 排名 | 指标 | 方向 | 阈值 | 移除笔数 | 移除总收益 | 移除胜率 | 保留笔数 | 保留总收益 | 保留胜率 |")
    add("|---|---|---|---|---|---|---|---|---|---|")
    for rank, c in enumerate(candidates[:20], 1):
        add(f"| {rank} | {c['feat']} | {c['dir']} | {c['thr']:.3f} | {c['rem_n']} | {c['rem_ret']:+.2f}% | "
            f"{c['rem_wr']:.1f}% | {c['keep_n']} | {c['keep_ret']:+.2f}% | {c['keep_wr']:.1f}% |")
    add("")

    # ---------------- 8b. 分年度效果（连接 2020/2021 亏损） ----------------
    add("## 8b. 过滤规则分年度效果（连接 2020/2021 亏损）")
    add("")
    rules_yr = [
        ("realized_vol_48 ≥ 0.522 跳过（高波动过滤）", "realized_vol_48", "high", 0.522),
        ("cross_count_96 ≥ 1 跳过（均线交替穿越过滤）", "cross_count_96", "high", 1.0),
        ("donchian_width_96 ≥ 9.15 跳过（宽箱体过滤）", "donchian_width_96", "high", 9.15),
    ]
    years = sorted({t["entry_year"] for t in trades})
    for label, feat, dir_, thr in rules_yr:
        removed, keep = _split_rule(trades, feat, dir_, thr)
        yr_base = defaultdict(float)
        yr_keep = defaultdict(float)
        yr_rem = defaultdict(float)
        for t in trades:
            yr_base[t["entry_year"]] += t["ret_pct"]
        for t in keep:
            yr_keep[t["entry_year"]] += t["ret_pct"]
        for t in removed:
            yr_rem[t["entry_year"]] += t["ret_pct"]
        add(f"### {label}")
        add("")
        add("| 年份 | 基线收益 | 保留收益 | 被移除收益 | 移除笔数 |")
        add("|---|---|---|---|---|")
        for y in years:
            rem_n = sum(1 for t in removed if t["entry_year"] == y)
            add(f"| {y} | {yr_base[y]:+.2f}% | {yr_keep[y]:+.2f}% | {yr_rem[y]:+.2f}% | {rem_n} |")
        add("")

    # ---------------- 9. 震荡类规则专门测试 ----------------
    add("## 9. 震荡/交织类过滤规则（专门测试用户假设）")
    add("")
    osc_feats = ["interweave_bars_48", "interweave_bars_96", "interweave_bars_288",
                 "cross_count_48", "cross_count_96", "cross_count_288",
                 "mean_spread_96", "mean_spread_288", "efficiency_ratio_96", "donchian_width_96"]
    add("| 指标 | 规则 | 移除笔数 | 移除总收益 | 移除胜率 | 保留总收益 | 保留胜率 |")
    add("|---|---|---|---|---|---|---|")
    for feat in osc_feats:
        best = None
        for c in _sweep(trades, only=[feat]):
            if c["rem_ret"] < 0 and c["rem_n"] >= 50:
                if best is None or c["rem_ret"] < best["rem_ret"]:
                    best = c
        if best:
            rule = f"入场时 {feat} {best['dir']} {best['thr']:.2f} 则跳过"
            add(f"| {feat} | {rule} | {best['rem_n']} | {best['rem_ret']:+.2f}% | {best['rem_wr']:.1f}% | "
                f"{best['keep_ret']:+.2f}% | {best['keep_wr']:.1f}% |")
    add("")

    # ---------------- 10. 贪心组合过滤 ----------------
    add("## 10. 贪心组合过滤（多规则叠加：规避亏损且尽量不伤盈利）")
    add("")
    add("从全部候选规则中贪心地逐条加入『最能移除亏损、且移除组胜率 ≤ 20%』的规则，最多 5 条。")
    add("")
    keep = list(trades)
    baseline_ret = sum(t["ret_pct"] for t in trades)
    rules_applied: List[Dict[str, Any]] = []
    add(f"| 步 | 基线/当前保留 | 加入规则 | 本步移除笔数 | 本步移除总收益 | 本步移除胜率 | 累积保留总收益 | 累积保留胜率 |")
    add("|---|---|---|---|---|---|---|---|")
    add(f"| 0 | 全部 {len(trades)} 笔 | — | — | — | — | {baseline_ret:+.2f}% | {wins_all / n_all * 100:.1f}% |")
    for step in range(5):
        best = None
        for c in _sweep(keep):
            if c["rem_n"] >= 30 and c["rem_ret"] < 0 and c["rem_wr"] <= 20.0:
                if best is None or c["rem_ret"] < best["rem_ret"]:
                    best = c
        if best is None:
            break
        rules_applied.append(best)
        keep = _apply_rule(keep, best)
        kwins = sum(1 for t in keep if t["ret_pct"] > 0)
        kret = sum(t["ret_pct"] for t in keep)
        rule = f"{best['feat']} {best['dir']} {best['thr']:.3f}"
        add(f"| {step + 1} | {len(keep) + best['rem_n']} 笔 | {rule} | {best['rem_n']} | {best['rem_ret']:+.2f}% | "
            f"{best['rem_wr']:.1f}% | {kret:+.2f}% | {kwins / len(keep) * 100:.1f}% |")
    # 复利口径对比
    add("")
    add("| 口径 | 全部交易 | 过滤后 |")
    add("|---|---|---|")
    add(f"| 总收益(简单) | {baseline_ret:+.2f}% | {sum(t['ret_pct'] for t in keep):+.2f}% |")
    add(f"| 总收益(复利) | {_compound(trades):+.2f}% | {_compound(keep):+.2f}% |")
    add(f"| 保留笔数 | {len(trades)} | {len(keep)} |")
    add("")

    # ---------------- 11. 结论 ----------------
    add("## 11. 结论速览")
    add("")
    add("- 见上方各表；关键结论请结合第 4~10 节阅读。")
    add("")

    with open(MD_PATH, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {MD_PATH}")
    print(f"总交易数 {len(trades)}，基线总收益 {baseline_ret:+.2f}%")
    return 0


# ----------------------------------------------------------------------
# 过滤扫描与贪心辅助
# ----------------------------------------------------------------------

def _sweep(trades: List[Dict[str, Any]], only: Optional[List[str]] = None) -> List[Dict[str, Any]]:
    feats = only if only is not None else FEATURES
    out: List[Dict[str, Any]] = []
    for feat in feats:
        vals = sorted(t["entry"][feat] for t in trades if t["entry"].get(feat) is not None)
        if len(vals) < 100:
            continue
        for q in (0.1, 0.15, 0.2, 0.25, 0.3, 0.35, 0.4, 0.45, 0.5, 0.55, 0.6, 0.65, 0.7, 0.75, 0.8, 0.85, 0.9):
            thr = vals[min(len(vals) - 1, int(q * len(vals)))]
            for dir_ in ("high", "low"):
                c = _rule_stats(trades, feat, dir_, thr)
                if c["rem_n"] >= 30:
                    out.append(c)
    out.sort(key=lambda c: c["rem_ret"])
    return out


def _rule_stats(trades: List[Dict[str, Any]], feat: str, dir_: str, thr: float) -> Dict[str, Any]:
    removed, keep = _split_rule(trades, feat, dir_, thr)
    return {
        "feat": feat, "dir": dir_, "thr": thr,
        "rem_n": len(removed),
        "rem_ret": sum(t["ret_pct"] for t in removed),
        "rem_wr": _win_rate(removed),
        "keep_n": len(keep),
        "keep_ret": sum(t["ret_pct"] for t in keep),
        "keep_wr": _win_rate(keep),
    }


def _split_rule(trades, feat, dir_, thr):
    removed, keep = [], []
    for t in trades:
        v = t["entry"].get(feat)
        if v is None:
            keep.append(t)  # 无值不参与过滤
            continue
        skip = (dir_ == "high" and v >= thr) or (dir_ == "low" and v <= thr)
        (removed if skip else keep).append(t)
    return removed, keep


def _apply_rule(trades, rule):
    _, keep = _split_rule(trades, rule["feat"], rule["dir"], rule["thr"])
    return keep


def _win_rate(ts):
    return (sum(1 for t in ts if t["ret_pct"] > 0) / len(ts) * 100.0) if ts else 0.0


def _compound(ts):
    eq = 1.0
    for t in ts:
        eq *= (1.0 + t["ret"])
    return (eq - 1.0) * 100.0


if __name__ == "__main__":
    raise SystemExit(main())
