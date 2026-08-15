"""对比 slow_ma_period = 488 vs 480 的基线收益差异（反转出场OFF，即当前 Python 口径）。"""
from __future__ import annotations

import backtest as bt
import data_config as dc
from loader import load_klines_30m
from ma_trend_pullback import Params


def make_params(base: Params, slow: int) -> Params:
    return Params(
        fast_ma_period=base.fast_ma_period, slow_ma_period=slow,
        stop_mode=base.stop_mode, hard_stop_pct=base.hard_stop_pct,
        take_profit_mode=base.take_profit_mode,
        trailing_activate_pct=base.trailing_activate_pct,
        trailing_callback_pct=base.trailing_callback_pct,
        slope_threshold=0.0, bbw_threshold=0.0, vol_threshold=0.0,
        use_30m_expanding=False, use_5m_expanding=False, min_angle_5m=0.0,
        entry_timeframe="30m",
    )


def main() -> int:
    print(f"{'币种':12} {'slow488简单':>12} {'slow480简单':>12} {'差值':>10} | "
          f"{'slow488复利':>12} {'slow480复利':>12} {'差值':>10} | {'488笔数':>7} {'480笔数':>7}")
    print("-" * 100)
    for sym in dc.SYMBOLS:
        base = dc.SYMBOL_PARAMS[sym]
        bars = load_klines_30m(sym)
        m488 = bt.compute_metrics(bt.backtest(sym, make_params(base, 488), bars))
        m480 = bt.compute_metrics(bt.backtest(sym, make_params(base, 480), bars))
        print(f"{sym:12} {m488['total_ret']:>+12.2f}% {m480['total_ret']:>+12.2f}% "
              f"{m480['total_ret']-m488['total_ret']:>+10.2f}% | "
              f"{m488['compound_ret']:>+12.2f}% {m480['compound_ret']:>+12.2f}% "
              f"{m480['compound_ret']-m488['compound_ret']:>+10.2f}% | {m488['n']:>7} {m480['n']:>7}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
