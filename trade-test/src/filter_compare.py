"""基线 vs 高波动过滤 对比（分币种 × 分年度 + 总收益）。

过滤规则：入场时 realized_vol_48 >= 0.522 则跳过（与 feature_analysis_report 8b 一致）。
读取 feature_report/trade_features.json。
"""
import json
import os
from collections import defaultdict

SRC = os.path.dirname(os.path.abspath(__file__))
JSON = os.path.join(SRC, "feature_report", "trade_features.json")

with open(JSON, "r", encoding="utf-8") as f:
    trades = json.load(f)

THR = 0.522
FEAT = "realized_vol_48"


def is_kept(t):
    v = t["entry"].get(FEAT)
    return v is None or v < THR


kept = [t for t in trades if is_kept(t)]
removed = [t for t in trades if not is_kept(t)]

coins = sorted({t["symbol"] for t in trades})
years = sorted({t["entry_year"] for t in trades})


def agg(ts):
    d = defaultdict(lambda: defaultdict(float))
    tot = defaultdict(float)
    n = defaultdict(int)
    for t in ts:
        d[t["symbol"]][t["entry_year"]] += t["ret_pct"]
        tot[t["symbol"]] += t["ret_pct"]
        n[t["symbol"]] += 1
    return d, tot, n


base_grid, base_tot, base_n = agg(trades)
keep_grid, keep_tot, keep_n = agg(kept)
rem_grid, rem_tot, rem_n = agg(removed)


def compound(ts, symbol):
    eq = 1.0
    for t in ts:
        if t["symbol"] == symbol:
            eq *= (1.0 + t["ret"])
    return (eq - 1.0) * 100.0


lines = []
add = lines.append
add("# 基线 vs 高波动过滤 对比（realized_vol_48 ≥ 0.522 跳过）")
add("")
add("## 1. 分币种总收益：基线 vs 过滤后")
add("")
add("| 币种 | 基线简单 | 基线复利 | 过滤后简单 | 过滤后复利 | 保留笔数/总笔数 | 被移除收益 |")
add("|---|---|---|---|---|---|---|")
for c in coins:
    add(f"| {c} | {base_tot[c]:+.2f}% | {compound(trades, c):+.2f}% | {keep_tot[c]:+.2f}% | "
        f"{compound(kept, c):+.2f}% | {keep_n[c]}/{base_n[c]} | {rem_tot[c]:+.2f}% |")
add("")

add("## 2. 分年度：基线 vs 过滤后（全币种合计，简单）")
add("")
add("| 年份 | 基线 | 过滤后 | 被移除 | 移除笔数 |")
add("|---|---|---|---|---|")
year_base = defaultdict(float)
year_keep = defaultdict(float)
year_rem = defaultdict(float)
year_remn = defaultdict(int)
for t in trades:
    year_base[t["entry_year"]] += t["ret_pct"]
for t in kept:
    year_keep[t["entry_year"]] += t["ret_pct"]
for t in removed:
    year_rem[t["entry_year"]] += t["ret_pct"]
    year_remn[t["entry_year"]] += 1
for y in years:
    add(f"| {y} | {year_base[y]:+.2f}% | {year_keep[y]:+.2f}% | {year_rem[y]:+.2f}% | {year_remn[y]} |")
add("")

add("## 3. 分币种 × 分年度：基线 → 过滤后（简单，'→'后为过滤后）")
add("")
head = "| 币种 | " + " | ".join(years) + " |"
add(head)
add("|" + "---|" * (len(years) + 1))
for c in coins:
    cells = []
    for y in years:
        b = base_grid[c][y]
        k = keep_grid[c][y]
        if base_n[c] == 0:
            cells.append("—")
        elif b == 0.0 and k == 0.0:
            cells.append("0.00")
        else:
            cells.append(f"{b:+.2f}→{k:+.2f}")
    add(f"| {c} | " + " | ".join(cells) + " |")
add("")

MD = os.path.join(SRC, "feature_report", "filter_compare.md")
with open(MD, "w", encoding="utf-8") as f:
    f.write("\n".join(lines))
print("\n".join(lines))
print(f"\n[written] {MD}")
