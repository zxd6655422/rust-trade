"""对比：保留「趋势反转出场」 vs 取消「趋势反转出场」的收益差异。

背景：
- 生产 Rust 策略（ma_trend_pullback.rs）从未实现「趋势反转出场」逻辑，
  回测脚本 backtest.py 之前多算了这一步（MA288 与 MA488 交叉即平仓），
  在 commit 478f94b「优化过滤参数」中被注释掉。
- 本脚本用同一份数据、同一套参数，量化这一步开/关对收益的影响。

三种口径（便于对照 JS 第十六次测试与 Python 当前版本）：
  1. Python当前(反转OFF)：硬止损 + MA288止损 + 移动止盈        （backtest.py 现状）
  2. Python旧版(反转ON) ：硬止损 + MA288止损 + 移动止盈 + 趋势反转
  3. JS第十六次实际      ：硬止损 + 移动止盈                    （MA288止损、趋势反转都禁用）

运行：cd trade-test/src && python compare_reversal_exit.py
"""
from __future__ import annotations

from typing import List, Dict, Any, Optional

import backtest as bt
import data_config as dc
from loader import load_klines_30m
from ma_trend_pullback import KlineBar, Params


def backtest_toggle(
    symbol: str,
    params: Params,
    bars: List[KlineBar],
    use_ma288_stop: bool = True,
    use_reversal: bool = False,
) -> List[Dict[str, Any]]:
    """与 backtest.backtest 逻辑一致，但可独立开关 MA288止损 / 趋势反转出场。"""
    fast = params.fast_ma_period
    slow = params.slow_ma_period
    n = len(bars)
    closes = [b.close for b in bars]

    prefix = [0.0] * (n + 1)
    for i in range(n):
        prefix[i + 1] = prefix[i] + closes[i]

    def sma_at(idx: int, period: int) -> Optional[float]:
        if idx + 1 < period:
            return None
        return (prefix[idx + 1] - prefix[idx + 1 - period]) / period

    trades: List[Dict[str, Any]] = []
    pos: Optional[Dict[str, Any]] = None

    for i in range(n):
        if i + 1 < slow:
            continue

        close = closes[i]
        prev_close = closes[i - 1]
        fast_ma = sma_at(i, fast)
        slow_ma = sma_at(i, slow)
        prev_fast_ma = sma_at(i - 1, fast)

        # ---- 持仓中：平仓（优先级：硬止损 > MA288止损 > 移动止盈 > 趋势反转） ----
        if pos is not None:
            bar = bars[i]
            side = pos["side"]
            entry = pos["entry_price"]
            pnl = (close - entry) / entry * 100.0 if side == "LONG" else (entry - close) / entry * 100.0
            pos["max_profit"] = max(pos["max_profit"], pnl)

            exit_price: Optional[float] = None
            reason = ""

            # 1. 硬止损
            if params.hard_stop_pct > 0.0:
                if side == "LONG" and bar.low <= pos["hard_stop_price"]:
                    exit_price, reason = pos["hard_stop_price"], "硬止损"
                elif side == "SHORT" and bar.high >= pos["hard_stop_price"]:
                    exit_price, reason = pos["hard_stop_price"], "硬止损"

            # 2. MA288 止损（收盘价反向穿越 MA288）
            if use_ma288_stop and exit_price is None and params.stop_mode == "ma288" and prev_fast_ma is not None:
                if side == "LONG" and prev_close > prev_fast_ma and close < fast_ma:
                    exit_price, reason = close, "MA288止损"
                elif side == "SHORT" and prev_close < prev_fast_ma and close > fast_ma:
                    exit_price, reason = close, "MA288止损"

            # 3. 移动止盈
            if exit_price is None and params.take_profit_mode == "trailing":
                if pos["max_profit"] >= params.trailing_activate_pct:
                    if pos["max_profit"] - pnl >= params.trailing_callback_pct:
                        exit_price, reason = close, "移动止盈"

            # 4. 趋势反转（MA288 与 MA488 交叉）
            if use_reversal and exit_price is None:
                if side == "LONG" and fast_ma < slow_ma:
                    exit_price, reason = close, "趋势反转"
                elif side == "SHORT" and fast_ma > slow_ma:
                    exit_price, reason = close, "趋势反转"

            if exit_price is not None:
                ret = (exit_price - entry) / entry if side == "LONG" else (entry - exit_price) / entry
                trades.append({
                    "symbol": symbol, "side": "多" if side == "LONG" else "空",
                    "ret": ret, "ret_pct": ret * 100.0, "reason": reason,
                    "bars": i - pos["entry_idx"],
                })
                pos = None
                continue

        # ---- 无持仓：入场 ----
        if pos is None and fast_ma is not None and slow_ma is not None and prev_fast_ma is not None:
            if fast_ma > slow_ma:
                if prev_close < prev_fast_ma and close > fast_ma:
                    hard_stop = close * (1.0 - params.hard_stop_pct / 100.0) if params.hard_stop_pct > 0.0 else fast_ma * 0.98
                    pos = {"side": "LONG", "entry_price": close, "entry_idx": i,
                           "hard_stop_price": hard_stop, "max_profit": 0.0}
            elif fast_ma < slow_ma:
                if prev_close > prev_fast_ma and close < fast_ma:
                    hard_stop = close * (1.0 + params.hard_stop_pct / 100.0) if params.hard_stop_pct > 0.0 else fast_ma * 1.02
                    pos = {"side": "SHORT", "entry_price": close, "entry_idx": i,
                           "hard_stop_price": hard_stop, "max_profit": 0.0}

    # 末尾仍持仓
    if pos is not None:
        entry = pos["entry_price"]
        side = pos["side"]
        exit_price = closes[-1]
        ret = (exit_price - entry) / entry if side == "LONG" else (entry - exit_price) / entry
        trades.append({
            "symbol": symbol, "side": "多" if side == "LONG" else "空",
            "ret": ret, "ret_pct": ret * 100.0, "reason": "持仓到结束",
            "bars": n - 1 - pos["entry_idx"],
        })

    return trades


def fmt_reasons(m: Dict[str, Any]) -> str:
    rc = m.get("reason_cnt", {})
    order = ["硬止损", "MA288止损", "移动止盈", "趋势反转", "持仓到结束"]
    return "/".join(str(rc.get(k, 0)) for k in order)


def main() -> int:
    configs = [
        ("Python旧版(反转ON)", True, True),
        ("Python当前(反转OFF)", True, False),
        ("JS第十六次实际", False, False),
    ]

    # 逐币种明细
    print("=" * 118)
    print("反转出场开关对比（同数据 / 同参数，未计手续费滑点）")
    print("=" * 118)

    per_coin: Dict[str, Dict[str, Dict[str, Any]]] = {}

    for symbol in dc.SYMBOLS:
        params = dc.SYMBOL_PARAMS[symbol]
        bars = load_klines_30m(symbol)
        per_coin[symbol] = {}
        print(f"\n### {symbol}  (params: hard_stop={params.hard_stop_pct}, act={params.trailing_activate_pct}, cb={params.trailing_callback_pct}, bars={len(bars)})")
        print(f"{'口径':22} {'笔数':>5} {'胜率':>7} {'简单收益':>10} {'复利收益':>10} {'最大回撤':>9} {'盈亏比':>6} {'利润因子':>8}  {'平仓原因(硬/MA288/止盈/反转/结束)':>34}")
        print("-" * 118)
        for name, use_ma288, use_rev in configs:
            trades = backtest_toggle(symbol, params, bars, use_ma288_stop=use_ma288, use_reversal=use_rev)
            m = bt.compute_metrics(trades)
            per_coin[symbol][name] = m
            pr = f"{m['payoff_ratio']:.2f}" if m['payoff_ratio'] != float('inf') else "∞"
            pf = f"{m['profit_factor']:.2f}" if m['profit_factor'] != float('inf') else "∞"
            print(f"{name:22} {m['n']:>5} {m['win_rate']:>6.1f}% {m['total_ret']:>+10.2f}% {m['compound_ret']:>+10.2f}% "
                  f"{m['max_drawdown']:>8.2f}% {pr:>6} {pf:>8}  {fmt_reasons(m):>34}")

    # 反转出场 ON vs OFF 的差值（核心问题）
    print("\n" + "=" * 118)
    print("核心对比：趋势反转出场 ON(旧版) vs OFF(当前) —— 收益差值")
    print("=" * 118)
    print(f"{'币种':12} {'简单收益差':>12} {'复利收益差':>12} {'笔数差':>7} {'最大回撤差':>11} {'反转出场笔数(ON)':>16}")
    print("-" * 118)
    sum_simple_on = sum_compound_on = 0.0
    sum_simple_off = sum_compound_off = 0.0
    total_rev = 0
    for symbol in dc.SYMBOLS:
        on = per_coin[symbol]["Python旧版(反转ON)"]
        off = per_coin[symbol]["Python当前(反转OFF)"]
        d_simple = on["total_ret"] - off["total_ret"]
        d_compound = on["compound_ret"] - off["compound_ret"]
        d_n = on["n"] - off["n"]
        d_dd = on["max_drawdown"] - off["max_drawdown"]
        n_rev = on["reason_cnt"].get("趋势反转", 0)
        total_rev += n_rev
        sum_simple_on += on["total_ret"]
        sum_simple_off += off["total_ret"]
        sum_compound_on += on["compound_ret"]
        sum_compound_off += off["compound_ret"]
        print(f"{symbol:12} {d_simple:>+12.2f}% {d_compound:>+12.2f}% {d_n:>+7} {d_dd:>+11.2f}% {n_rev:>16}")

    print("-" * 118)
    print(f"{'六币合计':12} {sum_simple_on - sum_simple_off:>+12.2f}% {sum_compound_on - sum_compound_off:>+12.2f}% "
          f"{'':>7} {'':>11} {total_rev:>16}")

    # JS 口径 vs Python 当前 的差值（辅助观察：MA288止损 的影响）
    print("\n" + "=" * 118)
    print("辅助对比：JS第十六次(无MA288止损) vs Python当前(含MA288止损) —— 揭示 MA288止损 的影响")
    print("=" * 118)
    print(f"{'币种':12} {'简单收益差':>12} {'复利收益差':>12} {'笔数差':>7}")
    print("-" * 118)
    for symbol in dc.SYMBOLS:
        js = per_coin[symbol]["JS第十六次实际"]
        py = per_coin[symbol]["Python当前(反转OFF)"]
        print(f"{symbol:12} {js['total_ret'] - py['total_ret']:>+12.2f}% "
              f"{js['compound_ret'] - py['compound_ret']:>+12.2f}% {js['n'] - py['n']:>+7}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
