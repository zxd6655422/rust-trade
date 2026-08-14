"""研究 001：逐币种 vol 阈值标定 + 样本外验证。

背景：全局阈值 realized_vol_48 >= 0.522 对低波币(BTC/ETH/SOL)有效，但对高波币(SUI/HYPE)
太紧，会误伤盈利单。本脚本为每个币种单独标定阈值，并用两种时间样本外方法检验。

方法：
  1) 逐币种 walk-forward：用该币过去 12 个月的交易选阈值 → 只应用到下一个月，滚动汇总。
  2) 逐币种时间切分：该币前 60% 交易选阈值 → 后 40% 交易验证。
  3) 各币种 realized_vol_48 分布（分位数），解释为何全局阈值对高波币失效。

数据：src/feature_report/trade_features.json
输出：studies/001-per-coin-vol-threshold/results/
"""
from __future__ import annotations

import json
import os
from collections import defaultdict
from typing import List, Dict, Any, Optional

BASE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
JSON_PATH = os.path.join(BASE, "src", "feature_report", "trade_features.json")
STUDY_DIR = os.path.join(BASE, "studies", "001-per-coin-vol-threshold")
RESULTS = os.path.join(STUDY_DIR, "results")

FEAT = "realized_vol_48"
DIRECTION = "high"  # 跳过 >= 阈值
MIN_KEEP = 0.30


def load() -> List[Dict[str, Any]]:
    with open(JSON_PATH, "r", encoding="utf-8") as f:
        return json.load(f)


def keep_trades(ts, thr):
    kept = []
    for t in ts:
        v = t["entry"].get(FEAT)
        if v is None or v < thr:
            kept.append(t)
    return kept


def total(ts):
    return sum(t["ret_pct"] for t in ts)


def compound(ts):
    s = sorted(ts, key=lambda t: t["entry_time"])
    eq = 1.0
    for t in s:
        eq *= (1.0 + t["ret"])
    return (eq - 1.0) * 100.0


def pick_threshold(train) -> Optional[float]:
    vals = sorted(t["entry"][FEAT] for t in train if t["entry"].get(FEAT) is not None)
    if len(vals) < 30:
        return None
    best = None
    for q in range(5, 96, 5):
        thr = vals[min(len(vals) - 1, int(q / 100.0 * len(vals)))]
        kept = keep_trades(train, thr)
        if len(kept) < len(train) * MIN_KEEP:
            continue
        kret = total(kept)
        if best is None or kret > best[1]:
            best = (thr, kret)
    return best[0] if best else None


def quantile(vals, q):
    s = sorted(vals)
    return s[min(len(s) - 1, int(q * len(s)))]


def main() -> int:
    os.makedirs(RESULTS, exist_ok=True)
    trades = load()
    coins = sorted({t["symbol"] for t in trades})
    for c in coins:
        trades_by_coin = {s: sorted([t for t in trades if t["symbol"] == s], key=lambda t: t["entry_time"]) for s in coins}

    md: List[str] = []
    add = md.append
    add("# 研究 001 结果：逐币种 vol 阈值标定")
    add("")
    add(f"- 指标：{FEAT}；方向：跳过 >= 阈值；选阈值时至少保留 {int(MIN_KEEP*100)}% 交易")
    add("")

    # ---- 1. 分布 ----
    add("## 1. 各币种 realized_vol_48 分布（%）")
    add("")
    add("| 币种 | 笔数 | 中位数 | P70 | P80 | P90 | 均值 |")
    add("|---|---|---|---|---|---|---|")
    dist = {}
    for c in coins:
        vals = [t["entry"][FEAT] for t in trades_by_coin[c] if t["entry"].get(FEAT) is not None]
        dist[c] = vals
        add(f"| {c} | {len(vals)} | {quantile(vals,0.5):.3f} | {quantile(vals,0.7):.3f} | "
            f"{quantile(vals,0.8):.3f} | {quantile(vals,0.9):.3f} | {sum(vals)/len(vals):.3f} |")
    add("")
    add("> 全局阈值 0.522 相当于把每个币「波动率前 X%」的单子砍掉；X 随币种不同差异很大，这就是误伤来源。")
    add("")

    # ---- 2. walk-forward ----
    add("## 2. 逐币种 walk-forward（过去12个月选阈值 → 下一个月）")
    add("")
    add("| 币种 | OOS月数 | 基线简单 | OOS简单 | 基线复利 | OOS复利 | 保留比 | 月度阈值中位 | 阈值范围 |")
    add("|---|---|---|---|---|---|---|---|---|")
    wf_summary = {}
    for c in coins:
        ts = trades_by_coin[c]
        months = sorted({t["entry_time"][:7] for t in ts})
        if len(months) < 13:
            add(f"| {c} | 不足(仅{len(months)}月) | — | — | — | — | — | — | — |")
            continue
        oos_base = []
        oos_keep = []
        thr_list = []
        for mi in range(12, len(months)):
            m = months[mi]
            win_start = months[mi - 12]
            train = [t for t in ts if win_start <= t["entry_time"][:7] < m]
            test = [t for t in ts if t["entry_time"][:7] == m]
            thr = pick_threshold(train)
            if thr is None or not test:
                continue
            thr_list.append(thr)
            kept = keep_trades(test, thr)
            oos_base.extend(test)
            oos_keep.extend(kept)
        n_months = len(thr_list)
        keep_ratio = len(oos_keep) / len(oos_base) * 100 if oos_base else 0
        med_thr = quantile(thr_list, 0.5) if thr_list else 0
        add(f"| {c} | {n_months} | {total(oos_base):+.2f}% | {total(oos_keep):+.2f}% | "
            f"{compound(oos_base):+.2f}% | {compound(oos_keep):+.2f}% | {keep_ratio:.0f}% | "
            f"{med_thr:.3f} | {min(thr_list):.3f}~{max(thr_list):.3f} |")
        wf_summary[c] = (thr_list, oos_base, oos_keep)
    add("")

    # ---- 3. 时间切分 ----
    add("## 3. 逐币种时间切分（前60%选阈值 → 后40%验证）")
    add("")
    add("| 币种 | 训练阈值 | 训练基线→过滤后 | 测试基线→过滤后 | 测试保留比 |")
    add("|---|---|---|---|---|")
    for c in coins:
        ts = trades_by_coin[c]
        if len(ts) < 50:
            continue
        split = int(len(ts) * 0.6)
        train, test = ts[:split], ts[split:]
        thr = pick_threshold(train)
        if thr is None:
            continue
        kept = keep_trades(test, thr)
        add(f"| {c} | {thr:.3f} | {total(train):+.2f}%→{total(keep_trades(train,thr)):+.2f}% | "
            f"{total(test):+.2f}%→{total(kept):+.2f}% | {len(kept)/len(test)*100:.0f}% |")
    add("")

    # ---- 4. 全局 0.522 vs 逐币滚动 汇总对比 ----
    add("## 4. 对比：全局 0.522 vs 逐币种滚动阈值")
    add("")
    add("| 币种 | 基线简单 | 全局0.522简单 | 逐币滚动OOS简单 | 基线复利 | 全局0.522复利 | 逐币滚动OOS复利 |")
    add("|---|---|---|---|---|---|---|")
    for c in coins:
        ts = trades_by_coin[c]
        base_s = total(ts)
        base_c = compound(ts)
        g_s = total(keep_trades(ts, 0.522))
        g_c = compound(keep_trades(ts, 0.522))
        if c in wf_summary:
            _, ob, ok = wf_summary[c]
            w_s = total(ok)
            w_c = compound(ok)
        else:
            w_s = w_c = float("nan")
        w_s_txt = f"{w_s:+.2f}%" if w_s == w_s else "—"
        w_c_txt = f"{w_c:+.2f}%" if w_c == w_c else "—"
        add(f"| {c} | {base_s:+.2f}% | {g_s:+.2f}% | {w_s_txt} | {base_c:+.2f}% | {g_c:+.2f}% | {w_c_txt} |")
    add("")

    md_path = os.path.join(RESULTS, "results.md")
    with open(md_path, "w", encoding="utf-8") as f:
        f.write("\n".join(md))

    # 保存逐币月度阈值序列（供后续查证）
    thr_json = {c: (wf_summary[c][0] if c in wf_summary else []) for c in coins}
    with open(os.path.join(RESULTS, "monthly_thresholds.json"), "w", encoding="utf-8") as f:
        json.dump(thr_json, f, ensure_ascii=False)

    print("\n".join(md))
    print(f"\n[written] {md_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
