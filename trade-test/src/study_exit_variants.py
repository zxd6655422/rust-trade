"""止盈方案对比：MA480 分级长拿 + 延迟止盈 + 回撤离场 等。

背景：A7 用 MA192 动态止盈大幅跑赢基线；但 slow 均线 MA480 目前只用于趋势方向与
「趋势反转」出场，从未被当作止盈线。本脚本对比多种「拿更大利润」的止盈方案：
  1. 分级长拿（用户想法）：盈利达到阈值后，把止盈线从 MA192 换成更慢的 MA480，长拿大单。
  2. 延迟止盈：MA192/MA288 上增加 confirm 根数（延迟确认离场）。
  3. 回撤离场：从持仓最高价回撤 X% 才离场（百分比吊灯）。
  4. 更宽的吊灯止损（k*ATR）。
  5. 去掉 MA288 止损（避免大单被 MA288 提前打掉）。

口径：对齐生产（slow=480 + vol过滤 + 硬止损→[MA288止损]→止盈规则→趋势反转）。
  分级/MA480 的关键：一旦「晋级为长拿单」就**关闭 MA288 止损**（否则 MA288 先于 MA480 触发）。

输出：feature_report/exit_variants_report.md
"""
from __future__ import annotations

import math
import os
from datetime import datetime
from typing import List, Dict, Any, Optional

import data_config as dc
from loader import load_klines_30m


# ----------------------------------------------------------------------
# 指标序列
# ----------------------------------------------------------------------

def sma_series(values, period):
    n = len(values)
    p = [0.0] * (n + 1)
    for i in range(n):
        p[i + 1] = p[i] + values[i]
    out = [None] * n
    for i in range(period - 1, n):
        out[i] = (p[i + 1] - p[i + 1 - period]) / period
    return out


def atr_series(bars, period):
    n = len(bars)
    tr = [0.0] * n
    for i in range(n):
        h = bars[i].high
        l = bars[i].low
        pc = bars[i - 1].close if i > 0 else bars[i].close
        tr[i] = max(h - l, abs(h - pc), abs(l - pc))
    return sma_series(tr, period)


def realized_vol_48_series(closes):
    n = len(closes)
    rets = [0.0] * n
    for i in range(1, n):
        if closes[i - 1] != 0.0:
            rets[i] = closes[i] / closes[i - 1] - 1.0
    p = [0.0] * (n + 1)
    p2 = [0.0] * (n + 1)
    for i in range(n):
        p[i + 1] = p[i] + rets[i]
        p2[i + 1] = p2[i] + rets[i] * rets[i]
    W = 48
    out = [None] * n
    for i in range(W, n):
        mean = (p[i + 1] - p[i + 1 - W]) / W
        msq = (p2[i + 1] - p2[i + 1 - W]) / W
        var = msq - mean * mean
        if var < 0.0:
            var = 0.0
        out[i] = math.sqrt(var) * 100.0
    return out


def precompute(bars):
    closes = [b.close for b in bars]
    highs = [b.high for b in bars]
    lows = [b.low for b in bars]
    n = len(bars)
    prefix = [0.0] * (n + 1)
    for i in range(n):
        prefix[i + 1] = prefix[i] + closes[i]
    return {
        "closes": closes, "highs": highs, "lows": lows, "prefix": prefix,
        "vol48": realized_vol_48_series(closes),
        "ma192": sma_series(closes, 192),
        "ma288": sma_series(closes, 288),
        "ma480": sma_series(closes, 480),
        "atr": atr_series(bars, 14),
    }


# ----------------------------------------------------------------------
# 回测（可配置止盈模式），返回逐笔交易
# ----------------------------------------------------------------------

MA_SERIES_KEY = {192: "ma192", 288: "ma288", 480: "ma480"}


def exit_config(mode, max_profit, activate, switch_at):
    """返回 (MA288止损是否启用, 止盈线周期 or None)。基于 max_profit 锁存（不因回撤降级）。"""
    if mode == "base":
        return (True, None)
    if mode == "ma192":
        return (True, 192)
    if mode == "ma192_nostop":
        return (False, 192)
    if mode == "ma288":
        return (False, 288)
    if mode == "ma480":
        # 达到 activate 前靠 MA288 止损保护；达到后关闭 MA288 止损、改用 MA480
        return (max_profit < activate, 480)
    if mode == "tiered":
        # 达到 switch_at 前：MA192 + MA288止损；达到后：关闭 MA288止损、改用 MA480
        return (False, 480) if max_profit >= switch_at else (True, 192)
    if mode == "pct":
        return (True, None)
    if mode == "chandelier":
        return (True, None)
    return (True, None)


def backtest_trades(symbol, params, bars, pre, mode, activate=15.0, confirm=10,
                    switch_at=30.0, trail_pct=20.0, atr_k=3.0, y0=None, y1=None):
    fast = params.fast_ma_period
    slow = params.slow_ma_period
    n = len(bars)
    years = [datetime.fromtimestamp(b.open_time / 1000).year for b in bars]
    closes = pre["closes"]
    highs = pre["highs"]
    lows = pre["lows"]
    vol48 = pre["vol48"]
    ma192 = pre["ma192"]
    ma288s = pre["ma288"]
    ma480s = pre["ma480"]
    atr = pre["atr"]
    prefix = pre["prefix"]

    def sma_at(idx, period):
        if idx + 1 < period:
            return None
        return (prefix[idx + 1] - prefix[idx + 1 - period]) / period

    trades: List[Dict[str, Any]] = []
    pos = None
    last_in = None
    for i in range(n):
        if i + 1 < slow:
            continue
        if y0 is not None and not (y0 <= years[i] <= y1):
            continue
        last_in = i
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
            fav = bar.high if side == "LONG" else bar.low
            if (side == "LONG" and fav > pos["extreme"]) or (side == "SHORT" and fav < pos["extreme"]):
                pos["extreme"] = fav
                pos["extreme_idx"] = i

            ma288_on, tp = exit_config(mode, pos["max_profit"], activate, switch_at)
            exit_price = None
            reason = ""
            # 1. 硬止损
            if params.hard_stop_pct > 0.0:
                if side == "LONG" and bar.low <= pos["hard_stop"]:
                    exit_price, reason = pos["hard_stop"], "硬止损"
                elif side == "SHORT" and bar.high >= pos["hard_stop"]:
                    exit_price, reason = pos["hard_stop"], "硬止损"
            # 2. MA288 止损
            if exit_price is None and ma288_on and prev_fast_ma is not None:
                if side == "LONG" and prev_close > prev_fast_ma and close < fast_ma:
                    exit_price, reason = close, "MA288止损"
                elif side == "SHORT" and prev_close < prev_fast_ma and close > fast_ma:
                    exit_price, reason = close, "MA288止损"
            # 3. 止盈规则
            if exit_price is None:
                if mode == "base":
                    if pos["max_profit"] >= params.trailing_activate_pct and \
                       pos["max_profit"] - pnl >= params.trailing_callback_pct:
                        exit_price, reason = close, "移动止盈"
                elif mode in ("ma192", "ma192_nostop", "ma288", "ma480", "tiered"):
                    ma_series = pre[MA_SERIES_KEY[tp]]
                    if pos["max_profit"] >= activate and ma_series[i] is not None:
                        ma_v = ma_series[i]
                        below = (side == "LONG" and close < ma_v) or (side == "SHORT" and close > ma_v)
                        if pos.get("tp") != tp:
                            pos["below_count"] = 0
                            pos["tp"] = tp
                        pos["below_count"] = pos["below_count"] + 1 if below else 0
                        if pos["below_count"] >= confirm:
                            exit_price, reason = close, f"MA{tp}止盈"
                elif mode == "pct":
                    if pos["max_profit"] >= activate:
                        dd = ((pos["extreme"] - close) if side == "LONG" else (close - pos["extreme"])) / pos["extreme"] * 100.0
                        if dd >= trail_pct:
                            exit_price, reason = close, f"回撤{trail_pct:.0f}%离场"
                elif mode == "chandelier":
                    if pos["max_profit"] >= activate and atr[i] is not None:
                        if side == "LONG" and close <= pos["extreme"] - atr_k * atr[i]:
                            exit_price, reason = close, f"吊灯k{atr_k:.0f}"
                        elif side == "SHORT" and close >= pos["extreme"] + atr_k * atr[i]:
                            exit_price, reason = close, f"吊灯k{atr_k:.0f}"
            # 4. 趋势反转
            if exit_price is None:
                if side == "LONG" and fast_ma < slow_ma:
                    exit_price, reason = close, "趋势反转"
                elif side == "SHORT" and fast_ma > slow_ma:
                    exit_price, reason = close, "趋势反转"
            if exit_price is not None:
                ret = (exit_price - entry) / entry if side == "LONG" else (entry - exit_price) / entry
                trades.append({
                    "side": side, "reason": reason, "ret_pct": ret * 100.0,
                    "entry": entry, "exit": exit_price, "entry_idx": pos["entry_idx"], "exit_idx": i,
                    "mfe_pct": pos["max_profit"], "extreme": pos["extreme"], "extreme_idx": pos["extreme_idx"],
                })
                pos = None
                continue

        if pos is None and fast_ma is not None and slow_ma is not None and prev_fast_ma is not None:
            if vol48[i] is not None and vol48[i] >= params.realized_vol_threshold:
                continue
            if fast_ma > slow_ma and prev_close < prev_fast_ma and close > fast_ma:
                pos = {"side": "LONG", "entry": close, "entry_idx": i,
                       "hard_stop": close * (1.0 - params.hard_stop_pct / 100.0),
                       "max_profit": 0.0, "extreme": bars[i].high, "extreme_idx": i,
                       "below_count": 0, "tp": None}
            elif fast_ma < slow_ma and prev_close > prev_fast_ma and close < fast_ma:
                pos = {"side": "SHORT", "entry": close, "entry_idx": i,
                       "hard_stop": close * (1.0 + params.hard_stop_pct / 100.0),
                       "max_profit": 0.0, "extreme": bars[i].low, "extreme_idx": i,
                       "below_count": 0, "tp": None}

    if pos is not None and last_in is not None:
        ret = (closes[last_in] - pos["entry"]) / pos["entry"] if pos["side"] == "LONG" else (pos["entry"] - closes[last_in]) / pos["entry"]
        trades.append({"side": pos["side"], "reason": "持仓到结束", "ret_pct": ret * 100.0,
                       "entry": pos["entry"], "exit": closes[last_in], "entry_idx": pos["entry_idx"], "exit_idx": last_in,
                       "mfe_pct": pos["max_profit"], "extreme": pos["extreme"], "extreme_idx": pos["extreme_idx"]})
    return trades


def comp(rets):
    eq = 1.0
    for r in rets:
        eq *= (1.0 + r / 100.0)
    return (eq - 1.0) * 100.0


def mean(xs):
    return sum(xs) / len(xs) if xs else 0.0


# ----------------------------------------------------------------------
# 主流程
# ----------------------------------------------------------------------

STRATS = [
    ("基线callback(生产)", "base", {}),
    ("MA192 c10(A7最优)", "ma192", {"confirm": 10}),
    ("MA192 c20(延迟)", "ma192", {"confirm": 20}),
    ("MA192 c40(更延迟)", "ma192", {"confirm": 40}),
    ("MA192去MA288止损", "ma192_nostop", {"confirm": 10}),
    ("MA288 c3", "ma288", {"confirm": 3}),
    ("MA288 c10", "ma288", {"confirm": 10}),
    ("MA480 c1", "ma480", {"confirm": 1}),
    ("MA480 c3", "ma480", {"confirm": 3}),
    ("MA480 c10", "ma480", {"confirm": 10}),
    ("分级≥20%转MA480", "tiered", {"confirm": 10, "switch_at": 20.0}),
    ("分级≥30%转MA480", "tiered", {"confirm": 10, "switch_at": 30.0}),
    ("分级≥50%转MA480", "tiered", {"confirm": 10, "switch_at": 50.0}),
    ("回撤15%离场", "pct", {"trail_pct": 15.0}),
    ("回撤25%离场", "pct", {"trail_pct": 25.0}),
    ("吊灯k4", "chandelier", {"atr_k": 4.0}),
    ("吊灯k5", "chandelier", {"atr_k": 5.0}),
]


def main() -> int:
    md: List[str] = []
    add = md.append
    add("# 止盈方案对比：MA480 分级长拿 + 延迟止盈 + 回撤离场")
    add("")
    add("> 口径：对齐生产（slow=480 + vol过滤 + 硬止损→[MA288止损]→止盈规则→趋势反转）。activate 统一 15%。")
    add("> 分级/MA480 方案在「晋级长拿」后关闭 MA288 止损（否则 MA288 会先于 MA480 触发，MA480 形同虚设）。")
    add("")

    coins = ["BTCUSDT", "ETHUSDT", "SOLUSDT"]
    data = {}
    for coin in coins:
        bars = load_klines_30m(coin)
        data[coin] = (bars, precompute(bars))

    add("## Part 1. 复利收益对比（全样本）")
    add("")
    add("| 方案 | BTC | ETH | SOL |")
    add("|---|---|---|---|")
    results = {}
    for label, mode, cfg in STRATS:
        cells = []
        for coin in coins:
            params = dc.SYMBOL_PARAMS[coin]
            bars, pre = data[coin]
            trades = backtest_trades(coin, params, bars, pre, mode=mode, **cfg)
            c = comp([t["ret_pct"] for t in trades])
            cells.append(c)
            results[(label, coin)] = trades
        add(f"| {label} | " + " | ".join(f"{c:+.1f}%" for c in cells) + " |")
    add("")
    add("> 加粗 = 该币最优。重点看「分级转MA480」能否在不伤小单的前提下，靠长拿大单超过 MA192。")
    add("")

    # 找出每币最优
    add("## Part 2. 每币最优方案 + 相对 MA192 的提升")
    add("")
    add("| 币种 | MA192 复利 | 最优方案 | 最优复利 | 提升 |")
    add("|---|---|---|---|---|")
    for coin in coins:
        ma192_c = comp([t["ret_pct"] for t in results[("MA192 c10(A7最优)", coin)]])
        best = max(STRATS, key=lambda s: comp([t["ret_pct"] for t in results[(s[0], coin)]]))
        best_c = comp([t["ret_pct"] for t in results[(best[0], coin)]])
        add(f"| {coin} | {ma192_c:+.1f}% | {best[0]} | {best_c:+.1f}% | {best_c - ma192_c:+.1f}pp |")
    add("")

    # Part 3：分级方案明细（大单是否真的长拿到了）
    add("## Part 3. 「分级≥30%转MA480」明细（长拿是否有效）")
    add("")
    for coin in coins:
        trades = results[("分级≥30%转MA480", coin)]
        promoted = [t for t in trades if t["mfe_pct"] >= 30.0]   # 触发过 ≥30% 的单子
        long_held = [t for t in promoted if t["reason"].startswith("MA480")]  # 最终 MA480 离场
        add(f"### {coin}")
        add(f"- 触发过 ≥30% 的单子：{len(promoted)} 笔，平均收益 {mean([t['ret_pct'] for t in promoted]):+.1f}%")
        add(f"- 其中最终 MA480 止盈离场：{len(long_held)} 笔，平均收益 {mean([t['ret_pct'] for t in long_held]):+.1f}%")
        add("")
    add("> 若「触发≥30%的单子」最终大多没走 MA480、而是被 MA288 止损/趋势反转带走，说明 MA480 长拿实际很难触发。")
    add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "exit_variants_report.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
