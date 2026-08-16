"""查证：ETH/BTC 各年份 realized_vol_48 >= 阈值 的高波动 bar 占比。

vol 过滤不是"跳过某段连续数据"，而是"入场信号触发时，若当时 48 根 bar 的已实现波动率 >= 阈值，跳过这次入场"。
本脚本统计：各年份有多少 30m bar 处于高波动状态（vol>=阈值），即"被过滤的高波动时段"分布。
"""
from __future__ import annotations

import os
from collections import defaultdict
from datetime import datetime

import data_config as dc
from loader import load_klines_30m
from ma_trend_pullback import KlineBar


def realized_vol_48_series(closes):
    import math
    n = len(closes)
    rets = [0.0] * n
    for i in range(1, n):
        if closes[i - 1] != 0.0:
            rets[i] = closes[i] / closes[i - 1] - 1.0
    p = [0.0] * (n + 1)
    p2 = [0.0] * (n + 1)
    for i in range(n):
        p[i + 1] = p[i] + rets[i]
        p2[i + 1] = p2[i] + rets[i] * rets[i]
    W = 48
    out = [None] * n
    for i in range(W, n):
        mean = (p[i + 1] - p[i + 1 - W]) / W
        msq = (p2[i + 1] - p2[i + 1 - W]) / W
        var = msq - mean * mean
        if var < 0.0:
            var = 0.0
        out[i] = math.sqrt(var) * 100.0
    return out


def main():
    for coin in ["BTCUSDT", "ETHUSDT", "SOLUSDT"]:
        thr = dc.SYMBOL_PARAMS[coin].realized_vol_threshold
        bars = load_klines_30m(coin)
        closes = [b.close for b in bars]
        vol = realized_vol_48_series(closes)
        # 按年份统计高波动 bar 占比
        year_total = defaultdict(int)
        year_high = defaultdict(int)
        for b, v in zip(bars, vol):
            if v is None:
                continue
            y = datetime.fromtimestamp(b.open_time / 1000).year
            year_total[y] += 1
            if v >= thr:
                year_high[y] += 1
        print(f"==== {coin} 阈值 {thr} ====")
        print(f"{'年份':6} {'总bar':>8} {'高波动bar':>9} {'占比':>7}")
        for y in sorted(year_total):
            t = year_total[y]
            h = year_high[y]
            print(f"{y:6} {t:>8} {h:>9} {h/t*100:>6.1f}%")
        print()


if __name__ == "__main__":
    main()
