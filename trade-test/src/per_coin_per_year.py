"""完整逐币种 × 逐年份报告。

每个币种一张表，行=年份，列=笔数/胜率/简单收益/复利收益/平均收益/最大盈利/最大亏损/平仓原因分布。
另附：全币种 × 年份简单收益矩阵 + 币种总计。

读取 feature_report/trade_features.json。
输出 feature_report/per_coin_per_year_full.md
"""
import json
import os
from collections import defaultdict

SRC = os.path.dirname(os.path.abspath(__file__))
JSON = os.path.join(SRC, "feature_report", "trade_features.json")
MD = os.path.join(SRC, "feature_report", "per_coin_per_year_full.md")

with open(JSON, "r", encoding="utf-8") as f:
    trades = json.load(f)

coins = sorted({t["symbol"] for t in trades})
years = sorted({t["entry_year"] for t in trades})


def compound(ts):
    s = sorted(ts, key=lambda t: t["entry_time"])
    eq = 1.0
    for t in s:
        eq *= (1.0 + t["ret"])
    return (eq - 1.0) * 100.0


def reasons(ts):
    d = defaultdict(int)
    for t in ts:
        d[t["reason"]] += 1
    return d


lines = []
add = lines.append
add("# 完整逐币种 × 逐年份报告")
add("")
add("> 收益为「简单相加」与「该年内复利」两种口径；未计手续费/滑点。")
add("")

# 矩阵：全币种 x 年份 简单收益
add("## 0. 全币种 × 年份 简单收益矩阵（%）")
add("")
head = "| 币种 | " + " | ".join(years) + " | 合计 |"
add(head)
add("|" + "---|" * (len(years) + 2))
for c in coins:
    cells = []
    tot = 0.0
    for y in years:
        ts = [t for t in trades if t["symbol"] == c and t["entry_year"] == y]
        s = sum(t["ret_pct"] for t in ts)
        tot += s
        cells.append(f"{s:+.2f}" if ts else "—")
    add(f"| {c} | " + " | ".join(cells) + f" | {tot:+.2f} |")
add("")

# 每个币种一张详细表
for c in coins:
    add(f"## {c}")
    add("")
    add("| 年份 | 笔数 | 胜率 | 简单收益 | 复利收益 | 平均收益 | 最大盈利 | 最大亏损 | 平仓原因(止损/硬止损/止盈/反转/结束) |")
    add("|---|---|---|---|---|---|---|---|---|")
    tot_s = 0.0
    all_ts = []
    for y in years:
        ts = [t for t in trades if t["symbol"] == c and t["entry_year"] == y]
        if not ts:
            continue
        n = len(ts)
        wins = sum(1 for t in ts if t["ret_pct"] > 0)
        wr = wins / n * 100
        s = sum(t["ret_pct"] for t in ts)
        comp = compound(ts)
        avg = s / n
        mx = max(t["ret_pct"] for t in ts)
        mn = min(t["ret_pct"] for t in ts)
        r = reasons(ts)
        rc = f"{r.get('MA288止损',0)}/{r.get('硬止损',0)}/{r.get('移动止盈',0)}/{r.get('趋势反转',0)}/{r.get('持仓到结束',0)}"
        add(f"| {y} | {n} | {wr:.1f}% | {s:+.2f}% | {comp:+.2f}% | {avg:+.2f}% | {mx:+.2f}% | {mn:+.2f}% | {rc} |")
        tot_s += s
        all_ts.extend(ts)
    n = len(all_ts)
    wins = sum(1 for t in all_ts if t["ret_pct"] > 0)
    add(f"| **合计** | {n} | {wins/n*100:.1f}% | {tot_s:+.2f}% | {compound(all_ts):+.2f}% | "
        f"{tot_s/n:+.2f}% | {max(t['ret_pct'] for t in all_ts):+.2f}% | {min(t['ret_pct'] for t in all_ts):+.2f}% | — |")
    add("")

with open(MD, "w", encoding="utf-8") as f:
    f.write("\n".join(lines))

print("\n".join(lines))
print(f"\n[written] {MD}")
