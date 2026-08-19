r"""A12 MTF 策略逐笔诊断：回答 2017-2022 为什么差。

输出：
  1) feature_report/mtf_trades.csv / mtf_trades.json —— A12 全样本逐笔交易 + MFE/MAE + 离场后漂移
  2) feature_report/mtf_yearly_diag.md           —— 分年度盈亏诊断 + 离场后漂移 + 入场条件缺失统计

运行：
  cd D:\dev-projects\rust-trade\trade-test\src
  python study_mtf_yearly_diag.py
"""
from __future__ import annotations

import csv
import json
import math
import os
from collections import defaultdict
from datetime import datetime, timezone, timedelta
from typing import Dict, List, Any

import data_config as dc
from loader import load_klines_30m, load_klines_4h
from study_adaptive_ma_trailing import comp, precompute
from study_mtf_all_coins import backtest_mtf_hold

BJ = timezone(timedelta(hours=8))
SRC = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(SRC, "feature_report")
HORIZONS = [20, 50, 100]
ACTIVATE = 4.0


def max_drawdown(rets: List[float]) -> float:
    eq = 1.0
    peak = 1.0
    dd = 0.0
    for r in rets:
        eq *= (1.0 + r / 100.0)
        peak = max(peak, eq)
        dd = max(dd, (peak - eq) / peak)
    return dd * 100.0


def post_exit_favorable(bars, exit_idx: int, side: str, horizon: int) -> float:
    """离场后 horizon 根 30m 内，行情继续朝原持仓方向走的幅度（相对离场收盘价 %）。"""
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
    out = []
    for t in trades:
        row = dict(t)
        for h in HORIZONS:
            row[f"post_exit_{h}"] = post_exit_favorable(
                bars30, int(t["exit_idx"]), t["side"], h)
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
        return {"n": 0, "win_rate": 0.0, "simple": 0.0, "compound": 0.0,
                "avg_win": 0.0, "avg_loss": 0.0, "max_win": 0.0, "max_loss": 0.0,
                "mfe_ge_act": 0, "mfe_ge_act_loss": 0, "mfe_ge_act_ret_le_05": 0,
                "loss_mfe_lt_05": 0}
    rets = [t["ret_pct"] for t in ts]
    wins = [t["ret_pct"] for t in ts if t["ret_pct"] > 0]
    losses = [t["ret_pct"] for t in ts if t["ret_pct"] <= 0]
    mfe_ge = [t for t in ts if t["mfe_pct"] >= ACTIVATE]
    mfe_ge_loss = [t for t in mfe_ge if t["ret_pct"] <= 0]
    mfe_ge_ret_le_05 = [t for t in mfe_ge if t["ret_pct"] <= 0.5]
    loss_mfe_lt_05 = [t for t in ts if t["ret_pct"] <= 0 and t["mfe_pct"] < 0.5]
    return {
        "n": n,
        "win_rate": len(wins) / n * 100.0,
        "simple": sum(rets),
        "compound": comp(rets),
        "avg_win": sum(wins) / len(wins) if wins else 0.0,
        "avg_loss": sum(losses) / len(losses) if losses else 0.0,
        "max_win": max(rets),
        "max_loss": min(rets),
        "mfe_ge_act": len(mfe_ge),
        "mfe_ge_act_loss": len(mfe_ge_loss),
        "mfe_ge_act_ret_le_05": len(mfe_ge_ret_le_05),
        "loss_mfe_lt_05": len(loss_mfe_lt_05),
    }


def missed_entry_stats(bars, params) -> Dict[int, Dict[str, int]]:
    """统计每年有多少 bar 满足/不满足各类入场条件，定位大行情没吃到的原因。"""
    pre = precompute(bars)
    closes = pre["closes"]
    vol48 = pre["vol48"]
    ma288 = pre["ma288"]
    ma480 = pre["ma480"]
    n = len(bars)
    years = [datetime.fromtimestamp(b.open_time / 1000).year for b in bars]
    slow = params.slow_ma_period
    thr = params.realized_vol_threshold
    agg: Dict[int, Dict[str, int]] = defaultdict(lambda: defaultdict(int))

    for i in range(slow - 1, n):
        y = years[i]
        a = agg[y]
        f = ma288[i]
        s = ma480[i]
        pf = ma288[i - 1]
        if f is None or s is None or pf is None:
            continue
        c = closes[i]
        pc = closes[i - 1]
        a["bars_ready"] += 1
        trend_up = f > s
        trend_down = f < s
        cross_up = pc < pf and c > f
        cross_down = pc > pf and c < f
        vol_skip = vol48[i] is not None and vol48[i] >= thr

        if trend_up:
            a["bars_trend_up"] += 1
        if trend_down:
            a["bars_trend_down"] += 1
        if cross_up:
            a["cross_up"] += 1
        if cross_down:
            a["cross_down"] += 1
        if cross_up and trend_up:
            a["cross_up_trend"] += 1
        if cross_down and trend_down:
            a["cross_down_trend"] += 1
        if cross_up and not trend_up:
            a["cross_up_no_trend"] += 1
        if cross_down and not trend_down:
            a["cross_down_no_trend"] += 1
        if ((cross_up and trend_up) or (cross_down and trend_down)) and vol_skip:
            a["cross_trend_vol_skip"] += 1
        if ((cross_up and not trend_up) or (cross_down and not trend_down)) and not vol_skip:
            a["cross_no_trend_vol_ok"] += 1
    return {y: dict(a) for y, a in sorted(agg.items())}


def main() -> int:
    os.makedirs(OUT, exist_ok=True)
    all_trades: List[Dict] = []
    md: List[str] = []
    add = md.append
    add("# A12 MTF 逐笔诊断（2017-2026）")
    add("")
    add("> 口径：30m 入场（MA288>MA480 + 收盘穿越 MA288）+ 4h MA40 止损 + 移动止盈 4%+1% + 硬止损。")
    add("> 未计手续费/滑点。`year` 为出场年份。")
    add("")

    for coin in dc.SYMBOLS:
        params = dc.SYMBOL_PARAMS[coin]
        bars30 = load_klines_30m(coin)
        bars4 = load_klines_4h(coin)
        trades = backtest_mtf_hold(coin, params, bars30, bars4, 40, 4.0, 1.0)
        trades = enrich_trades(bars30, trades)
        all_trades.extend(trades)

        by_year = yearly_agg(trades)
        add(f"## {coin}")
        add("")
        add("### 分年度盈亏 + 大浮盈回吐诊断")
        add("")
        add("| 年份 | 笔数 | 胜率 | 简单% | 复利% | 平均盈利% | 平均亏损% | 最大盈利% | 最大亏损% | 达到4%激活 | 其中亏损出场 | 其中<=0.5%出场 | 亏损单中MFE<0.5% |")
        add("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
        for y, ts in by_year.items():
            st = stats(ts)
            add(f"| {y} | {st['n']} | {st['win_rate']:.1f} | {st['simple']:+.1f} | {st['compound']:+.1f} | "
                f"{st['avg_win']:+.2f} | {st['avg_loss']:+.2f} | {st['max_win']:+.2f} | {st['max_loss']:+.2f} | "
                f"{st['mfe_ge_act']} | {st['mfe_ge_act_loss']} | {st['mfe_ge_act_ret_le_05']} | {st['loss_mfe_lt_05']} |")
        add("")

        add("### 离场后行情继续走多远（按原持仓方向，%相对离场收盘价）")
        add("")
        add("| 年份 | 全部交易 post20 | post50 | post100 | 盈利单 post20 | post50 | post100 | 亏损单 post20 | post50 | post100 |")
        add("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
        for y, ts in by_year.items():
            wins = [t for t in ts if t["ret_pct"] > 0]
            losses = [t for t in ts if t["ret_pct"] <= 0]
            def avg(vals, k):
                return sum(t[k] for t in vals) / len(vals) if vals else 0.0
            add(f"| {y} | {avg(ts, 'post_exit_20'):+.2f} | {avg(ts, 'post_exit_50'):+.2f} | {avg(ts, 'post_exit_100'):+.2f} | "
                f"{avg(wins, 'post_exit_20'):+.2f} | {avg(wins, 'post_exit_50'):+.2f} | {avg(wins, 'post_exit_100'):+.2f} | "
                f"{avg(losses, 'post_exit_20'):+.2f} | {avg(losses, 'post_exit_50'):+.2f} | {avg(losses, 'post_exit_100'):+.2f} |")
        add("")

        add("### 入场条件缺失统计（慢线就绪后的 30m bar 数）")
        add("")
        me = missed_entry_stats(bars30, params)
        add("| 年份 | 可交易bar | MA288>MA480 | MA288<MA480 | 上穿MA288 | 下穿MA288 | 上穿+多头趋势 | 下穿+空头趋势 | 上穿无趋势 | 下穿无趋势 | 趋势穿越被vol过滤 | 无趋势穿越且vol未过滤 |")
        add("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
        for y, r in me.items():
            add(f"| {y} | {r.get('bars_ready',0)} | {r.get('bars_trend_up',0)} | {r.get('bars_trend_down',0)} | "
                f"{r.get('cross_up',0)} | {r.get('cross_down',0)} | {r.get('cross_up_trend',0)} | {r.get('cross_down_trend',0)} | "
                f"{r.get('cross_up_no_trend',0)} | {r.get('cross_down_no_trend',0)} | "
                f"{r.get('cross_trend_vol_skip',0)} | {r.get('cross_no_trend_vol_ok',0)} |")
        add("")

    # 写逐笔 CSV
    csv_path = os.path.join(OUT, "mtf_trades.csv")
    fields = [
        "symbol", "year", "side", "entry_time", "exit_time", "entry_idx", "exit_idx",
        "ret_pct", "reason", "mfe_pct", "mae_pct", "hold_bars",
        "post_exit_20", "post_exit_50", "post_exit_100",
    ]
    with open(csv_path, "w", encoding="utf-8", newline="") as f:
        w = csv.DictWriter(f, fieldnames=fields, extrasaction="ignore")
        w.writeheader()
        for t in all_trades:
            w.writerow({k: t.get(k) for k in fields})
    json_path = os.path.join(OUT, "mtf_trades.json")
    with open(json_path, "w", encoding="utf-8") as f:
        json.dump(all_trades, f, ensure_ascii=False)

    md.append("---")
    md.append("")
    md.append("### 大浮盈回吐 / 入场即错 的判定口径")
    md.append("- `达到4%激活`：mfe_pct >= 4%（移动止盈激活线）。")
    md.append("- `其中亏损出场`：达到激活线但最终 ret_pct <= 0。")
    md.append("- `其中<=0.5%出场`：达到激活线但最终 ret_pct <= 0.5%（大浮盈几乎吐光）。")
    md.append("- `亏损单中MFE<0.5%`：亏损单里最大浮盈从未超过 0.5%，即“入场就没走出来”。")
    md.append("- `离场后行情`：离场后 20/50/100 根 30m 内，价格朝原持仓方向继续走的幅度。")
    md_path = os.path.join(OUT, "mtf_yearly_diag.md")
    with open(md_path, "w", encoding="utf-8") as f:
        f.write("\n".join(md))

    print(f"[written] {md_path}")
    print(f"[written] {csv_path}")
    print(f"[written] {json_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
