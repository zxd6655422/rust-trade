"""统一数据加载：按 open_time 升序排序（不假设 CSV 内部顺序）。

CSV 文件内部顺序不一（有的是最新在前，有的最旧在前），这里统一排序后返回升序 K 线。
"""
from __future__ import annotations

import csv
import os
from datetime import datetime, timezone, timedelta
from typing import List

from ma_trend_pullback import KlineBar

BASE_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# 数据已移到仓库外：rust-projects/data_2026-08-13
DATA_DIR = os.path.join(os.path.dirname(os.path.dirname(BASE_DIR)), "data_2026-08-13")

# 30m K 线（BTC/ETH 用 2017-09 起的扩展数据）
CSV_30M = {
    "BTCUSDT": "kline_30m_202608141617_BTC.csv",   # 扩展 2017-09-01 起（统一降序）
    "ETHUSDT": "kline_30m_202608141605_ETH.csv",   # 扩展 2017-09-01 起
    "SOLUSDT": "kline_30m_202608131247_SOL.csv",
    "BNBUSDT": "kline_30m_202608141530_BNB.csv",
    "SUIUSDT": "kline_30m_202608141533_SUI.csv",
    "HYPEUSDT": "kline_30m_202608141537_HYPE.csv",
}

BJ = timezone(timedelta(hours=8))


def load_klines_30m(symbol: str) -> List[KlineBar]:
    path = os.path.join(DATA_DIR, CSV_30M[symbol])
    bars: List[KlineBar] = []
    with open(path, "r", encoding="utf-8", newline="") as f:
        reader = csv.DictReader(f)
        for row in reader:
            dt = datetime.strptime(row["open_time"], "%Y-%m-%d %H:%M:%S.%f %z")
            bars.append(KlineBar(
                open_time=int(dt.timestamp() * 1000),
                open=float(row["open"]),
                high=float(row["high"]),
                low=float(row["low"]),
                close=float(row["close"]),
                volume=float(row["volume"]),
            ))
    bars.sort(key=lambda b: b.open_time)  # 统一升序
    return bars


def fmt_time(ms: int) -> str:
    return datetime.fromtimestamp(ms / 1000, tz=BJ).strftime("%Y-%m-%d %H:%M")
