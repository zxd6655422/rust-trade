"""三段式分级止盈：移动止盈 → MA192 → MA480（完整融合 A1+A7+A9）。

- 段1（max_profit < switch1）：移动止盈 activate_small+callback_small（高敏锁小利，同 A1）
- 段2（switch1 <= max_profit < switch2）：MA192 锁利（中等趋势，同 A7）
- 段3（max_profit >= switch2）：MA480 长拿 + 衰竭降级 demote_pct 降回 MA192（同 A9）

扫描 switch1 × switch2 两个切换阈值。
口径：slow=480 + vol过滤 + 硬止损→MA288止损→三段式止盈→趋势反转。

输出：feature_report/tiered_hybrid_report.md
"""
from __future__ import annotations

import os
from datetime import datetime
from itertools import product

import data_config as dc
from loader import load_klines_30m
from study_adaptive_ma_trailing import precompute, comp


def backtest_tiered3(symbol, params, bars, pre, switch1, switch2,
                     activate_small=4.0, callback_small=1.5, confirm=10, demote_pct=10.0,
                     y0=None, y1=None):
    fast = params.fast_ma_period
    slow = params.slow_ma_period
    n = len(bars)
    years = [datetime.fromtimestamp(b.open_time / 1000).year for b in bars]
    closes = pre["closes"]
    vol48 = pre["vol48"]
    ma192s = pre["ma192"]
    ma480s = pre["ma480"]
    prefix = pre["prefix"]

    def sma_at(idx, period):
        if idx + 1 < period:
            return None
        return (prefix[idx + 1] - prefix[idx + 1 - period]) / period

    trades = []
    pos = None
    for i in range(n):
        if i + 1 < slow:
            continue
        if y0 is not None and not (y0 <= years[i] <= y1):
            continue
        close = closes[i]
        prev_close = closes[i - 1]
        fast_ma = sma_at(i, fast)
        slow_ma = sma_at(i, slow)
        prev_fast_ma = sma_at(i - 1, fast)

        if pos is not None:
            bar = bars[i]
            side = pos["side"]
            entry = pos["entry"]
            pnl = (close - entry) / entry * 100.0 if side == "LONG" else (entry - close) / entry * 100.0
            pos["max_profit"] = max(pos["max_profit"], pnl)
            pos["hold_bars"] += 1

            use_ma288_stop = True
            tp = None
            exit_price = None
            reason = ""

            if pos.get("demoted"):
                use_ma288_stop, tp = False, 192  # 已降级：MA192 锁利
            elif pos["max_profit"] >= switch2:
                # 段3 大单：MA480 长拿 + 衰竭降级
                profit_decline = pos["max_profit"] - pnl
                if profit_decline >= demote_pct and pnl < pos["max_profit"] - demote_pct:
                    pos["demoted"] = True
                    use_ma288_stop, tp = True, 192
                else:
                    use_ma288_stop, tp = False, 480
            elif pos["max_profit"] >= switch1:
                # 段2 中单：MA192 锁利
                use_ma288_stop, tp = True, 192
            else:
                # 段1 小单：移动止盈（高敏）
                use_ma288_stop, tp = True, None
                if pos["max_profit"] >= activate_small and pos["max_profit"] - pnl >= callback_small:
                    exit_price, reason = close, "移动止盈"

            # 1. 硬止损
            if exit_price is None and params.hard_stop_pct > 0.0:
                if side == "LONG" and bar.low <= pos["hard_stop"]:
                    exit_price, reason = pos["hard_stop"], "硬止损"
                elif side == "SHORT" and bar.high >= pos["hard_stop"]:
                    exit_price, reason = pos["hard_stop"], "硬止损"
            # 2. MA288 止损
            if exit_price is None and use_ma288_stop and prev_fast_ma is not None:
                if side == "LONG" and prev_close > prev_fast_ma and close < fast_ma:
                    exit_price, reason = close, "MA288止损"
                elif side == "SHORT" and prev_close < prev_fast_ma and close > fast_ma:
                    exit_price, reason = close, "MA288止损"
            # 3. MA 止盈线（段2/段3/降级后）
            if exit_price is None and tp is not None:
                ma_v = (ma192s if tp == 192 else ma480s)[i]
                if ma_v is not None:
                    below = (side == "LONG" and close < ma_v) or (side == "SHORT" and close > ma_v)
                    pos["below_count"] = pos["below_count"] + 1 if below else 0
                    if pos["below_count"] >= confirm:
                        exit_price, reason = close, f"MA{tp}止盈"
            # 4. 趋势反转
            if exit_price is None:
                if side == "LONG" and fast_ma < slow_ma:
                    exit_price, reason = close, "趋势反转"
                elif side == "SHORT" and fast_ma > slow_ma:
                    exit_price, reason = close, "趋势反转"

            if exit_price is not None:
                ret = (exit_price - entry) / entry if side == "LONG" else (entry - exit_price) / entry
                trades.append({"ret_pct": ret * 100.0, "reason": reason,
                               "entry_idx": pos["entry_idx"], "exit_idx": i,
                               "mfe_pct": pos["max_profit"], "hold_bars": pos["hold_bars"]})
                pos = None
                continue

        if pos is None and fast_ma is not None and slow_ma is not None and prev_fast_ma is not None:
            if vol48[i] is not None and vol48[i] >= params.realized_vol_threshold:
                continue
            if fast_ma > slow_ma and prev_close < prev_fast_ma and close > fast_ma:
                hs = close * (1.0 - params.hard_stop_pct / 100.0)
                pos = {"side": "LONG", "entry": close, "entry_idx": i, "hard_stop": hs,
                       "max_profit": 0.0, "hold_bars": 0, "below_count": 0, "demoted": False}
            elif fast_ma < slow_ma and prev_close > prev_fast_ma and close < fast_ma:
                hs = close * (1.0 + params.hard_stop_pct / 100.0)
                pos = {"side": "SHORT", "entry": close, "entry_idx": i, "hard_stop": hs,
                       "max_profit": 0.0, "hold_bars": 0, "below_count": 0, "demoted": False}

    if pos is not None:
        ret = (closes[-1] - pos["entry"]) / pos["entry"] if pos["side"] == "LONG" else (pos["entry"] - closes[-1]) / pos["entry"]
        trades.append({"ret_pct": ret * 100.0, "reason": "持仓到结束",
                       "entry_idx": pos["entry_idx"], "exit_idx": n - 1, "mfe_pct": pos["max_profit"], "hold_bars": pos["hold_bars"]})
    return trades


def main() -> int:
    md = []
    add = md.append
    add("# 三段式分级止盈：移动止盈 → MA192 → MA480")
    add("")
    add("- 段1(<switch1)：移动止盈 4+1.5；段2(switch1~switch2)：MA192 c10；段3(≥switch2)：MA480 c10+衰竭降级10%。")
    add("")

    SWITCH1 = [4.0, 6.0, 8.0]
    SWITCH2 = [12.0, 15.0, 20.0, 25.0]

    for coin in ["BTCUSDT", "ETHUSDT", "SOLUSDT"]:
        params = dc.SYMBOL_PARAMS[coin]
        bars = load_klines_30m(coin)
        pre = precompute(bars)
        years = [datetime.fromtimestamp(b.open_time / 1000).year for b in bars]
        y0, y1 = min(years), max(years)
        mid = (y0 + y1) // 2

        add(f"## {coin}")
        add("")
        grid = []
        for s1, s2 in product(SWITCH1, SWITCH2):
            if s2 <= s1:
                continue
            trades = backtest_tiered3(coin, params, bars, pre, s1, s2)
            c = comp([t["ret_pct"] for t in trades])
            grid.append((s1, s2, c, len(trades)))
        grid.sort(key=lambda x: -x[2])
        add("| switch1 | switch2 | 复利 | 笔数 |")
        add("|---|---|---|---|")
        for s1, s2, c, n in grid:
            add(f"| {s1}% | {s2}% | {c:+.1f}% | {n} |")
        add("")

        # 时间切分验证最优
        best = grid[0]
        s1, s2 = best[0], best[1]
        val = comp([t["ret_pct"] for t in backtest_tiered3(coin, params, bars, pre, s1, s2, y0=mid+1, y1=y1)])
        val_base = comp([t["ret_pct"] for t in backtest_tiered3(coin, params, bars, pre, 999, 999, y0=mid+1, y1=y1)])
        add(f"**时间切分（训练 {y0}-{mid} → 验证 {mid+1}-{y1}）：最优 switch1={s1}%/switch2={s2}% → 验证段 {val:+.1f}% vs 基线 {val_base:+.1f}%**")
        add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "tiered_hybrid_report.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
