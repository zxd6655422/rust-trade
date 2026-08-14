"""研究 002：过滤对收益稳定性的影响（风险调整视角）。

此前结论「BNB 不建议过滤」只比较了总收益。但过滤的核心价值是抗灾/稳定：
避免某一年像 BTC/ETH/SOL 那样反复止损暴跌。本脚本比较 基线 vs 过滤后 的风险指标：
  总收益、最大回撤、最差年份、亏损年份数、年度收益波动率、类Sharpe。

过滤：逐币种阈值（研究001），入场时 realized_vol_48 >= 阈值 跳过。
输出：studies/002-filter-risk-adjust/results/results.md
"""
import json
import os
from collections import defaultdict

BASE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
JSON = os.path.join(BASE, "src", "feature_report", "trade_features.json")
OUT = os.path.join(BASE, "studies", "002-filter-risk-adjust", "results")
MD = os.path.join(OUT, "results.md")

THRESHOLDS = {
    "BTCUSDT": 0.426, "ETHUSDT": 0.445, "SOLUSDT": 0.790,
    "SUIUSDT": 0.788, "HYPEUSDT": 0.646, "BNBUSDT": 0.488,
}

with open(JSON, "r", encoding="utf-8") as f:
    trades = json.load(f)

coins = sorted({t["symbol"] for t in trades})


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


def max_drawdown(ts):
    s = sorted(ts, key=lambda t: t["entry_time"])
    eq = 1.0
    peak = 1.0
    dd = 0.0
    for t in s:
        eq *= (1.0 + t["ret"])
        peak = max(peak, eq)
        dd = max(dd, (peak - eq) / peak)
    return dd * 100.0


def yearly(ts):
    d = defaultdict(float)
    for t in ts:
        d[t["entry_year"]] += t["ret_pct"]
    return d


def risk_metrics(ts):
    y = yearly(ts)
    ys = sorted(y.values())
    n_years = len(ys)
    mean_y = sum(ys) / n_years if n_years else 0.0
    var = sum((v - mean_y) ** 2 for v in ys) / n_years if n_years else 0.0
    std_y = var ** 0.5
    sharpe = mean_y / std_y if std_y > 0 else 0.0
    losing = sum(1 for v in ys if v < 0)
    return {
        "total_simple": sum(ys),
        "total_compound": compound(ts),
        "max_dd": max_drawdown(ts),
        "worst_year": min(ys) if ys else 0.0,
        "n_years": n_years,
        "n_losing_years": losing,
        "std_yearly": std_y,
        "mean_yearly": mean_y,
        "sharpe_like": sharpe,
    }


lines = []
add = lines.append
add("# 研究 002 结果：基线 vs 过滤后 风险调整对比")
add("")
add("> 过滤：逐币种阈值，入场时 realized_vol_48 >= 阈值 跳过。")
add("")
add("## 1. 每币种 风险指标：基线 vs 过滤后")
add("")
add("| 币种 | 口径 | 总收益(简单) | 总收益(复利) | 最大回撤 | 最差年份 | 亏损年份数 | 年度波动率 | 类Sharpe |")
add("|---|---|---|---|---|---|---|---|---|")
for c in coins:
    ts = [t for t in trades if t["symbol"] == c]
    kt = kept_of(ts, c)
    b = risk_metrics(ts)
    k = risk_metrics(kt)
    add(f"| {c} | 基线 | {b['total_simple']:+.1f}% | {b['total_compound']:+.1f}% | {b['max_dd']:.1f}% | "
        f"{b['worst_year']:+.1f}% | {b['n_losing_years']}/{b['n_years']} | {b['std_yearly']:.1f} | {b['sharpe_like']:.2f} |")
    add(f"| {c} | 过滤后 | {k['total_simple']:+.1f}% | {k['total_compound']:+.1f}% | {k['max_dd']:.1f}% | "
        f"{k['worst_year']:+.1f}% | {k['n_losing_years']}/{k['n_years']} | {k['std_yearly']:.1f} | {k['sharpe_like']:.2f} |")
add("")

# 2. 每币种 分年度 基线→过滤后（简单），用于看"灾年被救回"
add("## 2. 分年度：基线 → 过滤后（简单 %）")
add("")
years = sorted({t["entry_year"] for t in trades})
head = "| 币种 | " + " | ".join(years) + " |"
add(head)
add("|" + "---|" * (len(years) + 1))
for c in coins:
    ts = [t for t in trades if t["symbol"] == c]
    kt = kept_of(ts, c)
    yb = yearly(ts)
    yk = yearly(kt)
    cells = []
    for y in years:
        if y not in yb:
            cells.append("—")
        else:
            cells.append(f"{yb[y]:+.1f}→{yk[y]:+.1f}")
    add(f"| {c} | " + " | ".join(cells) + " |")
add("")

os.makedirs(OUT, exist_ok=True)
with open(MD, "w", encoding="utf-8") as f:
    f.write("\n".join(lines))
print("done")
print(f"[written] {MD}")
