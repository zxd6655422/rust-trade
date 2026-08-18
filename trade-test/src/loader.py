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

# 1h / 4h K 线（多时间框架分析用，BTC/ETH/SOL）
CSV_1H = {
    "BTCUSDT": "kline_1h_202608190050_BTC.csv",
    "ETHUSDT": "kline_1h_202608190050_ETH.csv",
    "SOLUSDT": "kline_1h_202608190051_SOL.csv",
}
CSV_4H = {
    "BTCUSDT": "kline_4h_202608190050_BTC.csv",
    "ETHUSDT": "kline_4h_202608190051_ETH.csv",
    "SOLUSDT": "kline_4h_202608190052_SOL.csv",
}
# 5m K 线（短周期支撑压力位分析用）
CSV_5M = {
    "BTCUSDT": "kline_5m_202608131243_BTC.csv",
    "ETHUSDT": "kline_5m_202608131246_ETH.csv",
    "SOLUSDT": "kline_5m_202608131248_SOL.csv",
    "BNBUSDT": "kline_5m_202608141531_BNB.csv",
    "SUIUSDT": "kline_5m_202608141535_SUI.csv",
    "HYPEUSDT": "kline_5m_202608141538_HYPE.csv",
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


def _load_csv(filename: str) -> List[KlineBar]:
    path = os.path.join(DATA_DIR, filename)
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
    bars.sort(key=lambda b: b.open_time)
    return bars


def load_klines_1h(symbol: str) -> List[KlineBar]:
    return _load_csv(CSV_1H[symbol])


def load_klines_4h(symbol: str) -> List[KlineBar]:
    return _load_csv(CSV_4H[symbol])


def load_klines_5m(symbol: str) -> List[KlineBar]:
    return _load_csv(CSV_5M[symbol])


def fmt_time(ms: int) -> str:
    return datetime.fromtimestamp(ms / 1000, tz=BJ).strftime("%Y-%m-%d %H:%M")
