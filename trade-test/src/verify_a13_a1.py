r"""补充核查：A1 基线（新数据）+ lookahead 修正口径敏感性（et vs et+30m）。"""
from __future__ import annotations

import os
from bisect import bisect_left, bisect_right
from datetime import datetime, timezone, timedelta

import data_config as dc
from loader import load_klines_30m, load_klines_4h
from study_adaptive_ma_trailing import backtest_trades, precompute, comp

BJ = timezone(timedelta(hours=8))
BAR_30M_MS = 30 * 60 * 1000
BAR_4H_MS = 4 * 60 * 60 * 1000


def sma_series(closes, period):
    n = len(closes)
    p = [0.0] * (n + 1)
    for i in range(n):
        p[i + 1] = p[i] + closes[i]
    out = [None] * n
    for i in range(period - 1, n):
        out[i] = (p[i + 1] - p[i + 1 - period]) / period
    return out


def backtest_fixed(symbol, params, bars30, bars4, ref="close"):
    """ref='close' -> 用 30m bar 收盘(et+30m)时刻已收盘的4h bar；ref='open' -> 用 bar 开盘(et)时刻。"""
    fast = params.fast_ma_period
    slow = params.slow_ma_period
    n = len(bars30)
    years = [datetime.fromtimestamp(b.open_time / 1000).year for b in bars30]
    closes = [b.close for b in bars30]
    pre = precompute(bars30)
    vol48 = pre["vol48"]
    prefix = pre["prefix"]
    closes4 = [b.close for b in bars4]
    ma4 = sma_series(closes4, 40)
    ts4 = [b.open_time for b in bars4]
    shift = BAR_30M_MS if ref == "close" else 0

    def fourh_bearish(et):
        j = bisect_right(ts4, et + shift - BAR_4H_MS) - 1
        if j < 0 or ma4[j] is None:
            return False
        return closes4[j] < ma4[j]

    def sma_at(idx, period):
        if idx + 1 < period:
            return None
        return (prefix[idx + 1] - prefix[idx + 1 - period]) / period

    trades = []
    pos = None
    for i in range(n):
        if i + 1 < slow:
            continue
        close = closes[i]
        prev_close = closes[i - 1]
        fast_ma = sma_at(i, fast)
        slow_ma = sma_at(i, slow)
        prev_fast_ma = sma_at(i - 1, fast)
        if pos is not None:
            bar = bars30[i]
            side = pos["side"]
            entry = pos["entry"]
            pnl = (close - entry) / entry * 100.0 if side == "LONG" else (entry - close) / entry * 100.0
            pos["max_profit"] = max(pos["max_profit"], pnl)
            exit_price = None
            if params.hard_stop_pct > 0.0:
                if side == "LONG" and bar.low <= pos["hard_stop"]:
                    exit_price = pos["hard_stop"]
                elif side == "SHORT" and bar.high >= pos["hard_stop"]:
                    exit_price = pos["hard_stop"]
            if exit_price is None:
                if side == "LONG" and fourh_bearish(bars30[i].open_time):
                    exit_price = close
                elif side == "SHORT" and not fourh_bearish(bars30[i].open_time):
                    exit_price = close
            if exit_price is None and pos["max_profit"] >= 4.0 and pos["max_profit"] - pnl >= 1.0:
                exit_price = close
            if exit_price is not None:
                ret = (exit_price - entry) / entry if side == "LONG" else (entry - exit_price) / entry
                trades.append(ret * 100.0)
                pos = None
                continue
        if pos is None and fast_ma is not None and slow_ma is not None and prev_fast_ma is not None:
            if vol48[i] is not None and vol48[i] >= params.realized_vol_threshold:
                continue
            if fast_ma > slow_ma and prev_close < prev_fast_ma and close > fast_ma:
                pos = {"side": "LONG", "entry": close, "hard_stop": close * (1.0 - params.hard_stop_pct / 100.0), "max_profit": 0.0}
            elif fast_ma < slow_ma and prev_close > prev_fast_ma and close < fast_ma:
                pos = {"side": "SHORT", "entry": close, "hard_stop": close * (1.0 + params.hard_stop_pct / 100.0), "max_profit": 0.0}
    if pos is not None:
        ret = (closes[-1] - pos["entry"]) / pos["entry"] if pos["side"] == "LONG" else (pos["entry"] - closes[-1]) / pos["entry"]
        trades.append(ret * 100.0)
    return trades


def main():
    print(f"{'币种':<10} {'A1基线(新数据)':>14} {'MTF修正(ref=close)':>18} {'MTF修正(ref=open)':>18}")
    for coin in ["BTCUSDT", "ETHUSDT", "SOLUSDT", "BNBUSDT", "SUIUSDT", "HYPEUSDT"]:
        params = dc.SYMBOL_PARAMS[coin]
        bars30 = load_klines_30m(coin)
        bars4 = load_klines_4h(coin)
        a1 = comp([t["ret_pct"] for t in backtest_trades(coin, params, bars30, precompute(bars30), mode="base")])
        c = comp(backtest_fixed(coin, params, bars30, bars4, ref="close"))
        o = comp(backtest_fixed(coin, params, bars30, bars4, ref="open"))
        print(f"{coin:<10} {a1:>+14.1f} {c:>+18.1f} {o:>+18.1f}")


if __name__ == "__main__":
    main()
