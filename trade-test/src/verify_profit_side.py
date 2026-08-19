r"""回到止盈端（A7/A9/A10/A11）——重跑当前数据下的复利，确认最优组合。

注意：这些方案全部用 30m MA288 止损（不用 4h 数据），因此不受 4h lookahead bug 影响。
"""
from __future__ import annotations

import data_config as dc
from loader import load_klines_30m
from study_adaptive_ma_trailing import backtest_trades, precompute, comp
from study_hybrid_trailing import backtest_hybrid
from study_tiered_hybrid import backtest_tiered3

SWITCH = {
    "BTCUSDT": (6.0, 12.0),
    "ETHUSDT": (4.0, 15.0),
    "SOLUSDT": (4.0, 20.0),
}


def main():
    print(f"{'方案':<24}{'BTC':>10}{'ETH':>10}{'SOL':>10}")
    rows = []
    for coin in ["BTCUSDT", "ETHUSDT", "SOLUSDT"]:
        params = dc.SYMBOL_PARAMS[coin]
        bars = load_klines_30m(coin)
        pre = precompute(bars)

        a1 = comp([t["ret_pct"] for t in backtest_trades(coin, params, bars, pre, mode="base")])
        a7 = comp([t["ret_pct"] for t in backtest_trades(coin, params, bars, pre, mode="ma192", activate=15.0, confirm=10)])
        a9 = comp([t["ret_pct"] for t in backtest_trades(coin, params, bars, pre, mode="tiered_demote", confirm=10, switch_at=20.0, demote_pct=10.0, activate=15.0)])
        a10 = comp([t["ret_pct"] for t in backtest_hybrid(coin, params, bars, pre, switch_at=8.0, activate_small=4.0, callback_small=1.5, demote_pct=10.0, confirm=10)])
        s1, s2 = SWITCH[coin]
        a11 = comp([t["ret_pct"] for t in backtest_tiered3(coin, params, bars, pre, s1, s2)])
        rows.append((coin, a1, a7, a9, a10, a11))

    names = ["A1 基线(生产移动止盈)", "A7 MA192 c10(activate15)", "A9 分级+衰竭降级10%", "A10 两段式(switch8)", "A11 三段式(per-coin)"]
    for k, name in enumerate(names):
        cells = [f"{rows[i][k+1]:+.1f}%" for i in range(3)]
        print(f"{name:<24}{cells[0]:>10}{cells[1]:>10}{cells[2]:>10}")


if __name__ == "__main__":
    main()
