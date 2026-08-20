r"""A11 BTC 逐年诊断：分年度盈亏 + 离场后走势 + 入场质量分析。

口径：A11 三段式止盈（移动止盈→MA192→MA480），BTC switch1=6% / switch2=12%。
      slow=480 + vol过滤 + 硬止损→MA288止损→三段式止盈→趋势反转。

输出：feature_report/a11_btc_yearly_diag.md

运行：
  cd D:\dev-projects\rust-trade\trade-test\src
  python a11_btc_yearly_diag.py
"""
from __future__ import annotations

import os
from collections import defaultdict
from datetime import datetime, timezone, timedelta
from typing import Dict, List, Any

import data_config as dc
from loader import load_klines_30m
from study_adaptive_ma_trailing import comp, precompute

BJ = timezone(timedelta(hours=8))
SRC = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(SRC, "feature_report")
HORIZONS = [20, 50, 100]
SYMBOL = "BTCUSDT"
SWITCH1 = 6.0
SWITCH2 = 12.0
ACTIVATE_SMALL = 4.0
CALLBACK_SMALL = 1.5
CONFIRM = 10
DEMOTE_PCT = 10.0


def backtest_a11(symbol, params, bars, pre, switch1, switch2,
                 activate_small=4.0, callback_small=1.5, confirm=10, demote_pct=10.0):
    """A11 三段式止盈回测，返回含 side/year 的逐笔交易。"""
    fast = params.fast_ma_period
    slow = params.slow_ma_period
    n = len(bars)
    years = [datetime.fromtimestamp(b.open_time / 1000, tz=BJ).year for b in bars]
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
                use_ma288_stop, tp = False, 192
            elif pos["max_profit"] >= switch2:
                profit_decline = pos["max_profit"] - pnl
                if profit_decline >= demote_pct and pnl < pos["max_profit"] - demote_pct:
                    pos["demoted"] = True
                    use_ma288_stop, tp = True, 192
                else:
                    use_ma288_stop, tp = False, 480
            elif pos["max_profit"] >= switch1:
                use_ma288_stop, tp = True, 192
            else:
                use_ma288_stop, tp = True, None
                if pos["max_profit"] >= activate_small and pos["max_profit"] - pnl >= callback_small:
                    exit_price, reason = close, "移动止盈"

            # 硬止损
            if exit_price is None and params.hard_stop_pct > 0.0:
                if side == "LONG" and bar.low <= pos["hard_stop"]:
                    exit_price, reason = pos["hard_stop"], "硬止损"
                elif side == "SHORT" and bar.high >= pos["hard_stop"]:
                    exit_price, reason = pos["hard_stop"], "硬止损"
            # MA288 止损
            if exit_price is None and use_ma288_stop and prev_fast_ma is not None:
                if side == "LONG" and prev_close > prev_fast_ma and close < fast_ma:
                    exit_price, reason = close, "MA288止损"
                elif side == "SHORT" and prev_close < prev_fast_ma and close > fast_ma:
                    exit_price, reason = close, "MA288止损"
            # MA 止盈线
            if exit_price is None and tp is not None:
                ma_v = (ma192s if tp == 192 else ma480s)[i]
                if ma_v is not None:
                    below = (side == "LONG" and close < ma_v) or (side == "SHORT" and close > ma_v)
                    pos["below_count"] = pos["below_count"] + 1 if below else 0
                    if pos["below_count"] >= confirm:
                        exit_price, reason = close, f"MA{tp}止盈"
            # 趋势反转
            if exit_price is None:
                if side == "LONG" and fast_ma < slow_ma:
                    exit_price, reason = close, "趋势反转"
                elif side == "SHORT" and fast_ma > slow_ma:
                    exit_price, reason = close, "趋势反转"

            if exit_price is not None:
                ret = (exit_price - entry) / entry * 100.0 if side == "LONG" else (entry - exit_price) / entry * 100.0
                trades.append({
                    "ret_pct": ret, "reason": reason, "side": side,
                    "entry_idx": pos["entry_idx"], "exit_idx": i,
                    "mfe_pct": pos["max_profit"], "hold_bars": pos["hold_bars"],
                    "year": years[i], "demoted": pos.get("demoted", False),
                })
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
        ret = (closes[-1] - pos["entry"]) / pos["entry"] * 100.0 if pos["side"] == "LONG" else (pos["entry"] - closes[-1]) / pos["entry"] * 100.0
        trades.append({
            "ret_pct": ret, "reason": "持仓到结束", "side": pos["side"],
            "entry_idx": pos["entry_idx"], "exit_idx": n - 1,
            "mfe_pct": pos["max_profit"], "hold_bars": pos["hold_bars"],
            "year": years[-1], "demoted": pos.get("demoted", False),
        })
    return trades


def post_exit_favorable(bars, exit_idx: int, side: str, horizon: int) -> float:
    """离场后 horizon 根 30m 内，行情继续朝原持仓方向走的最大幅度（%）。"""
    if exit_idx + 1 >= len(bars):
        return 0.0
    seg = bars[exit_idx + 1: exit_idx + 1 + horizon]
    if not seg:
        return 0.0
    ref = bars[exit_idx].close
    if side == "LONG":
        best = max(b.high for b in seg)
        return (best - ref) / ref * 100.0 if ref > 0 else 0.0
    else:
        best = min(b.low for b in seg)
        return (ref - best) / ref * 100.0 if ref > 0 else 0.0


def enrich_trades(bars30, trades: List[Dict]) -> List[Dict]:
    """给每笔交易附加离场后漂移数据。"""
    out = []
    for t in trades:
        row = dict(t)
        for h in HORIZONS:
            row[f"post_{h}"] = post_exit_favorable(bars30, int(t["exit_idx"]), t["side"], h)
        out.append(row)
    return out


def yearly_agg(trades: List[Dict]) -> Dict[int, List[Dict]]:
    by = defaultdict(list)
    for t in trades:
        by[int(t["year"])].append(t)
    return dict(sorted(by.items()))


def stats(ts: List[Dict]) -> Dict[str, Any]:
    n = len(ts)
    if n == 0:
        return {"n": 0}
    wins = [t for t in ts if t["ret_pct"] > 0]
    losses = [t for t in ts if t["ret_pct"] <= 0]
    rets = [t["ret_pct"] for t in ts]
    return {
        "n": n,
        "win_rate": len(wins) / n * 100.0,
        "simple_pct": sum(rets),
        "compound_pct": comp(rets),
        "avg_win": sum(t["ret_pct"] for t in wins) / len(wins) if wins else 0.0,
        "avg_loss": sum(t["ret_pct"] for t in losses) / len(losses) if losses else 0.0,
        "avg_mfe": sum(t["mfe_pct"] for t in ts) / n,
        "avg_hold": sum(t["hold_bars"] for t in ts) / n,
    }


def main() -> int:
    params = dc.SYMBOL_PARAMS[SYMBOL]
    bars = load_klines_30m(SYMBOL)
    pre = precompute(bars)

    trades_raw = backtest_a11(SYMBOL, params, bars, pre, SWITCH1, SWITCH2,
                              ACTIVATE_SMALL, CALLBACK_SMALL, CONFIRM, DEMOTE_PCT)
    trades = enrich_trades(bars, trades_raw)
    by_year = yearly_agg(trades)

    md = []
    add = md.append
    add(f"# A11 {SYMBOL} 逐年诊断")
    add("")
    add(f"> 口径：三段式止盈 switch1={SWITCH1}% / switch2={SWITCH2}%，")
    add(f"> activate_small={ACTIVATE_SMALL}% / callback_small={CALLBACK_SMALL}% / confirm={CONFIRM} / demote_pct={DEMOTE_PCT}%")
    add(f"> slow=480 + vol过滤 + 硬止损→MA288止损→三段式止盈→趋势反转")
    add(f"> 全样本：{len(trades)} 笔，复利 {comp([t['ret_pct'] for t in trades]):+.1f}%")
    add("")

    # ===== 一、分年度盈亏 =====
    add("## 一、分年度盈亏")
    add("")
    add("| 年份 | 笔数 | 胜率 | 简单% | 复利% | 平均盈利% | 平均亏损% | 盈亏比 | 平均持仓bar | 平均MFE% |")
    add("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
    for year, yts in by_year.items():
        s = stats(yts)
        rr = abs(s["avg_win"] / s["avg_loss"]) if s["avg_loss"] != 0 else float("inf")
        add(f"| {year} | {s['n']} | {s['win_rate']:.1f} | {s['simple_pct']:+.1f} | {s['compound_pct']:+.1f} | "
            f"{s['avg_win']:+.2f} | {s['avg_loss']:+.2f} | {rr:.2f} | {s['avg_hold']:.0f} | {s['avg_mfe']:+.2f} |")
    add("")

    # ===== 二、分年度离场原因 =====
    add("## 二、分年度离场原因")
    add("")
    add("| 年份 | 离场原因 | 笔数 | 占比 | 总收益% | 平均收益% |")
    add("|---|---|---:|---:|---:|---:|")
    for year, yts in by_year.items():
        by_reason = defaultdict(list)
        for t in yts:
            by_reason[t["reason"]].append(t)
        for reason, rts in sorted(by_reason.items(), key=lambda x: -len(x[1])):
            total_ret = sum(t["ret_pct"] for t in rts)
            avg_ret = total_ret / len(rts)
            pct = len(rts) / len(yts) * 100.0
            add(f"| {year} | {reason} | {len(rts)} | {pct:.1f} | {total_ret:+.1f} | {avg_ret:+.2f} |")
    add("")

    # ===== 三、盈利单离场后走势 =====
    add("## 三、盈利单离场后走势（价格继续朝原方向走多远）")
    add("")
    add("| 年份 | 盈利单数 | post20 最大% | post50 最大% | post100 最大% | 平均盈利% | 平均持仓bar |")
    add("|---|---:|---:|---:|---:|---:|---:|")
    for year, yts in by_year.items():
        wins = [t for t in yts if t["ret_pct"] > 0]
        if not wins:
            add(f"| {year} | 0 | — | — | — | — | — |")
            continue
        p20 = sum(t["post_20"] for t in wins) / len(wins)
        p50 = sum(t["post_50"] for t in wins) / len(wins)
        p100 = sum(t["post_100"] for t in wins) / len(wins)
        avg_r = sum(t["ret_pct"] for t in wins) / len(wins)
        avg_h = sum(t["hold_bars"] for t in wins) / len(wins)
        add(f"| {year} | {len(wins)} | {p20:+.2f} | {p50:+.2f} | {p100:+.2f} | {avg_r:+.2f} | {avg_h:.0f} |")
    add("")

    # ===== 四、亏损单离场后走势 =====
    add("## 四、亏损单离场后走势（价格继续朝原方向走多远）")
    add("")
    add("| 年份 | 亏损单数 | post20 最大% | post50 最大% | post100 最大% | 平均亏损% | 平均持仓bar |")
    add("|---|---:|---:|---:|---:|---:|---:|")
    for year, yts in by_year.items():
        losses = [t for t in yts if t["ret_pct"] <= 0]
        if not losses:
            add(f"| {year} | 0 | — | — | — | — | — |")
            continue
        p20 = sum(t["post_20"] for t in losses) / len(losses)
        p50 = sum(t["post_50"] for t in losses) / len(losses)
        p100 = sum(t["post_100"] for t in losses) / len(losses)
        avg_r = sum(t["ret_pct"] for t in losses) / len(losses)
        avg_h = sum(t["hold_bars"] for t in losses) / len(losses)
        add(f"| {year} | {len(losses)} | {p20:+.2f} | {p50:+.2f} | {p100:+.2f} | {avg_r:+.2f} | {avg_h:.0f} |")
    add("")

    # ===== 五、盈利单入场质量（MFE 分布）=====
    add("## 五、盈利单入场质量（MFE 分布）")
    add("")
    add("| 年份 | 盈利单数 | MFE<1% | MFE 1-4% | MFE 4-15% | MFE≥15% | 平均MFE% |")
    add("|---|---:|---:|---:|---:|---:|---:|")
    for year, yts in by_year.items():
        wins = [t for t in yts if t["ret_pct"] > 0]
        if not wins:
            add(f"| {year} | 0 | — | — | — | — | — |")
            continue
        lt1 = sum(1 for t in wins if t["mfe_pct"] < 1.0)
        lt4 = sum(1 for t in wins if 1.0 <= t["mfe_pct"] < 4.0)
        lt15 = sum(1 for t in wins if 4.0 <= t["mfe_pct"] < 15.0)
        ge15 = sum(1 for t in wins if t["mfe_pct"] >= 15.0)
        avg_mfe = sum(t["mfe_pct"] for t in wins) / len(wins)
        add(f"| {year} | {len(wins)} | {lt1} | {lt4} | {lt15} | {ge15} | {avg_mfe:+.2f} |")
    add("")

    # ===== 六、亏损单入场质量（MFE 分布）=====
    add("## 六、亏损单入场质量（MFE 分布）")
    add("")
    add("| 年份 | 亏损单数 | MFE<0.3%（入场即错） | MFE 0.3-1% | MFE 1-4% | MFE≥4%（曾盈利但亏出） | 平均MFE% |")
    add("|---|---:|---:|---:|---:|---:|---:|")
    for year, yts in by_year.items():
        losses = [t for t in yts if t["ret_pct"] <= 0]
        if not losses:
            add(f"| {year} | 0 | — | — | — | — | — |")
            continue
        lt03 = sum(1 for t in losses if t["mfe_pct"] < 0.3)
        lt1 = sum(1 for t in losses if 0.3 <= t["mfe_pct"] < 1.0)
        lt4 = sum(1 for t in losses if 1.0 <= t["mfe_pct"] < 4.0)
        ge4 = sum(1 for t in losses if t["mfe_pct"] >= 4.0)
        avg_mfe = sum(t["mfe_pct"] for t in losses) / len(losses)
        add(f"| {year} | {len(losses)} | {lt03} | {lt1} | {lt4} | {ge4} | {avg_mfe:+.2f} |")
    add("")

    # ===== 七、亏损单持仓时长分布（短持仓 = 入场即错）=====
    add("## 七、亏损单持仓时长分布")
    add("")
    add("| 年份 | 亏损单数 | ≤3bar（秒死） | 4-10bar | 11-30bar | >30bar | 平均持仓bar |")
    add("|---|---:|---:|---:|---:|---:|---:|")
    for year, yts in by_year.items():
        losses = [t for t in yts if t["ret_pct"] <= 0]
        if not losses:
            add(f"| {year} | 0 | — | — | — | — | — |")
            continue
        le3 = sum(1 for t in losses if t["hold_bars"] <= 3)
        le10 = sum(1 for t in losses if 4 <= t["hold_bars"] <= 10)
        le30 = sum(1 for t in losses if 11 <= t["hold_bars"] <= 30)
        gt30 = sum(1 for t in losses if t["hold_bars"] > 30)
        avg_h = sum(t["hold_bars"] for t in losses) / len(losses)
        add(f"| {year} | {len(losses)} | {le3} | {le10} | {le30} | {gt30} | {avg_h:.0f} |")
    add("")

    # ===== 八、盈利单 vs 亏损单分段止盈触发统计 =====
    add("## 八、分段止盈触发统计")
    add("")
    add("| 年份 | 段1（移动止盈） | 段2（MA192） | 段3（MA480） | 降级 | MA288止损 | 硬止损 | 趋势反转 |")
    add("|---|---:|---:|---:|---:|---:|---:|---:|")
    for year, yts in by_year.items():
        seg1 = sum(1 for t in yts if t["reason"] == "移动止盈")
        seg2 = sum(1 for t in yts if t["reason"] == "MA192止盈")
        seg3 = sum(1 for t in yts if t["reason"] == "MA480止盈")
        demoted = sum(1 for t in yts if t["demoted"])
        ma288 = sum(1 for t in yts if t["reason"] == "MA288止损")
        hard = sum(1 for t in yts if t["reason"] == "硬止损")
        rev = sum(1 for t in yts if t["reason"] == "趋势反转")
        add(f"| {year} | {seg1} | {seg2} | {seg3} | {demoted} | {ma288} | {hard} | {rev} |")
    add("")

    # ===== 九、大单详情（MFE ≥ 15%）=====
    add("## 九、大单详情（MFE ≥ 15%）")
    add("")
    big = [t for t in trades if t["mfe_pct"] >= 15.0]
    if big:
        add("| 入场时间 | 方向 | 收益% | MFE% | 持仓bar | 离场原因 | 降级 | post20% | post50% | post100% |")
        add("|---|---|---:|---:|---:|---|---:|---:|---:|---:|")
        for t in big:
            entry_time = datetime.fromtimestamp(bars[t["entry_idx"]].open_time / 1000, tz=BJ).strftime("%Y-%m-%d %H:%M")
            add(f"| {entry_time} | {t['side']} | {t['ret_pct']:+.2f} | {t['mfe_pct']:+.2f} | {t['hold_bars']} | "
                f"{t['reason']} | {'是' if t['demoted'] else '否'} | {t['post_20']:+.2f} | {t['post_50']:+.2f} | {t['post_100']:+.2f} |")
    else:
        add("（无 MFE ≥ 15% 的大单）")
    add("")

    # ===== 十、全局统计汇总 =====
    add("## 十、全局统计汇总")
    add("")
    all_stats = stats(trades)
    wins_all = [t for t in trades if t["ret_pct"] > 0]
    losses_all = [t for t in trades if t["ret_pct"] <= 0]
    mfe_lt03_loss = sum(1 for t in losses_all if t["mfe_pct"] < 0.3)
    mfe_lt05_loss = sum(1 for t in losses_all if t["mfe_pct"] < 0.5)

    add(f"- 总交易数：{all_stats['n']}")
    add(f"- 胜率：{all_stats['win_rate']:.1f}%")
    add(f"- 简单收益：{all_stats['simple_pct']:+.1f}%")
    add(f"- 复利收益：{all_stats['compound_pct']:+.1f}%")
    add(f"- 平均盈利：{all_stats['avg_win']:+.2f}% / 平均亏损：{all_stats['avg_loss']:+.2f}%")
    add(f"- 盈亏比：{abs(all_stats['avg_win'] / all_stats['avg_loss']):.2f}")
    add(f"- 平均持仓：{all_stats['avg_hold']:.0f} bar")
    add("")
    add(f"- 亏损单中 MFE<0.3%（入场即错）：{mfe_lt03_loss} / {len(losses_all)} = {mfe_lt03_loss/len(losses_all)*100:.1f}%")
    add(f"- 亏损单中 MFE<0.5%（几乎没走出来）：{mfe_lt05_loss} / {len(losses_all)} = {mfe_lt05_loss/len(losses_all)*100:.1f}%")
    add("")
    add(f"- 盈利单离场后 post100 平均继续走：{sum(t['post_100'] for t in wins_all)/len(wins_all):+.2f}%")
    add(f"- 亏损单离场后 post100 平均继续走：{sum(t['post_100'] for t in losses_all)/len(losses_all):+.2f}%")
    add("")

    out = os.path.join(OUT, "a11_btc_yearly_diag.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
