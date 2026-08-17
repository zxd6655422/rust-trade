"""A9 续：自适应均线止盈研究 v3。

v3 新增三个研究方向：
  1. 趋势衰竭降级：MA480 长拿期间，利润从峰值回撤超过阈值 → 降回 MA192 锁利
  2. 早期速度窗口：用入场后前 50 bar 的利润速度作为升级信号（而非全程速度）
  3. 切换阈值敏感性：switch_at 从 15% 到 30% 的敏感性分析

口径：对齐生产（slow=480 + vol过滤 + 硬止损→[MA288止损]→止盈规则→趋势反转）。
输出：feature_report/adaptive_ma_trailing_report.md
"""
from __future__ import annotations

import math
import os
from datetime import datetime
from typing import List, Dict, Any

import data_config as dc
from loader import load_klines_30m


# =====================================================================
# 指标序列
# =====================================================================

def sma_series(values, period):
    n = len(values)
    p = [0.0] * (n + 1)
    for i in range(n):
        p[i + 1] = p[i] + values[i]
    out = [None] * n
    for i in range(period - 1, n):
        out[i] = (p[i + 1] - p[i + 1 - period]) / period
    return out


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
    n = len(bars)
    prefix = [0.0] * (n + 1)
    for i in range(n):
        prefix[i + 1] = prefix[i] + closes[i]
    return {
        "closes": closes, "prefix": prefix,
        "vol48": realized_vol_48_series(closes),
        "ma192": sma_series(closes, 192),
        "ma288": sma_series(closes, 288),
        "ma480": sma_series(closes, 480),
    }


# =====================================================================
# 回测引擎
# =====================================================================

MA_KEYS = {192: "ma192", 288: "ma288", 480: "ma480"}


def new_pos(side, entry, idx, bar, params):
    return {
        "side": side, "entry": entry, "entry_idx": idx,
        "hard_stop": entry * (1.0 - params.hard_stop_pct / 100.0) if side == "LONG"
                     else entry * (1.0 + params.hard_stop_pct / 100.0),
        "max_profit": 0.0, "extreme": bar.high if side == "LONG" else bar.low, "extreme_idx": idx,
        "hold_bars": 0, "below_count": 0, "tp": None, "tp_prev": None,
        "use_ma288_stop": True,
        "demoted": False,  # 是否已降级
        # 早期速度窗口
        "early_vel": 0.0,
        "early_profit_at_50": None,  # 入场后第 50 bar 的 max_profit
    }


def backtest_trades(symbol, params, bars, pre, mode, activate=15.0, confirm=10,
                    switch_at=20.0, demote_pct=10.0, early_vel_thr=0.10,
                    y0=None, y1=None, record_details=False):
    """回测并返回逐笔交易。

    mode:
      base       — 生产移动止盈
      ma192      — 固定 MA192
      tiered     — 分级：profit < switch_at → MA192，>= switch_at → MA480
      tiered_demote — 分级 + 趋势衰竭降级
      tiered_early  — 分级 + 早期速度窗口
      tiered_full   — 分级 + 衰竭降级 + 早期速度
    """
    fast = params.fast_ma_period   # 288
    slow = params.slow_ma_period   # 480
    n = len(bars)
    years = [datetime.fromtimestamp(b.open_time / 1000).year for b in bars]
    closes = pre["closes"]
    vol48 = pre["vol48"]
    ma192s = pre["ma192"]
    ma288s = pre["ma288"]
    ma480s = pre["ma480"]
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
            pos["hold_bars"] += 1

            fav = bar.high if side == "LONG" else bar.low
            if (side == "LONG" and fav > pos["extreme"]) or (side == "SHORT" and fav < pos["extreme"]):
                pos["extreme"] = fav
                pos["extreme_idx"] = i

            # ── 早期速度窗口 ──
            if pos["hold_bars"] == 50:
                pos["early_profit_at_50"] = pos["max_profit"]
                pos["early_vel"] = pos["max_profit"] / 50.0

            # ── 确定止盈线 ──
            use_ma288_stop = True
            tp = None

            if mode == "base":
                use_ma288_stop, tp = True, None
            elif mode == "ma192":
                use_ma288_stop, tp = True, 192
            elif mode == "tiered":
                if pos["max_profit"] >= switch_at:
                    use_ma288_stop, tp = False, 480
                else:
                    use_ma288_stop, tp = True, 192
            elif mode == "tiered_keep288":
                # 分级但升级后保持 MA288 止损开启（验证 MA288 关闭的影响）
                if pos["max_profit"] >= switch_at:
                    use_ma288_stop, tp = True, 480  # MA288 保持开启！
                else:
                    use_ma288_stop, tp = True, 192
            elif mode == "tiered_demote":
                # 分级 + 衰竭降级
                if pos.get("demoted"):
                    # 已降级，锁定 MA192（MA288 止损保持关闭，与 A9 分级一致）
                    use_ma288_stop, tp = False, 192
                elif pos["max_profit"] >= switch_at:
                    # 检查是否衰竭：利润从峰值回撤超过 demote_pct
                    profit_decline = pos["max_profit"] - pnl  # 从峰值到当前的回撤
                    if profit_decline >= demote_pct and pnl < pos["max_profit"] - demote_pct:
                        # 衰竭！降回 MA192
                        pos["demoted"] = True
                        use_ma288_stop, tp = True, 192
                    else:
                        use_ma288_stop, tp = False, 480
                else:
                    use_ma288_stop, tp = True, 192
            elif mode == "tiered_demote_keep288":
                # 分级 + 衰竭降级 + MA288 保持开启
                if pos.get("demoted"):
                    use_ma288_stop, tp = True, 192
                elif pos["max_profit"] >= switch_at:
                    profit_decline = pos["max_profit"] - pnl
                    if profit_decline >= demote_pct and pnl < pos["max_profit"] - demote_pct:
                        pos["demoted"] = True
                        use_ma288_stop, tp = True, 192
                    else:
                        use_ma288_stop, tp = True, 480  # MA288 保持开启！
                else:
                    use_ma288_stop, tp = True, 192
            elif mode == "tiered_early":
                # 分级 + 早期速度窗口
                if pos["max_profit"] >= switch_at:
                    # 检查早期速度
                    ev = pos.get("early_vel", 0.0)
                    if pos["hold_bars"] <= 50:
                        # 还在早期窗口内，用当前速度
                        cur_vel = pos["max_profit"] / pos["hold_bars"] if pos["hold_bars"] > 0 else 0
                        if cur_vel >= early_vel_thr:
                            use_ma288_stop, tp = False, 480
                        else:
                            use_ma288_stop, tp = True, 192
                    elif ev >= early_vel_thr:
                        # 早期速度快 → 升级到 MA480
                        use_ma288_stop, tp = False, 480
                    else:
                        # 早期速度慢 → 保持 MA192
                        use_ma288_stop, tp = True, 192
                else:
                    use_ma288_stop, tp = True, 192
            elif mode == "tiered_full":
                # 分级 + 衰竭降级 + 早期速度
                if pos.get("demoted"):
                    use_ma288_stop, tp = False, 192
                elif pos["max_profit"] >= switch_at:
                    # 先检查早期速度
                    ev = pos.get("early_vel", 0.0)
                    if pos["hold_bars"] > 50 and ev < early_vel_thr:
                        # 早期速度慢 → 不升级
                        use_ma288_stop, tp = True, 192
                    else:
                        # 检查衰竭
                        profit_decline = pos["max_profit"] - pnl
                        if profit_decline >= demote_pct and pnl < pos["max_profit"] - demote_pct:
                            pos["demoted"] = True
                            use_ma288_stop, tp = True, 192
                        else:
                            use_ma288_stop, tp = False, 480
                else:
                    use_ma288_stop, tp = True, 192
            else:
                use_ma288_stop, tp = True, None

            pos["use_ma288_stop"] = use_ma288_stop
            pos["tp"] = tp

            exit_price = None
            reason = ""
            # 1. 硬止损
            if params.hard_stop_pct > 0.0:
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
            # 3. 止盈规则
            if exit_price is None:
                if mode == "base":
                    if pos["max_profit"] >= params.trailing_activate_pct and \
                       pos["max_profit"] - pnl >= params.trailing_callback_pct:
                        exit_price, reason = close, "移动止盈"
                elif tp is not None and pos["max_profit"] >= activate:
                    ma_v = pre[MA_KEYS[tp]][i]
                    if ma_v is not None:
                        below = (side == "LONG" and close < ma_v) or (side == "SHORT" and close > ma_v)
                        if pos.get("tp_prev") != tp:
                            pos["below_count"] = 0
                            pos["tp_prev"] = tp
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
                trade = {
                    "side": side, "reason": reason, "ret_pct": ret * 100.0,
                    "entry": entry, "exit": exit_price, "entry_idx": pos["entry_idx"], "exit_idx": i,
                    "mfe_pct": pos["max_profit"], "extreme": pos["extreme"], "extreme_idx": pos["extreme_idx"],
                    "hold_bars": pos["hold_bars"],
                }
                if record_details:
                    trade["demoted"] = pos.get("demoted", False)
                    trade["early_vel"] = pos.get("early_vel", 0.0)
                    trade["tp_final"] = pos.get("tp")
                trades.append(trade)
                pos = None
                continue

        if pos is None and fast_ma is not None and slow_ma is not None and prev_fast_ma is not None:
            if vol48[i] is not None and vol48[i] >= params.realized_vol_threshold:
                continue
            if fast_ma > slow_ma and prev_close < prev_fast_ma and close > fast_ma:
                pos = new_pos("LONG", close, i, bars[i], params)
            elif fast_ma < slow_ma and prev_close > prev_fast_ma and close < fast_ma:
                pos = new_pos("SHORT", close, i, bars[i], params)

    if pos is not None and last_in is not None:
        ret = (closes[last_in] - pos["entry"]) / pos["entry"] if pos["side"] == "LONG" else (pos["entry"] - closes[last_in]) / pos["entry"]
        trades.append({"side": pos["side"], "reason": "持仓到结束", "ret_pct": ret * 100.0,
                       "entry": pos["entry"], "exit": closes[last_in], "entry_idx": pos["entry_idx"], "exit_idx": last_in,
                       "mfe_pct": pos["max_profit"], "extreme": pos["extreme"], "extreme_idx": pos["extreme_idx"],
                       "hold_bars": pos.get("hold_bars", 0)})
    return trades


def comp(rets):
    eq = 1.0
    for r in rets:
        eq *= (1.0 + r / 100.0)
    return (eq - 1.0) * 100.0


def mean(xs):
    return sum(xs) / len(xs) if xs else 0.0


def fmt(x, d=1):
    if x is None or (isinstance(x, float) and math.isnan(x)):
        return "N/A"
    return f"{x:.{d}f}"


# =====================================================================
# 主流程
# =====================================================================

STRATS = [
    ("基线callback", "base", {}),
    ("MA192 c10", "ma192", {"confirm": 10}),
    ("分级>=20%转MA480", "tiered", {"confirm": 10, "switch_at": 20.0}),
    ("分级+保持MA288", "tiered_keep288", {"confirm": 10, "switch_at": 20.0}),
    ("分级+衰竭降级10%", "tiered_demote", {"confirm": 10, "switch_at": 20.0, "demote_pct": 10.0}),
    ("分级+衰竭+保持288", "tiered_demote_keep288", {"confirm": 10, "switch_at": 20.0, "demote_pct": 10.0}),
    ("分级+早期速度0.05", "tiered_early", {"confirm": 10, "switch_at": 20.0, "early_vel_thr": 0.05}),
]


def main() -> int:
    md: List[str] = []
    add = md.append
    add("# A9 续：自适应均线止盈研究 v3")
    add("")
    add("> **v2 结论**：趋势健康度验证无增量价值——能达到 20% 利润的交易必然趋势健康。")
    add("> **v3 新方向**：")
    add("> 1. 趋势衰竭降级：MA480 长拿期间利润从峰值回撤超阈值 → 降回 MA192 锁利")
    add("> 2. 早期速度窗口：用入场后前 50 bar 的利润速度作为升级信号")
    add("> 3. 切换阈值敏感性：switch_at 从 15% 到 30%")
    add("> 口径：对齐生产（slow=480 + vol过滤 + 硬止损→[MA288止损]→止盈规则→趋势反转）。")
    add("")

    coins = ["BTCUSDT", "ETHUSDT", "SOLUSDT"]
    data = {}
    for coin in coins:
        bars = load_klines_30m(coin)
        data[coin] = (bars, precompute(bars))

    # ================= Part 1：早期速度分析 =================
    add("## Part 1. 早期速度分析（入场后前 50 bar 的利润速度）")
    add("")
    add("大单是否在入场早期就表现出更高的利润速度？")
    add("")

    for coin in coins:
        params = dc.SYMBOL_PARAMS[coin]
        bars, pre = data[coin]
        trades = backtest_trades(coin, params, bars, pre, mode="tiered", confirm=10,
                                 switch_at=20.0, record_details=True)

        winners = [t for t in trades if t["ret_pct"] > 0]
        losers = [t for t in trades if t["ret_pct"] <= 0]
        big = [t for t in trades if t["mfe_pct"] >= 20.0]  # 触发过升级的大单
        small_win = [t for t in trades if 0 < t["mfe_pct"] < 20.0]

        add(f"### {coin}")
        add("")
        add("| 类别 | 笔数 | 平均早期速度(%/bar) | 早期速度>=0.05 | 早期速度>=0.10 |")
        add("|---|---|---|---|---|")

        for label, subset in [("盈利单", winners), ("止损单", losers),
                               ("大单(MFE>=20%)", big), ("小盈利单(MFE<20%)", small_win)]:
            if not subset:
                add(f"| {label} | 0 | N/A | N/A | N/A |")
                continue
            evs = [t.get("early_vel", 0.0) for t in subset]
            avg_ev = mean(evs)
            pct_05 = len([e for e in evs if e >= 0.05]) / len(evs) * 100
            pct_10 = len([e for e in evs if e >= 0.10]) / len(evs) * 100
            add(f"| {label} | {len(subset)} | {avg_ev:.3f} | {pct_05:.1f}% | {pct_10:.1f}% |")
        add("")

    add("> **解读**：")
    add("> - 若大单的早期速度显著高于小盈利单 → 早期速度有区分度")
    add("> - 若大单早期速度 >=0.05 的比例远高于止损单 → 可用于过滤")
    add("")

    # ================= Part 2：全样本复利对比 =================
    add("## Part 2. 全样本复利对比")
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

    # 交易笔数
    add("### 交易笔数")
    add("")
    add("| 方案 | BTC | ETH | SOL |")
    add("|---|---|---|---|")
    for label, mode, cfg in STRATS:
        cells = [len(results[(label, c)]) for c in coins]
        add(f"| {label} | " + " | ".join(str(c) for c in cells) + " |")
    add("")

    # ================= Part 3：衰竭降级分析 =================
    add("## Part 3. 衰竭降级分析")
    add("")
    add("在「分级+衰竭降级10%」方案中，有多少大单被降级？降级后效果如何？")
    add("")

    for coin in coins:
        params = dc.SYMBOL_PARAMS[coin]
        bars, pre = data[coin]
        trades = backtest_trades(coin, params, bars, pre, mode="tiered_demote", confirm=10,
                                 switch_at=20.0, demote_pct=10.0, record_details=True)

        demoted = [t for t in trades if t.get("demoted")]
        not_demoted = [t for t in trades if not t.get("demoted") and t["mfe_pct"] >= 20.0]

        add(f"### {coin}")
        add("")
        add(f"- 触发过升级（MFE>=20%）的交易：{len(demoted) + len(not_demoted)} 笔")
        add(f"- 其中被降级（衰竭）：{len(demoted)} 笔")
        add(f"- 未降级（长拿到 MA480 离场）：{len(not_demoted)} 笔")
        if demoted:
            add(f"- 降级单平均收益：{mean([t['ret_pct'] for t in demoted]):+.2f}%")
            add(f"- 降级单平均MFE：{mean([t['mfe_pct'] for t in demoted]):+.2f}%")
        if not_demoted:
            add(f"- 未降级单平均收益：{mean([t['ret_pct'] for t in not_demoted]):+.2f}%")
            add(f"- 未降级单平均MFE：{mean([t['mfe_pct'] for t in not_demoted]):+.2f}%")
        add("")

    # ================= Part 4：切换阈值敏感性 =================
    add("## Part 4. 切换阈值敏感性（switch_at）")
    add("")
    add("固定 confirm=10，改变 switch_at 阈值（从 MA192 切到 MA480 的利润门槛）。")
    add("")
    add("| switch_at | BTC | ETH | SOL |")
    add("|---|---|---|---|")
    for sw in [12.0, 15.0, 18.0, 20.0, 22.0, 25.0, 30.0, 35.0]:
        cells = []
        for coin in coins:
            params = dc.SYMBOL_PARAMS[coin]
            bars, pre = data[coin]
            trades = backtest_trades(coin, params, bars, pre, mode="tiered", confirm=10, switch_at=sw)
            c = comp([t["ret_pct"] for t in trades])
            cells.append(c)
        add(f"| {sw:.0f}% | " + " | ".join(f"{c:+.1f}%" for c in cells) + " |")
    add("")
    add("> **解读**：")
    add("> - 若 15%-25% 范围内收益变化小 → 阈值不敏感，20% 是稳健选择")
    add("> - 若某个阈值明显最优 → 需要时间切分验证是否过拟合")
    add("")

    # switch_at 时间切分
    add("### switch_at 时间切分验证")
    add("")
    for coin in coins:
        params = dc.SYMBOL_PARAMS[coin]
        bars, pre = data[coin]
        years = [datetime.fromtimestamp(b.open_time / 1000).year for b in bars]
        y0, y1 = min(years), max(years)
        mid = (y0 + y1) // 2
        tr0, tr1, va0, va1 = y0, mid, mid + 1, y1
        if va1 - va0 < 1 or tr1 - tr0 < 1:
            continue

        add(f"#### {coin}（训练 {tr0}-{tr1} -> 验证 {va0}-{va1}）")
        add("")
        add("| switch_at | 训练复利 | 验证复利 |")
        add("|---|---|---|")
        for sw in [15.0, 18.0, 20.0, 22.0, 25.0, 30.0]:
            train_c = comp([t["ret_pct"] for t in backtest_trades(
                coin, params, bars, pre, mode="tiered", confirm=10, switch_at=sw, y0=tr0, y1=tr1)])
            val_c = comp([t["ret_pct"] for t in backtest_trades(
                coin, params, bars, pre, mode="tiered", confirm=10, switch_at=sw, y0=va0, y1=va1)])
            add(f"| {sw:.0f}% | {train_c:+.1f}% | {val_c:+.1f}% |")
        add("")

    # ================= Part 5：综合方案时间切分 =================
    add("## Part 5. 综合方案时间切分验证")
    add("")
    for coin in coins:
        params = dc.SYMBOL_PARAMS[coin]
        bars, pre = data[coin]
        years = [datetime.fromtimestamp(b.open_time / 1000).year for b in bars]
        y0, y1 = min(years), max(years)
        mid = (y0 + y1) // 2
        tr0, tr1, va0, va1 = y0, mid, mid + 1, y1
        if va1 - va0 < 1 or tr1 - tr0 < 1:
            continue

        add(f"### {coin}（训练 {tr0}-{tr1} -> 验证 {va0}-{va1}）")
        add("")
        add("| 方案 | 训练复利 | 验证复利 | vs基线 |")
        add("|---|---|---|---|")

        base_val = comp([t["ret_pct"] for t in backtest_trades(
            coin, params, bars, pre, mode="base", y0=va0, y1=va1)])
        for label, mode, cfg in STRATS:
            train_c = comp([t["ret_pct"] for t in backtest_trades(
                coin, params, bars, pre, mode=mode, y0=tr0, y1=tr1, **cfg)])
            val_c = comp([t["ret_pct"] for t in backtest_trades(
                coin, params, bars, pre, mode=mode, y0=va0, y1=va1, **cfg)])
            beat = "WIN" if val_c > base_val else "LOSE"
            add(f"| {label} | {train_c:+.1f}% | {val_c:+.1f}% | {beat} (基线{base_val:+.1f}%) |")
        add("")

    # ================= Part 6：回撤对比 =================
    add("## Part 6. 回撤对比")
    add("")
    add("| 方案 | BTC | ETH | SOL |")
    add("|---|---|---|---|")
    for label, mode, cfg in STRATS:
        cells = []
        for coin in coins:
            trades = results[(label, coin)]
            gbs = [t["mfe_pct"] - t["ret_pct"] for t in trades if t["mfe_pct"] > 0]
            cells.append(mean(gbs))
        add(f"| {label} | " + " | ".join(f"{c:+.2f}%" for c in cells) + " |")
    add("")

    # ================= 写出报告 =================
    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report",
                       "adaptive_ma_trailing_report.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
