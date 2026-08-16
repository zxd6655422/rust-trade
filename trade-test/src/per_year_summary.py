"""按币种 × 年份 汇总收益（简单相加）与各币种总收益（简单/复利）。

读取 feature_report/trade_features.json。
输出：per_year_summary.md + 控制台表格。
"""
import json
import os
from collections import defaultdict

SRC = os.path.dirname(os.path.abspath(__file__))
JSON = os.path.join(SRC, "feature_report", "trade_features.json")
MD = os.path.join(SRC, "feature_report", "per_year_summary.md")

with open(JSON, "r", encoding="utf-8") as f:
    trades = json.load(f)

coins = sorted({t["symbol"] for t in trades})
years = sorted({t["entry_year"] for t in trades})

# 币种 x 年份 -> [sum, cnt]
grid = defaultdict(lambda: defaultdict(lambda: [0.0, 0]))
coin_total_simple = defaultdict(float)
coin_total_compound = defaultdict(float)
coin_n = defaultdict(int)

for t in trades:
    c = t["symbol"]
    y = t["entry_year"]
    grid[c][y][0] += t["ret_pct"]
    grid[c][y][1] += 1
    coin_total_simple[c] += t["ret_pct"]
    coin_n[c] += 1

eq = defaultdict(lambda: 1.0)
for t in trades:
    eq[t["symbol"]] *= (1.0 + t["ret"])
for c in coins:
    coin_total_compound[c] = (eq[c] - 1.0) * 100.0

lines = []
add = lines.append

# 表1：币种 x 年份（简单收益）
add("# 分币种 × 分年度收益汇总")
add("")
add("> 收益为「简单相加」（每笔收益率直接求和），未计手续费/滑点。")
add("")
add("## 1. 分币种 × 分年度收益（简单相加 %）")
add("")
head = "| 币种 | " + " | ".join(years) + " | 合计(简单) | 合计(复利) | 笔数 |"
add(head)
add("|" + "---|" * (len(years) + 4))
for c in coins:
    cells = [f"{grid[c][y][0]:+.2f}" if grid[c][y][1] else "—" for y in years]
    add(f"| {c} | " + " | ".join(cells) +
        f" | {coin_total_simple[c]:+.2f} | {coin_total_compound[c]:+.2f} | {coin_n[c]} |")
add("")

# 表2：每年合计（全币种）
add("## 2. 分年度收益（全币种合计，简单相加 %）")
add("")
add("| 年份 | 收益% | 笔数 |")
add("|---|---|---|")
year_total = defaultdict(float)
year_n = defaultdict(int)
for t in trades:
    year_total[t["entry_year"]] += t["ret_pct"]
    year_n[t["entry_year"]] += 1
for y in years:
    add(f"| {y} | {year_total[y]:+.2f} | {year_n[y]} |")
add("")

# 表3：币种总计
add("## 3. 币种总收益")
add("")
add("| 币种 | 交易数 | 胜率 | 总收益(简单) | 总收益(复利) |")
add("|---|---|---|---|---|")
for c in coins:
    ts = [t for t in trades if t["symbol"] == c]
    wr = sum(1 for t in ts if t["ret_pct"] > 0) / len(ts) * 100
    add(f"| {c} | {len(ts)} | {wr:.1f}% | {coin_total_simple[c]:+.2f}% | {coin_total_compound[c]:+.2f}% |")
add("")

with open(MD, "w", encoding="utf-8") as f:
    f.write("\n".join(lines))

print("\n".join(lines))
print(f"\n[written] {MD}")
