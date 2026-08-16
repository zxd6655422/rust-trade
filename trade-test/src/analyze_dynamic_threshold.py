"""动态 realized_vol_threshold 可行性诊断。

思路：当前阈值是每币固定值。诊断 vol 过滤在不同「市场状态」下的效果是否一致：
  若某状态下 vol>=阈值的交易特别亏（过滤有效）、另一状态下不亏/赚（过滤有害），
  则动态调整阈值有空间。

市场状态维度（入场时）：
  - adx14（趋势强度）：低=震荡，高=趋势
  - interweave_bars_96（均线交织根数）：高=震荡
  - mean_spread_96（均线分离均值）：小=交织，大=趋势
  - position_in_range_96（区间位置）：低=底部，高=顶部

对每个维度分桶，看每个桶内「vol>=固定阈值（会被过滤）」交易的平均收益。
口径：无过滤回测（对齐生产退出链），vol 阈值用每币固定值。

输出：feature_report/dynamic_threshold_report.md
"""
from __future__ import annotations

import os
from typing import List, Dict, Any, Optional

import data_config as dc
from loader import load_klines_30m
from indicators import IndicatorSet, FAST, SLOW


def backtest_no_filter(symbol, params, bars, ind):
    """无 vol 过滤回测（对齐生产退出链），记录入场指标快照。"""
    n = len(bars)
    closes = ind.closes
    fast_ma = ind.sma_fast
    slow_ma = ind.sma_slow

    trades = []
    pos = None
    for i in range(n):
        if i + 1 < SLOW:
            continue
        close = closes[i]
        prev_close = closes[i - 1]
        fma = fast_ma[i]
        sma = slow_ma[i]
        prev_fma = fast_ma[i - 1]

        if pos is not None:
            bar = bars[i]
            side = pos["side"]
            entry = pos["entry_price"]
            pnl = (close - entry) / entry * 100.0 if side == "LONG" else (entry - close) / entry * 100.0
            pos["max_profit"] = max(pos["max_profit"], pnl)
            exit_price = None
            reason = ""
            if params.hard_stop_pct > 0.0:
                if side == "LONG" and bar.low <= pos["hard_stop_price"]:
                    exit_price, reason = pos["hard_stop_price"], "硬止损"
                elif side == "SHORT" and bar.high >= pos["hard_stop_price"]:
                    exit_price, reason = pos["hard_stop_price"], "硬止损"
            if exit_price is None and prev_fma is not None:
                if side == "LONG" and prev_close > prev_fma and close < fma:
                    exit_price, reason = close, "MA288止损"
                elif side == "SHORT" and prev_close < prev_fma and close > fma:
                    exit_price, reason = close, "MA288止损"
            if exit_price is None and params.take_profit_mode == "trailing":
                if pos["max_profit"] >= params.trailing_activate_pct:
                    if pos["max_profit"] - pnl >= params.trailing_callback_pct:
                        exit_price, reason = close, "移动止盈"
            if exit_price is None:
                if side == "LONG" and fma < sma:
                    exit_price, reason = close, "趋势反转"
                elif side == "SHORT" and fma > sma:
                    exit_price, reason = close, "趋势反转"
            if exit_price is not None:
                ret = (exit_price - entry) / entry if side == "LONG" else (entry - exit_price) / entry
                ei = pos["entry_idx"]
                trades.append({"ret_pct": ret * 100.0, "reason": reason, "snap": ind.snapshot(ei)})
                pos = None
                continue

        if pos is None and fma is not None and sma is not None and prev_fma is not None:
            if fma > sma and prev_close < prev_fma and close > fma:
                hs = close * (1.0 - params.hard_stop_pct / 100.0)
                pos = {"side": "LONG", "entry_price": close, "entry_idx": i, "hard_stop_price": hs, "max_profit": 0.0}
            elif fma < sma and prev_close > prev_fma and close < fma:
                hs = close * (1.0 + params.hard_stop_pct / 100.0)
                pos = {"side": "SHORT", "entry_price": close, "entry_idx": i, "hard_stop_price": hs, "max_profit": 0.0}

    if pos is not None:
        ret = (closes[-1] - pos["entry_price"]) / pos["entry_price"] if pos["side"] == "LONG" else (pos["entry_price"] - closes[-1]) / pos["entry_price"]
        ei = pos["entry_idx"]
        trades.append({"ret_pct": ret * 100.0, "reason": "持仓到结束", "snap": ind.snapshot(ei)})
    return trades


def analyze_dim(ts, feat, thr, bins, add, label):
    add(f"### {label}")
    add("")
    add("| 状态 | 总笔数 | vol≥阈值笔数 | vol≥阈值平均收益 | vol<阈值笔数 | vol<阈值平均收益 | 过滤净效果 |")
    add("|---|---|---|---|---|---|---|")
    for lo, hi, lab in bins:
        b = [t for t in ts if t["snap"].get(feat) is not None and lo <= t["snap"][feat] < hi]
        if not b:
            continue
        hi_vol = [t for t in b if t["snap"].get("realized_vol_48") is not None and t["snap"]["realized_vol_48"] >= thr]
        lo_vol = [t for t in b if t["snap"].get("realized_vol_48") is not None and t["snap"]["realized_vol_48"] < thr]
        if not hi_vol:
            continue
        h_avg = sum(t["ret_pct"] for t in hi_vol) / len(hi_vol)
        l_avg = sum(t["ret_pct"] for t in lo_vol) / len(lo_vol) if lo_vol else 0.0
        # 过滤净效果 = 若过滤掉 hi_vol，省下的亏损 = -h_avg（负 h_avg 表示这些交易在亏，过滤有效）
        add(f"| {lab} | {len(b)} | {len(hi_vol)} | {h_avg:+.3f}% | {len(lo_vol)} | {l_avg:+.3f}% | "
            f"{'✅有效' if h_avg < -0.05 else ('❌有害' if h_avg > 0.05 else '中性')} |")
    add("")


def main() -> int:
    md: List[str] = []
    add = md.append
    add("# realized_vol_threshold 动态调整可行性诊断")
    add("")
    add("- 看「vol>=固定阈值（会被过滤掉）」的交易，在不同市场状态下的平均收益。")
    add("- 若某状态 vol 高交易平均收益很负 → 该状态应过滤；若为正 → 该状态不应过滤（动态阈值有空间）。")
    add("")

    for coin in ["BTCUSDT", "ETHUSDT"]:
        params = dc.SYMBOL_PARAMS[coin]
        bars = load_klines_30m(coin)
        ind = IndicatorSet(bars)
        thr = params.realized_vol_threshold
        ts = backtest_no_filter(coin, params, bars, ind)

        add(f"## {coin}  （无过滤 {len(ts)} 笔，固定阈值 {thr}）")
        add("")

        analyze_dim(ts, "adx14", thr,
                    [(0, 25, "震荡(ADX<25)"), (25, 35, "中性(25~35)"), (35, 1e9, "趋势(ADX>35)")],
                    add, "1. 按趋势强度 ADX14")

        analyze_dim(ts, "interweave_bars_96", thr,
                    [(0, 10, "趋势(交织<10)"), (10, 30, "中性(10~30)"), (30, 1e9, "震荡(交织>30)")],
                    add, "2. 按均线交织 interweave_bars_96")

        analyze_dim(ts, "mean_spread_96", thr,
                    [(0, 1.0, "交织(分离<1%)"), (1.0, 2.0, "中性(1~2%)"), (2.0, 1e9, "趋势(分离>2%)")],
                    add, "3. 按均线分离 mean_spread_96")

        analyze_dim(ts, "position_in_range_96", thr,
                    [(0, 40, "底部(位置<40)"), (40, 60, "中部(40~60)"), (60, 100, "顶部(位置>60)")],
                    add, "4. 按区间位置 position_in_range_96")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "dynamic_threshold_report.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
