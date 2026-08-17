"""MA192 止盈离场质量 + 大趋势顶部/底部提前识别研究。

背景（A7）：盈利单改用 MA192 动态止盈线后，BTC/ETH/SOL 复利大幅跑赢基线。
本脚本回答两个新问题：
  1. MA192 止盈时，离场价距离真正的顶部还有多远？回撤（give-back）有多大？
  2. 有没有办法用指标提前识别大趋势的顶部/底部区域，更早离场拿到更大利润？

口径：对齐生产（slow=480 + vol过滤 + 硬止损→MA288止损→[止盈规则]→趋势反转）。
  MA192 动态止盈 = MA192 + activate15% + confirm10根（A7 最优）。

输出：feature_report/top_exit_report.md
"""
from __future__ import annotations

import math
import os
from typing import List, Dict, Any, Optional

import data_config as dc
from loader import load_klines_30m


# ----------------------------------------------------------------------
# 指标序列（纯 Python，与 indicators.py 一致）
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


def rsi_series(closes, period):
    n = len(closes)
    gains = [0.0] * n
    losses = [0.0] * n
    for i in range(1, n):
        d = closes[i] - closes[i - 1]
        if d > 0.0:
            gains[i] = d
        elif d < 0.0:
            losses[i] = -d
    pg = [0.0] * (n + 1)
    pl = [0.0] * (n + 1)
    for i in range(n):
        pg[i + 1] = pg[i] + gains[i]
        pl[i + 1] = pl[i] + losses[i]
    out = [None] * n
    for i in range(period, n):
        ag = (pg[i + 1] - pg[i + 1 - period]) / period
        al = (pl[i + 1] - pl[i + 1 - period]) / period
        if al == 0.0:
            out[i] = 100.0 if ag > 0.0 else 50.0
        else:
            out[i] = 100.0 - 100.0 / (1.0 + ag / al)
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


def rolling_min_series(values, window):
    n = len(values)
    out = [None] * n
    dq = []  # 下标，值单调递增
    for i in range(n):
        v = values[i]
        while dq and values[dq[-1]] >= v:
            dq.pop()
        dq.append(i)
        if dq[0] <= i - window:
            dq.pop(0)
        if i >= window - 1:
            out[i] = values[dq[0]]
    return out


def rolling_max_series(values, window):
    n = len(values)
    out = [None] * n
    dq = []  # 下标，值单调递减
    for i in range(n):
        v = values[i]
        while dq and values[dq[-1]] <= v:
            dq.pop()
        dq.append(i)
        if dq[0] <= i - window:
            dq.pop(0)
        if i >= window - 1:
            out[i] = values[dq[0]]
    return out


# ----------------------------------------------------------------------
# 回测（可切换止盈规则），返回逐笔交易明细
# ----------------------------------------------------------------------

def backtest_trades(symbol, params, bars, pre, mode="ma192",
                    activate=15.0, confirm=10, atr_k=3.0, rsi_over=65.0, rsi_exit=50.0):
    """回测并记录每笔交易的入场/离场 idx、价、收益、MFE（最有利价）、止盈规则。

    mode:
      base      —— 移动止盈（activate + callback，对齐生产）
      ma192     —— MA192 动态止盈（activate + confirm 根收盘跌破 MA192）
      ma48      —— MA48 动态止盈（更快）
      chandelier—— 最高价 - k*ATR 吊灯止损
      rsi       —— 超买后 RSI 跌破 exit 水平离场
      donchian  —— 收盘跌破 N 根最低价（/突破 N 根最高价）
    """
    fast = params.fast_ma_period
    slow = params.slow_ma_period
    n = len(bars)
    closes = pre["closes"]
    highs = pre["highs"]
    lows = pre["lows"]
    vol48 = pre["vol48"]
    ma48 = pre["ma48"]
    ma192 = pre["ma192"]
    rsi = pre["rsi"]
    atr = pre["atr"]
    loN = pre["loN"]
    hiN = pre["hiN"]
    prefix = pre["prefix"]

    def sma_at(idx, period):
        if idx + 1 < period:
            return None
        return (prefix[idx + 1] - prefix[idx + 1 - period]) / period

    trades: List[Dict[str, Any]] = []
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
            bar = bars[i]
            side = pos["side"]
            entry = pos["entry"]
            pnl = (close - entry) / entry * 100.0 if side == "LONG" else (entry - close) / entry * 100.0
            pos["max_profit"] = max(pos["max_profit"], pnl)
            # 最有利价（LONG 用最高价，SHORT 用最低价）
            fav = bar.high if side == "LONG" else bar.low
            if (side == "LONG" and fav > pos["extreme"]) or (side == "SHORT" and fav < pos["extreme"]):
                pos["extreme"] = fav
                pos["extreme_idx"] = i

            exit_price = None
            reason = ""
            # 1. 硬止损
            if params.hard_stop_pct > 0.0:
                if side == "LONG" and bar.low <= pos["hard_stop"]:
                    exit_price, reason = pos["hard_stop"], "硬止损"
                elif side == "SHORT" and bar.high >= pos["hard_stop"]:
                    exit_price, reason = pos["hard_stop"], "硬止损"
            # 2. MA288 止损
            if exit_price is None and prev_fast_ma is not None:
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
                elif mode in ("ma192", "ma48"):
                    ma = ma192 if mode == "ma192" else ma48
                    if pos["max_profit"] >= activate and ma[i] is not None:
                        below = (side == "LONG" and close < ma[i]) or (side == "SHORT" and close > ma[i])
                        pos["below_count"] = pos["below_count"] + 1 if below else 0
                        if pos["below_count"] >= confirm:
                            exit_price, reason = close, ("MA192止盈" if mode == "ma192" else "MA48止盈")
                elif mode == "chandelier":
                    if pos["max_profit"] >= activate and atr[i] is not None:
                        if side == "LONG" and close <= pos["extreme"] - atr_k * atr[i]:
                            exit_price, reason = close, f"吊灯止损(k{atr_k})"
                        elif side == "SHORT" and close >= pos["extreme"] + atr_k * atr[i]:
                            exit_price, reason = close, f"吊灯止损(k{atr_k})"
                elif mode == "rsi":
                    if pos["max_profit"] >= activate and rsi[i] is not None:
                        if rsi[i] >= rsi_over:
                            pos["seen_over"] = True
                        elif pos["seen_over"] and rsi[i] < rsi_exit:
                            exit_price, reason = close, f"RSI退出(<{rsi_exit:.0f})"
                elif mode == "donchian":
                    if pos["max_profit"] >= activate and loN[i] is not None and hiN[i] is not None:
                        if side == "LONG" and close <= loN[i]:
                            exit_price, reason = close, "Donchian突破"
                        elif side == "SHORT" and close >= hiN[i]:
                            exit_price, reason = close, "Donchian突破"
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
                       "below_count": 0, "seen_over": False}
            elif fast_ma < slow_ma and prev_close > prev_fast_ma and close < fast_ma:
                pos = {"side": "SHORT", "entry": close, "entry_idx": i,
                       "hard_stop": close * (1.0 + params.hard_stop_pct / 100.0),
                       "max_profit": 0.0, "extreme": bars[i].low, "extreme_idx": i,
                       "below_count": 0, "seen_over": False}

    if pos is not None:
        ret = (closes[-1] - pos["entry"]) / pos["entry"] if pos["side"] == "LONG" else (pos["entry"] - closes[-1]) / pos["entry"]
        trades.append({"side": pos["side"], "reason": "持仓到结束", "ret_pct": ret * 100.0,
                       "entry": pos["entry"], "exit": closes[-1], "entry_idx": pos["entry_idx"], "exit_idx": n - 1,
                       "mfe_pct": pos["max_profit"], "extreme": pos["extreme"], "extreme_idx": pos["extreme_idx"]})
    return trades


def comp(rets):
    eq = 1.0
    for r in rets:
        eq *= (1.0 + r / 100.0)
    return (eq - 1.0) * 100.0


def mean(xs):
    return sum(xs) / len(xs) if xs else 0.0


def median(xs):
    if not xs:
        return float('nan')
    s = sorted(xs)
    m = len(s) // 2
    return s[m] if len(s) % 2 else (s[m - 1] + s[m]) / 2.0


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
        "ma48": sma_series(closes, 48),
        "ma192": sma_series(closes, 192),
        "rsi": rsi_series(closes, 14),
        "atr": atr_series(bars, 14),
        "loN": rolling_min_series(lows, 48),
        "hiN": rolling_max_series(highs, 48),
    }


# ----------------------------------------------------------------------
# 主流程
# ----------------------------------------------------------------------

def main() -> int:
    md: List[str] = []
    add = md.append
    add("# MA192 止盈离场质量 + 顶部/底部提前识别研究")
    add("")
    add("> 口径：对齐生产（slow=480 + vol过滤 + 硬止损→MA288止损→止盈规则→趋势反转）。")
    add("> MA192 动态止盈 = MA192 + activate15% + confirm10根。")
    add("")

    coins = ["BTCUSDT", "ETHUSDT", "SOLUSDT", "BNBUSDT", "SUIUSDT", "HYPEUSDT"]

    # 先加载所有数据
    data = {}
    for coin in coins:
        bars = load_klines_30m(coin)
        data[coin] = (bars, precompute(bars))

    # ================= Part 1：MA192 离场质量（6 币） =================
    add("## Part 1. MA192 止盈离场质量（回撤 / 距离顶部）")
    add("")
    add("只统计「MA192止盈」离场的盈利大单（即 A7 里让趋势跑远的那些单子）。")
    add("")
    add("| 币种 | 盈利大单数 | 平均收益 | 平均MFE | 平均回撤(MFE-收益) | 回撤/峰值 | 离场后90天继续 |")
    add("|---|---|---|---|---|---|---|")
    all_winners = {}
    for coin in coins:
        params = dc.SYMBOL_PARAMS[coin]
        bars, pre = data[coin]
        trades = backtest_trades(coin, params, bars, pre, mode="ma192", activate=15.0, confirm=10)
        winners = [t for t in trades if t["reason"] == "MA192止盈"]
        all_winners[coin] = (winners, bars, pre)
        if not winners:
            add(f"| {coin} | 0 | — | — | — | — | — |")
            continue
        n = len(bars)
        giveback = []   # 回撤（入场价的 %）
        gb_rel = []     # 回撤 / 峰值价
        post90 = []     # 离场后 90 天（4320 根）继续涨
        for t in winners:
            mfe_p = t["mfe_pct"]
            gb = mfe_p - t["ret_pct"]
            giveback.append(gb)
            if t["extreme"] != 0.0:
                if t["side"] == "LONG":
                    gb_rel.append((t["extreme"] - t["exit"]) / t["extreme"] * 100.0)
                else:
                    gb_rel.append((t["exit"] - t["extreme"]) / t["extreme"] * 100.0)
            ei = t["exit_idx"]
            if ei + 1 < n:
                H = 90 * 48
                end = min(n, ei + 1 + H)
                if t["side"] == "LONG":
                    fh = max(bars[ei + 1:end], key=lambda b: b.high).high
                    post90.append((fh - t["exit"]) / t["exit"] * 100.0)
                else:
                    fl = min(bars[ei + 1:end], key=lambda b: b.low).low
                    post90.append((t["exit"] - fl) / t["exit"] * 100.0)
        add(f"| {coin} | {len(winners)} | {mean([t['ret_pct'] for t in winners]):+.2f}% | "
            f"{mean([t['mfe_pct'] for t in winners]):+.2f}% | {mean(giveback):+.2f}% | "
            f"{mean(gb_rel):+.2f}% | {mean(post90):+.2f}% |")
    add("")
    add("> 「平均回撤」= 从持仓期间的最高价（MFE）回撤到离场价，让掉了多少利润（占入场价 %）。")
    add("> 「回撤/峰值」= 离场价相对最高价跌了多少（%）。数值越大，说明 MA192 等收盘确认、离场越晚、离顶越远。")
    add("> 「离场后90天继续」= 离场后趋势又涨了多少（若为负，说明离场后已见顶回落）。")
    add("")

    # ================= Part 2：顶部形态特征（BTC/ETH/SOL） =================
    add("## Part 2. 顶部形态特征（能否用指标识别顶部）")
    add("")
    add("对 BTC/ETH/SOL 的每笔 MA192 盈利大单，在「持仓内最高价那根 bar」打快照，看顶部有哪些可识别特征。")
    add("")
    add("| 指标（在顶部时） | 与「回撤大小」相关性 | 与「离场后90天继续」相关性 |")
    add("|---|---|---|")
    for coin in ["BTCUSDT", "ETHUSDT", "SOLUSDT"]:
        winners, bars, pre = all_winners[coin]
        if not winners:
            continue
        closes = pre["closes"]
        n = len(bars)
        rows = []
        for t in winners:
            pi = t["extreme_idx"]
            if pi < 21:
                continue
            mfe_p = t["mfe_pct"]
            gb = mfe_p - t["ret_pct"]
            # 离场后 90 天继续
            ei = t["exit_idx"]
            post = 0.0
            if ei + 1 < n:
                end = min(n, ei + 1 + 90 * 48)
                if t["side"] == "LONG":
                    fh = max(bars[ei + 1:end], key=lambda b: b.high).high
                    post = (fh - t["exit"]) / t["exit"] * 100.0
                else:
                    fl = min(bars[ei + 1:end], key=lambda b: b.low).low
                    post = (t["exit"] - fl) / t["exit"] * 100.0
            rsi_p = pre["rsi"][pi]
            ext192 = (closes[pi] - pre["ma192"][pi]) / pre["ma192"][pi] * 100.0 if pre["ma192"][pi] else None
            vol_ratio = bars[pi].volume / (sum(b.volume for b in bars[pi - 20:pi]) / 20.0) if pi >= 20 else None
            time_to_peak = pi - t["entry_idx"]
            rows.append({"gb": gb, "post": post, "rsi": rsi_p, "ext192": ext192,
                         "vol_ratio": vol_ratio, "ttp": time_to_peak})
        # 相关性
        def corr(xs, ys):
            if len(xs) < 5:
                return float('nan')
            mx = mean(xs); my = mean(ys)
            cov = sum((x - mx) * (y - my) for x, y in zip(xs, ys))
            vx = sum((x - mx) ** 2 for x in xs); vy = sum((y - my) ** 2 for y in ys)
            if vx == 0 or vy == 0:
                return float('nan')
            return cov / (vx * vy) ** 0.5
        # 只输出 BTC/ETH/SOL 的汇总（各指标与回撤、与继续涨的相关性）
        rsi_all = [r["rsi"] for r in rows if r["rsi"] is not None]
        ext_all = [r["ext192"] for r in rows if r["ext192"] is not None]
        vol_all = [r["vol_ratio"] for r in rows if r["vol_ratio"] is not None]
        # 与回撤
        gb_rsi = corr([r["gb"] for r in rows if r["rsi"] is not None], rsi_all)
        gb_ext = corr([r["gb"] for r in rows if r["ext192"] is not None], ext_all)
        gb_vol = corr([r["gb"] for r in rows if r["vol_ratio"] is not None], vol_all)
        # 与继续涨
        p_rsi = corr([r["post"] for r in rows if r["rsi"] is not None], rsi_all)
        p_ext = corr([r["post"] for r in rows if r["ext192"] is not None], ext_all)
        p_vol = corr([r["post"] for r in rows if r["vol_ratio"] is not None], vol_all)
        add(f"| {coin}·RSI14 | {gb_rsi:+.2f} | {p_rsi:+.2f} |")
        add(f"| {coin}·离MA192幅度 | {gb_ext:+.2f} | {p_ext:+.2f} |")
        add(f"| {coin}·量比(顶部bar) | {gb_vol:+.2f} | {p_vol:+.2f} |")
    add("")
    add("> 相关性 >0 = 指标越高回撤越大/继续涨越多；<0 = 指标越高回撤越小。|r|>0.3 才算有弱信号，>0.5 较强。")
    add("> 若「离MA192幅度」与「离场后90天继续」强正相关 → 价格越远离均线越可能继续（追高反而对），顶部难提前判。")
    add("")

    # ================= Part 3：提前离场规则对比（BTC/ETH/SOL） =================
    add("## Part 3. 提前离场规则对比（能否拿到更大利润）")
    add("")
    add("用不同止盈规则替换 MA192，看复利是变多还是变少（更早离场 vs 被震下车）。")
    add("")
    add("| 币种 | 基线(移动止盈) | MA192 | MA48 | 吊灯k2 | 吊灯k3 | RSI退出 | Donchian48 |")
    add("|---|---|---|---|---|---|---|---|")
    MODES = [
        ("base", {}),
        ("ma192", {"activate": 15.0, "confirm": 10}),
        ("ma48", {"activate": 15.0, "confirm": 10}),
        ("chandelier", {"activate": 15.0, "atr_k": 2.0}),
        ("chandelier", {"activate": 15.0, "atr_k": 3.0}),
        ("rsi", {"activate": 15.0}),
        ("donchian", {"activate": 15.0}),
    ]
    for coin in ["BTCUSDT", "ETHUSDT", "SOLUSDT"]:
        params = dc.SYMBOL_PARAMS[coin]
        bars, pre = data[coin]
        cells = []
        for mode, cfg in MODES:
            trades = backtest_trades(coin, params, bars, pre, mode=mode, **cfg)
            cells.append(comp([t["ret_pct"] for t in trades]))
        add(f"| {coin} | " + " | ".join(f"{c:+.1f}%" for c in cells) + " |")
    add("")
    add("> 「基线」= 生产当前的回撤 callback 移动止盈；「MA192」= A7 最优。")
    add("> 若某提前离场规则复利超过 MA192，说明确实能靠更早识别顶部拿到更大利润；否则说明 MA192 已经在「回撤 vs 震下车」之间取到了更好平衡。")
    add("")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report", "top_exit_report.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
