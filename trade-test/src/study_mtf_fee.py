"""多时间框架持有 最终验证：手续费敏感性 + 双向时间切分 + 交易结构。

1. 手续费：往返 0%（无）/0.1%/0.2%/0.4% 对比
2. 双向切分：前半训练→后半验证，以及后半训练→前半验证
3. 交易结构：笔数、胜率、平均盈亏

输出：feature_report/mtf_fee_report.md
"""
from __future__ import annotations

import os
from datetime import datetime

import data_config as dc
from loader import load_klines_30m, load_klines_4h
from study_adaptive_ma_trailing import backtest_trades, precompute, comp
from study_mtf_hold import backtest_mtf_hold


def main() -> int:
    md = []
    add = md.append
    add("# 多时间框架持有 最终验证")
    add("")

    for coin in ["BTCUSDT", "ETHUSDT", "SOLUSDT"]:
        params = dc.SYMBOL_PARAMS[coin]
        bars30 = load_klines_30m(coin)
        bars4 = load_klines_4h(coin)
        years = [datetime.fromtimestamp(b.open_time / 1000).year for b in bars30]
        y0, y1 = min(years), max(years)
        mid = (y0 + y1) // 2

        add(f"## {coin}")
        add("")

        # 1. 手续费敏感性（全样本，4h MA40）
        add("### 1. 手续费敏感性（4h MA40，全样本复利）")
        add("")
        add("| 往返手续费 | MTF持有复利 | 30m A1复利 |")
        add("|---|---|---|")
        for fee in [0.0, 0.001, 0.002, 0.004]:
            mtf_t = backtest_mtf_hold(coin, params, bars30, bars4, 40, 4.0, 1.0)
            a1_t = backtest_trades(coin, params, bars30, precompute(bars30), mode="base")
            mtf_rets = [t["ret_pct"] - fee * 100 for t in mtf_t]
            a1_rets = [t["ret_pct"] - fee * 100 for t in a1_t]
            add(f"| {fee*100:.1f}% | {comp(mtf_rets):+.1f}% | {comp(a1_rets):+.1f}% |")
        add("")

        # 2. 交易结构
        mtf_t = backtest_mtf_hold(coin, params, bars30, bars4, 40, 4.0, 1.0)
        a1_t = backtest_trades(coin, params, bars30, precompute(bars30), mode="base")
        add("### 2. 交易结构对比")
        add("")
        add("| 方案 | 笔数 | 胜率 | 平均盈利 | 平均亏损 |")
        add("|---|---|---|---|---|")
        for name, trades in [("MTF持有", mtf_t), ("30m A1", a1_t)]:
            wins = [t["ret_pct"] for t in trades if t["ret_pct"] > 0]
            losses = [t["ret_pct"] for t in trades if t["ret_pct"] <= 0]
            aw = sum(wins) / len(wins) if wins else 0
            al = sum(losses) / len(losses) if losses else 0
            add(f"| {name} | {len(trades)} | {len(wins)/len(trades)*100:.1f}% | {aw:+.2f}% | {al:+.2f}% |")
        add("")

        # 3. 双向切分（后半训练 → 前半验证）
        add("### 3. 反向切分（后半训练选 4h 周期 → 前半验证）")
        add("")
        add("| 4h周期 | 后半训练复利 | 前半验证复利 |")
        add("|---|---|---|")
        for ma4_p in [20, 40, 60, 90]:
            train = comp([t["ret_pct"] for t in backtest_mtf_hold(coin, params, bars30, bars4, ma4_p, 4.0, 1.0, mid+1, y1)])
            val = comp([t["ret_pct"] for t in backtest_mtf_hold(coin, params, bars30, bars4, ma4_p, 4.0, 1.0, y0, mid)])
            add(f"| MA{ma4_p} | {train:+.1f}% | {val:+.1f}% |")
        add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "mtf_fee_report.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
