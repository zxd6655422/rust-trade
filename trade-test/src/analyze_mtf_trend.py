"""多时间框架趋势跟踪：4h/1h 级别能不能吃满 2024-2025 翻倍行情。

对比：
  - 30m 策略（A10/A11）：2024-2025 只赚 +16%~+54%
  - 4h 趋势跟踪（close 穿越 4h MA60）：？
  - 1h 趋势跟踪：？

若更高时间框架能赚到接近翻倍，说明「用宏观级别拿大趋势」是正确方向，
可融合进 30m 策略（30m 入场、4h 趋势作为持有依据）。

输出：feature_report/mtf_trend_report.md
"""
from __future__ import annotations

import os
from datetime import datetime

from loader import load_klines_30m, load_klines_1h, load_klines_4h


def sma_series(closes, period):
    n = len(closes)
    p = [0.0] * (n + 1)
    for i in range(n):
        p[i + 1] = p[i] + closes[i]
    out = [None] * n
    for i in range(period - 1, n):
        out[i] = (p[i + 1] - p[i + 1 - period]) / period
    return out


def trend_follow(bars, ma_period, y0, y1):
    """close 穿越 MA(ma_period) 做多/平仓，返回复利收益（限定年份）。"""
    closes = [b.close for b in bars]
    ma = sma_series(closes, ma_period)
    years = [datetime.fromtimestamp(b.open_time / 1000).year for b in bars]
    eq = 1.0
    pos = None  # None 或 'LONG'
    entry = 0.0
    for i in range(len(bars)):
        if ma[i] is None:
            continue
        if not (y0 <= years[i] <= y1):
            continue
        c = closes[i]
        prev_c = closes[i - 1] if i > 0 else c
        prev_ma = ma[i - 1] if i > 0 else ma[i]
        # 穿越：prev_close <= prev_ma 且 close > ma → 金叉做多
        if pos is None and prev_c <= prev_ma and c > ma[i]:
            pos = 'LONG'
            entry = c
        elif pos == 'LONG' and prev_c >= prev_ma and c < ma[i]:
            eq *= c / entry
            pos = None
    if pos == 'LONG':
        eq *= closes[-1] / entry
    return (eq - 1.0) * 100.0


def main() -> int:
    md = []
    add = md.append
    add("# 多时间框架趋势跟踪（close 穿越 MA60）2024-2025 表现")
    add("")
    add("| 币种 | 时间框架 | MA周期 | 2024 复利 | 2025 复利 | 2024+2025 合计 |")
    add("|---|---|---|---|---|---|")

    for coin in ["BTCUSDT", "ETHUSDT", "SOLUSDT"]:
        b1 = load_klines_1h(coin)
        b4 = load_klines_4h(coin)
        for tf, bars in [("1h", b1), ("4h", b4)]:
            for ma_p in ([20, 60] if tf == "4h" else [60, 120]):
                r24 = trend_follow(bars, ma_p, 2024, 2024)
                r25 = trend_follow(bars, ma_p, 2025, 2025)
                # 合计（连乘）
                eq = (1 + r24 / 100) * (1 + r25 / 100)
                add(f"| {coin} | {tf} | MA{ma_p} | {r24:+.1f}% | {r25:+.1f}% | {(eq-1)*100:+.1f}% |")
    add("")
    add("> 对照：30m 策略 A10 在 BTC 2024/2025 分别为 +16.7%/+54.0%；A11 为 +7.4%/+37.7%。")
    add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "mtf_trend_report.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
