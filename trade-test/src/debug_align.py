"""调试：确认生产策略服务到底用哪种 K 线窗口（forming bar vs 最后已收盘 bar）。

通过对比重算的 MA288/MA488 与信号 market_context 的误差，判定哪种对齐方式是"精确复现"。
若某种对齐下误差接近 0，则说明窗口正确；随后打印该窗口下的穿越判定输入，用于核实
crossover 结论是否为真。
"""
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import ma_trend_pullback as strat
from reproduce_test import (
    load_prod_signals, load_klines_30m, parse_iso_time, find_window,
    KLINE_COUNT,
)

def sma_diff_for_window(bars, end_idx, fast, slow, snapshot):
    """用 500 根窗口（末根 close 替换为 snapshot current_price）重算 MA，返回 (fast, slow, window)。"""
    start = max(0, end_idx - KLINE_COUNT + 1)
    window = [strat.KlineBar(**vars(b)) for b in bars[start:end_idx + 1]]
    if snapshot and window:
        window[-1].open = snapshot["open"]
        window[-1].close = snapshot["current_price"]
    f = strat.calculate_sma(window, fast)
    s = strat.calculate_sma(window, slow)
    return f, s, window

def main():
    signals = load_prod_signals()
    bars_by = {s: load_klines_30m(s) for s in strat.SYMBOL_PARAMS}

    print(f"{'symbol':8} {'created_at':24} {'forming_fast_err':>17} {'completed_fast_err':>19} "
          f"{'best':6}  {('forming慢err'):>14} {('completed慢err'):>16}")
    print("-" * 120)

    for sig in signals:
        sym = sig["symbol"]
        mc = sig["_market_context"]
        ts = parse_iso_time(sig["created_at"])
        bars = bars_by[sym]
        params = strat.SYMBOL_PARAMS[sym]
        prod_fast = float(mc["fast_ma"])
        prod_slow = float(mc["slow_ma"])

        forming_idx, completed_idx = find_window(bars, ts)
        snap = {"open": float(mc["open"]), "current_price": float(mc["current_price"])}

        f_form = f_comp = s_form = s_comp = None
        if forming_idx is not None:
            f_form, s_form, _ = sma_diff_for_window(bars, forming_idx, params.fast_ma_period, params.slow_ma_period, snap)
        if completed_idx is not None:
            f_comp, s_comp, _ = sma_diff_for_window(bars, completed_idx, params.fast_ma_period, params.slow_ma_period, snap)

        ef = (f_form - prod_fast) if f_form is not None else None
        ec = (f_comp - prod_fast) if f_comp is not None else None
        es = (s_form - prod_slow) if s_form is not None else None
        esc = (s_comp - prod_slow) if s_comp is not None else None

        best = "form" if (abs(ef) if ef is not None else 1e9) <= (abs(ec) if ec is not None else 1e9) else "comp"
        print(f"{sym:8} {sig['created_at']:24} {ef if ef is None else round(ef,6):>17} "
              f"{ec if ec is None else round(ec,6):>19} {best:6}  "
              f"{es if es is None else round(es,6):>14} {esc if esc is None else round(esc,6):>16}")

    # 对每一条信号，打印"最优对齐"下的穿越判定输入
    print("\n" + "=" * 120)
    print("最优对齐下的穿越判定输入（用于核实 crossover 结论）")
    print("=" * 120)
    for sig in signals:
        sym = sig["symbol"]
        mc = sig["_market_context"]
        ts = parse_iso_time(sig["created_at"])
        bars = bars_by[sym]
        params = strat.SYMBOL_PARAMS[sym]
        forming_idx, completed_idx = find_window(bars, ts)
        snap = {"open": float(mc["open"]), "current_price": float(mc["current_price"])}
        prod_fast = float(mc["fast_ma"])

        # 选择误差更小的对齐
        best_idx = None
        best_err = 1e9
        for idx in (forming_idx, completed_idx):
            if idx is None:
                continue
            f, _, _ = sma_diff_for_window(bars, idx, params.fast_ma_period, params.slow_ma_period, snap)
            err = abs(f - prod_fast) if f is not None else 1e9
            if err < best_err:
                best_err = err
                best_idx = idx
        if best_idx is None:
            continue
        f, s, window = sma_diff_for_window(bars, best_idx, params.fast_ma_period, params.slow_ma_period, snap)
        prev_fast = strat.calculate_sma(window[:-1], params.fast_ma_period)
        prev_close = window[-2].close
        close = window[-1].close
        trend = "Bullish" if f > s else "Bearish"
        if trend == "Bullish":
            crossed = prev_close < prev_fast and close > f
        else:
            crossed = prev_close > prev_fast and close < f
        print(f"\n{sym} {sig['created_at']}  signal={sig['signal_type']}  best_idx={best_idx}({bars[best_idx].open_time if best_idx else '-'})")
        print(f"  trend={trend}  fast={f:.6f}  slow={s:.6f}")
        print(f"  prev_close={prev_close:.6f}  prev_fast_ma={prev_fast:.6f}  (prev_close > prev_fast? {prev_close > prev_fast})")
        print(f"  close={close:.6f}  entry_fast_ma={f:.6f}  (close {'>' if close>f else '<'} fast? {close>f})")
        print(f"  => 复刻穿越={crossed}   生产信号={sig['signal_type']}   一致={crossed and ((trend=='Bullish' and sig['signal_type']=='BUY') or (trend=='Bearish' and sig['signal_type']=='SELL'))}")

if __name__ == "__main__":
    main()
