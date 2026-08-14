"""过滤后 逐币种 × 逐年份 报告。

过滤规则：逐币种阈值（来自研究 001 walk-forward 中位数），入场时 realized_vol_48 >= 阈值 则跳过。
BNB 依据研究 001 结论「不建议过滤」，此处仍给出（标注仅供参考）。

输出 feature_report/filtered_per_year.md
"""
import json
import os
from collections import defaultdict

SRC = os.path.dirname(os.path.abspath(__file__))
JSON = os.path.join(SRC, "feature_report", "trade_features.json")
MD = os.path.join(SRC, "feature_report", "filtered_per_year.md")

THRESHOLDS = {
    "BTCUSDT": 0.426,
    "ETHUSDT": 0.445,
    "SOLUSDT": 0.790,
    "SUIUSDT": 0.788,
    "HYPEUSDT": 0.646,
    "BNBUSDT": 0.488,   # 研究 001 结论：BNB 不建议过滤，仅供参考
}
NOT_RECOMMENDED = {"BNBUSDT"}

with open(JSON, "r", encoding="utf-8") as f:
    trades = json.load(f)

coins = sorted({t["symbol"] for t in trades})
years = sorted({t["entry_year"] for t in trades})


def kept_of(coin_trades, c):
    thr = THRESHOLDS[c]
    return [t for t in coin_trades
            if t["entry"].get("realized_vol_48") is None or t["entry"]["realized_vol_48"] < thr]


def compound(ts):
    s = sorted(ts, key=lambda t: t["entry_time"])
    eq = 1.0
    for t in s:
        eq *= (1.0 + t["ret"])
    return (eq - 1.0) * 100.0


by_coin = {c: [t for t in trades if t["symbol"] == c] for c in coins}
kept_all = []
kept_by_coin = {}
for c in coins:
    k = kept_of(by_coin[c], c)
    kept_by_coin[c] = k
    kept_all.extend(k)

lines = []
add = lines.append
add("# 过滤后 逐币种 × 逐年份 报告")
add("")
add("> 过滤规则：逐币种阈值（研究001），入场时 `realized_vol_48 >= 阈值` 跳过。")
add("> 阈值：BTC 0.426 / ETH 0.445 / SOL 0.790 / SUI 0.788 / HYPE 0.646 / BNB 0.488(仅供参考，研究001建议BNB不过滤)。")
add("> 收益口径：简单相加 与 年内复利；未计手续费/滑点。")
add("")

add("## 0. 过滤后 全币种 × 年份 简单收益矩阵（%）")
add("")
head = "| 币种 | " + " | ".join(years) + " | 合计 |"
add(head)
add("|" + "---|" * (len(years) + 2))
for c in coins:
    kt = kept_by_coin[c]
    cells = []
    tot = 0.0
    for y in years:
        ts = [t for t in kt if t["entry_year"] == y]
        s = sum(t["ret_pct"] for t in ts)
        tot += s
        cells.append(f"{s:+.2f}" if ts else "—")
    flag = " [注意]" if c in NOT_RECOMMENDED else ""
    add(f"| {c}{flag} | " + " | ".join(cells) + f" | {tot:+.2f} |")
add("")

for c in coins:
    kt = kept_by_coin[c]
    flag = "（注意：研究001建议不过滤，仅供参考）" if c in NOT_RECOMMENDED else ""
    add(f"## {c} {flag}")
    add("")
    add("| 年份 | 保留/总笔数 | 胜率 | 简单收益 | 复利收益 | 最大盈利 |")
    add("|---|---|---|---|---|---|")
    tot_s = 0.0
    all_kt = []
    for y in years:
        ts_all = [t for t in by_coin[c] if t["entry_year"] == y]
        ts = [t for t in kt if t["entry_year"] == y]
        if not ts_all:
            continue
        n_all = len(ts_all)
        n = len(ts)
        wins = sum(1 for t in ts if t["ret_pct"] > 0)
        wr = wins / n * 100 if n else 0
        s = sum(t["ret_pct"] for t in ts)
        comp = compound(ts) if ts else 0.0
        mx = max(t["ret_pct"] for t in ts) if ts else 0.0
        add(f"| {y} | {n}/{n_all} | {wr:.1f}% | {s:+.2f}% | {comp:+.2f}% | {mx:+.2f}% |")
        tot_s += s
        all_kt.extend(ts)
    n = len(all_kt)
    wins = sum(1 for t in all_kt if t["ret_pct"] > 0)
    add(f"| **合计** | {n}/{len(by_coin[c])} | {wins/n*100:.1f}% | {tot_s:+.2f}% | "
        f"{compound(all_kt):+.2f}% | {max((t['ret_pct'] for t in all_kt), default=0):+.2f}% |")
    add("")

add("## 汇总：基线 vs 过滤后（每币种）")
add("")
add("| 币种 | 基线简单 | 过滤后简单 | 基线复利 | 过滤后复利 | 保留笔数/总 |")
add("|---|---|---|---|---|---|")
for c in coins:
    ts = by_coin[c]
    kt = kept_by_coin[c]
    add(f"| {c} | {sum(t['ret_pct'] for t in ts):+.2f}% | {sum(t['ret_pct'] for t in kt):+.2f}% | "
        f"{compound(ts):+.2f}% | {compound(kt):+.2f}% | {len(kt)}/{len(ts)} |")
add("")

add("## 每年合计：基线 vs 过滤后（全币种简单）")
add("")
add("| 年份 | 基线 | 过滤后 | 被移除 |")
add("|---|---|---|---|")
yb = defaultdict(float)
yk = defaultdict(float)
for t in trades:
    yb[t["entry_year"]] += t["ret_pct"]
for t in kept_all:
    yk[t["entry_year"]] += t["ret_pct"]
for y in years:
    add(f"| {y} | {yb[y]:+.2f}% | {yk[y]:+.2f}% | {yk[y]-yb[y]:+.2f}% |")
add("")

with open(MD, "w", encoding="utf-8") as f:
    f.write("\n".join(lines))

print("done")
print(f"[written] {MD}")
