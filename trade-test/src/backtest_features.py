"""带监控指标快照的回测（与 backtest.py 的交易逻辑完全一致）。

在 backtest.py 的基础上，为每一笔交易额外记录：
  - 入场时刻的 27 个多维度监控指标快照（indicators.IndicatorSet.snapshot）
  - 出场时刻的同一批指标快照
  - 持仓路径指标：MFE（最大浮盈）、MAE（最大浮亏）、是否触发移动止盈激活线、持仓 bar 数

输出（供 analyze_features.py 分析）：
  - feature_report/trade_features.json   —— 结构化的逐笔交易+指标
  - feature_report/trade_features.csv    —— 平铺成列的 CSV（便于外部查看）

运行：cd F:/rust-projects/trade-test/src && python backtest_features.py
"""
from __future__ import annotations

import csv
import json
import os
from typing import List, Dict, Any, Optional

import backtest as bt
import ma_trend_pullback as strat
import data_config as dc
from loader import load_klines_30m, fmt_time
from indicators import IndicatorSet, FAST, SLOW

SRC_DIR = os.path.dirname(os.path.abspath(__file__))
OUT_DIR = os.path.join(SRC_DIR, "feature_report")


def run_backtest(symbol: str, params: strat.Params, bars, ind: IndicatorSet) -> List[Dict[str, Any]]:
    """与 backtest.backtest 逻辑一致，额外打指标快照并记录 MFE/MAE。"""
    n = len(bars)
    closes = ind.closes
    fast_ma = ind.sma_fast
    slow_ma = ind.sma_slow

    trades: List[Dict[str, Any]] = []
    pos: Optional[Dict[str, Any]] = None

    for i in range(n):
        if i + 1 < SLOW:
            continue

        close = closes[i]
        prev_close = closes[i - 1]
        fma = fast_ma[i]
        sma = slow_ma[i]
        prev_fma = fast_ma[i - 1]

        # ---- 持仓中：平仓（硬止损 > MA288止损 > 移动止盈 > 趋势反转） ----
        if pos is not None:
            bar = bars[i]
            side = pos["side"]
            entry = pos["entry_price"]
            pnl = (close - entry) / entry * 100.0 if side == "LONG" else (entry - close) / entry * 100.0
            pos["max_profit"] = max(pos["max_profit"], pnl)
            pos["min_profit"] = min(pos["min_profit"], pnl)

            exit_price: Optional[float] = None
            reason = ""

            if params.hard_stop_pct > 0.0:
                if side == "LONG" and bar.low <= pos["hard_stop_price"]:
                    exit_price, reason = pos["hard_stop_price"], "硬止损"
                elif side == "SHORT" and bar.high >= pos["hard_stop_price"]:
                    exit_price, reason = pos["hard_stop_price"], "硬止损"

            if exit_price is None and params.stop_mode == "ma288" and prev_fma is not None:
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
                trades.append(_make_trade(
                    symbol, side, pos, bars, ind, i,
                    entry_price=entry, exit_price=exit_price,
                    ret=ret, reason=reason,
                ))
                pos = None
                continue

        # ---- 无持仓：入场 ----
        if pos is None and fma is not None and sma is not None and prev_fma is not None:
            if fma > sma:
                if prev_close < prev_fma and close > fma:
                    hard_stop = close * (1.0 - params.hard_stop_pct / 100.0) if params.hard_stop_pct > 0.0 else fma * 0.98
                    pos = {
                        "side": "LONG", "entry_price": close,
                        "entry_time": bars[i].open_time, "entry_idx": i,
                        "hard_stop_price": hard_stop,
                        "max_profit": 0.0, "min_profit": 0.0,
                        "entry_snapshot": ind.snapshot(i),
                    }
            elif fma < sma:
                if prev_close > prev_fma and close < fma:
                    hard_stop = close * (1.0 + params.hard_stop_pct / 100.0) if params.hard_stop_pct > 0.0 else fma * 1.02
                    pos = {
                        "side": "SHORT", "entry_price": close,
                        "entry_time": bars[i].open_time, "entry_idx": i,
                        "hard_stop_price": hard_stop,
                        "max_profit": 0.0, "min_profit": 0.0,
                        "entry_snapshot": ind.snapshot(i),
                    }

    # 末尾仍持仓
    if pos is not None:
        entry = pos["entry_price"]
        side = pos["side"]
        exit_price = closes[-1]
        ret = (exit_price - entry) / entry if side == "LONG" else (entry - exit_price) / entry
        trades.append(_make_trade(
            symbol, side, pos, bars, ind, n - 1,
            entry_price=entry, exit_price=exit_price,
            ret=ret, reason="持仓到结束",
        ))

    return trades


def _make_trade(symbol, side, pos, bars, ind, exit_idx, entry_price, exit_price, ret, reason):
    entry_idx = pos["entry_idx"]
    return {
        "symbol": symbol,
        "side": "多" if side == "LONG" else "空",
        "entry_time": fmt_time(pos["entry_time"]),
        "exit_time": fmt_time(bars[exit_idx].open_time),
        "entry_price": entry_price,
        "exit_price": exit_price,
        "ret": ret,
        "ret_pct": ret * 100.0,
        "reason": reason,
        "bars": exit_idx - entry_idx,
        "mfe_pct": pos["max_profit"],
        "mae_pct": pos["min_profit"],
        "entry_year": fmt_time(pos["entry_time"])[:4],
        "entry": pos["entry_snapshot"],
        "exit": ind.snapshot(exit_idx),
    }


def _flatten(trade: Dict[str, Any]) -> Dict[str, Any]:
    row = {k: v for k, v in trade.items() if k not in ("entry", "exit")}
    for k, v in trade["entry"].items():
        row[f"entry_{k}"] = v
    for k, v in trade["exit"].items():
        row[f"exit_{k}"] = v
    return row


def main() -> int:
    os.makedirs(OUT_DIR, exist_ok=True)
    all_trades: List[Dict[str, Any]] = []

    print("=" * 100)
    print("带指标快照的回测（与基线 backtest.py 逻辑一致）")
    print("=" * 100)
    print(f"{'币种':10} {'交易数':>6} {'总收益(简单)%':>13}  基线交易数 / 基线总收益")
    print("-" * 100)

    for symbol in dc.SYMBOLS:
        params = dc.SYMBOL_PARAMS[symbol]
        bars = load_klines_30m(symbol)
        ind = IndicatorSet(bars)
        trades = run_backtest(symbol, params, bars, ind)
        total = sum(t["ret_pct"] for t in trades)
        all_trades.extend(trades)
        # 基线参照
        base_trades = bt.backtest(symbol, params, bars)
        base_total = sum(t["ret_pct"] for t in base_trades)
        print(f"{symbol:10} {len(trades):>6} {total:>12.2f}%   {len(base_trades)} / {base_total:.2f}%")

    # JSON（结构化）
    json_path = os.path.join(OUT_DIR, "trade_features.json")
    with open(json_path, "w", encoding="utf-8") as f:
        json.dump(all_trades, f, ensure_ascii=False)

    # CSV（平铺）
    csv_path = os.path.join(OUT_DIR, "trade_features.csv")
    flat = [_flatten(t) for t in all_trades]
    if flat:
        cols = list(flat[0].keys())
        with open(csv_path, "w", encoding="utf-8", newline="") as f:
            w = csv.DictWriter(f, fieldnames=cols)
            w.writeheader()
            for row in flat:
                w.writerow(row)

    print()
    print(f"[written] {json_path}")
    print(f"[written] {csv_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
