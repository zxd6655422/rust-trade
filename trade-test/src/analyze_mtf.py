"""多时间框架诊断：4h 宏观趋势对 30m 交易盈亏的影响。

- 4h 趋势状态：4h MA20 vs MA60（上升/下降/横盘）
- 对齐：每个 30m bar 用「它之前最近一根已收盘 4h bar」的趋势状态
- 统计：不同 4h 状态下，30m 交易（A1 基线）的笔数/胜率/收益

回答：30m 做多信号在「4h 上升趋势」里是否显著更优（能否用宏观趋势过滤提升）。

输出：feature_report/mtf_diagnosis.md
"""
from __future__ import annotations

import os
from bisect import bisect_right
from collections import defaultdict

import data_config as dc
from loader import load_klines_30m, load_klines_4h
from study_adaptive_ma_trailing import backtest_trades, precompute


def sma_series(closes, period):
    n = len(closes)
    p = [0.0] * (n + 1)
    for i in range(n):
        p[i + 1] = p[i] + closes[i]
    out = [None] * n
    for i in range(period - 1, n):
        out[i] = (p[i + 1] - p[i + 1 - period]) / period
    return out


def fourh_state(bars_4h):
    """返回 4h 每根 bar 的趋势状态：+1 上升 / -1 下降 / 0 横盘。"""
    closes = [b.close for b in bars_4h]
    ma20 = sma_series(closes, 20)
    ma60 = sma_series(closes, 60)
    states = []
    for i in range(len(bars_4h)):
        if ma20[i] is None or ma60[i] is None:
            states.append(None)
            continue
        spread = (ma20[i] - ma60[i]) / ma60[i]
        if spread > 0.001:
            states.append(1)   # 上升
        elif spread < -0.001:
            states.append(-1)  # 下降
        else:
            states.append(0)   # 横盘
    return states


def main() -> int:
    md = []
    add = md.append
    add("# 4h 宏观趋势对 30m 交易盈亏的影响")
    add("")
    add("- 4h 状态：MA20>MA60 上升、MA20<MA60 下降、其余横盘。")
    add("")

    for coin in ["BTCUSDT", "ETHUSDT", "SOLUSDT"]:
        params = dc.SYMBOL_PARAMS[coin]
        bars30 = load_klines_30m(coin)
        bars4 = load_klines_4h(coin)
        states4 = fourh_state(bars4)
        ts4 = [b.open_time for b in bars4]

        # 30m 交易（A1 基线）
        pre = precompute(bars30)
        trades = backtest_trades(coin, params, bars30, pre, mode="base")

        # 对齐：每笔 30m 交易入场时，之前最近一根 4h bar 的状态
        add(f"## {coin}")
        add("")
        add("| 4h状态 | 笔数 | 胜率 | 总收益 | 平均收益 |")
        add("|---|---|---|---|---|")
        for state, label in [(1, "上升"), (-1, "下降"), (0, "横盘")]:
            bucket = []
            for t in trades:
                et = bars30[t["entry_idx"]].open_time
                j = bisect_right(ts4, et) - 1  # 最近一根 <= et 的 4h bar
                if j < 0:
                    continue
                if states4[j] == state:
                    bucket.append(t)
            if not bucket:
                add(f"| {label} | 0 | — | — | — |")
                continue
            n = len(bucket)
            w = sum(1 for t in bucket if t["ret_pct"] > 0)
            tot = sum(t["ret_pct"] for t in bucket)
            add(f"| {label} | {n} | {w/n*100:.1f}% | {tot:+.2f}% | {tot/n:+.2f}% |")
        add("")

        # 方向细分：4h 上升时做多 vs 做空
        add("### 4h 上升时，做多 vs 做空")
        add("")
        add("| 方向 | 笔数 | 胜率 | 总收益 | 平均收益 |")
        add("|---|---|---|---|---|")
        for side, slabel in [("LONG", "做多"), ("SHORT", "做空")]:
            bucket = []
            for t in trades:
                if t["side"] != side:
                    continue
                et = bars30[t["entry_idx"]].open_time
                j = bisect_right(ts4, et) - 1
                if j >= 0 and states4[j] == 1:
                    bucket.append(t)
            if not bucket:
                add(f"| {slabel} | 0 | — | — | — |")
                continue
            n = len(bucket)
            w = sum(1 for t in bucket if t["ret_pct"] > 0)
            tot = sum(t["ret_pct"] for t in bucket)
            add(f"| {slabel} | {n} | {w/n*100:.1f}% | {tot:+.2f}% | {tot/n:+.2f}% |")
        add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "mtf_diagnosis.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
