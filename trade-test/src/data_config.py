"""全币种参数配置（供特征回测 / 分析 / OOS 验证统一使用）。

对齐生产：fast=288 / slow=480（生产配置已从 488 改为 480）。
过滤：realized_vol_48 逐币种阈值（对齐生产 realized_vol_threshold）。
BTC/ETH/SOL 沿用生产参数；BNB/SUI/HYPE 来自 backtest_matrix.py 的样本内最优
（BNB/SUI: 硬止损1.0/激活6.0/回调2.0；HYPE: 1.0/6.0/0.5，样本仅约1.2年，结论弱）。
"""
from __future__ import annotations

from typing import Dict, List

import ma_trend_pullback as strat


def _p(hard_stop: float, activate: float, callback: float, rv_thr: float) -> strat.Params:
    return strat.Params(
        fast_ma_period=288, slow_ma_period=480,
        stop_mode="ma288", hard_stop_pct=hard_stop,
        take_profit_mode="trailing", trailing_activate_pct=activate, trailing_callback_pct=callback,
        slope_threshold=0.0, bbw_threshold=0.0, vol_threshold=0.0, realized_vol_threshold=rv_thr,
        use_30m_expanding=False, use_5m_expanding=False, min_angle_5m=0.0,
        entry_timeframe="30m",
    )


SYMBOLS: List[str] = ["BTCUSDT", "ETHUSDT", "SOLUSDT", "BNBUSDT", "SUIUSDT", "HYPEUSDT"]

SYMBOL_PARAMS: Dict[str, strat.Params] = {
    "BTCUSDT": _p(1.5, 4.0, 1.0, 0.426),
    "ETHUSDT": _p(1.5, 5.0, 1.0, 0.445),
    "SOLUSDT": _p(2.0, 4.0, 1.0, 0.790),
    "BNBUSDT": _p(1.0, 6.0, 2.0, 0.488),
    "SUIUSDT": _p(1.0, 6.0, 2.0, 0.788),
    "HYPEUSDT": _p(1.0, 6.0, 0.5, 0.646),
}
