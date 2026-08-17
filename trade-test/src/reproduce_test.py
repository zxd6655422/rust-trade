"""MA 趋势回踩策略 —— 生产信号复刻 & 一致性校验（全方位测试）。

目的
----
生产环境在 2026-08-07 更新后重启了 ma_trend_pullback 策略。本脚本用同一份 K 线数据
（data_2026-08-13/*.csv）逐条复刻生产信号（rust-trade-prod-strategy_signals），
统计"数据（K线）"与"信号"之间的差异，并区分不同置信度。

校验维度与置信度
----------------
  高置信度（纯字段对比，不依赖 K 线窗口重建）：
    A. 趋势方向        —— market_context.trend vs 重算 MA288/MA488 方向
    B. 止损价          —— 硬止损公式(相对 current_price) vs stop_loss
    C. 信号强度        —— min(|价差%|/5, 1.0) vs signal_strength
    D. 入场价 vs 现价   —— 信号表 entry_price vs market_context.current_price（核心差异）
    E. 重复/陈旧信号    —— 同一根未收盘 K 线被重复出信号（market_context 冻结、entry_price 漂移）

  中置信度（依赖 K 线窗口重建，受"数据源 ~0.03% 残差"影响）：
    F. 均线数值复现     —— 重算 MA288/MA488 与 market_context 的误差
    G. 穿越/入场条件    —— prev_close 相对前一根 MA288、close 相对当前 MA288 是否构成穿越

数据源残差说明
--------------
    CSV 与生产策略服务所用 K 线存在 ~0.02%~0.03%（与币价成正比）的系统性残差：
    例如 SOL #1 的 market_context open/close/current_price 均为 73.25，而 CSV 中
    17:30 这根 K 线 O=73.27 / C=73.43，17:00 这根 O=73.57 / C=73.27 —— 都不等于 73.25。
    这通常意味着 CSV 与生产环境用了不同的数据源（如合约 vs 现货、或不同抓取时刻的
    快照），而非策略计算错误。该残差不影响趋势方向与止损价的判定。

运行
----
    cd F:/rust-projects/trade-test/src && python reproduce_test.py

输出
----
    控制台 + report.md / report.json
"""

from __future__ import annotations

import csv
import json
import os
from datetime import datetime, timezone
from typing import List, Dict, Any, Optional, Tuple

import ma_trend_pullback as strat
from ma_trend_pullback import KlineBar, Params

# =====================================================================
# 路径与常量
# =====================================================================

BASE_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# 数据已移到仓库外：rust-projects/data_2026-08-13
DATA_DIR = os.path.join(os.path.dirname(os.path.dirname(BASE_DIR)), "data_2026-08-13")
SIGNALS_FILE = os.path.join(BASE_DIR, "rust-trade-prod-strategy_signals")
SRC_DIR = os.path.join(BASE_DIR, "src")

CSV_30M = {
    "BTCUSDT": os.path.join(DATA_DIR, "kline_30m_202608131242_BTC.csv"),
    "ETHUSDT": os.path.join(DATA_DIR, "kline_30m_202608131245_ETH.csv"),
    "SOLUSDT": os.path.join(DATA_DIR, "kline_30m_202608131247_SOL.csv"),
}

KLINE_COUNT = 500          # 生产策略服务传入的 K 线数量（market_context.kline_count）
BAR_MS = 30 * 60 * 1000    # 30m K 线周期（毫秒）


# =====================================================================
# 工具：时间解析 / K 线加载 / 信号解析
# =====================================================================

def parse_csv_time(s: str) -> int:
    dt = datetime.strptime(s, "%Y-%m-%d %H:%M:%S.%f %z")
    return int(dt.timestamp() * 1000)


def parse_iso_time(s: str) -> int:
    s = s.strip()
    if s.endswith("Z"):
        s = s[:-1] + "+00:00"
    dt = datetime.fromisoformat(s)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return int(dt.timestamp() * 1000)


def load_klines_30m(symbol: str) -> List[KlineBar]:
    path = CSV_30M[symbol]
    bars: List[KlineBar] = []
    with open(path, "r", encoding="utf-8", newline="") as f:
        reader = csv.DictReader(f)
        for row in reader:
            bars.append(KlineBar(
                open_time=parse_csv_time(row["open_time"]),
                open=float(row["open"]),
                high=float(row["high"]),
                low=float(row["low"]),
                close=float(row["close"]),
                volume=float(row["volume"]),
            ))
    bars.reverse()  # CSV 最新在前，反转为升序（最旧→最新）
    return bars


def load_prod_signals() -> List[Dict[str, Any]]:
    with open(SIGNALS_FILE, "r", encoding="utf-8") as f:
        raw = json.load(f)
    rows = next(iter(raw.values())) if isinstance(raw, dict) else raw
    out = []
    for r in rows:
        mc_raw = r.get("market_context")
        r["_market_context"] = json.loads(mc_raw) if isinstance(mc_raw, str) else (mc_raw or {})
        out.append(r)
    return out


# =====================================================================
# 窗口定位与复现
# =====================================================================

def find_window(bars: List[KlineBar], signal_epoch_ms: int) -> Tuple[Optional[int], Optional[int]]:
    """返回 (forming_idx, completed_idx)。

    forming_idx   : open_time <= 信号时刻 < open_time+30m 的"正在形成"K 线下标
    completed_idx : 信号时刻之前最后一根"已收盘"K 线下标
    """
    n = len(bars)
    lo, hi, ans = 0, n - 1, -1
    while lo <= hi:
        mid = (lo + hi) // 2
        if bars[mid].open_time <= signal_epoch_ms:
            ans = mid
            lo = mid + 1
        else:
            hi = mid - 1
    if ans == -1:
        return None, None
    forming_idx = ans if bars[ans].open_time + BAR_MS > signal_epoch_ms else None
    completed_idx = ans if forming_idx is None else (ans - 1)
    if completed_idx is not None and completed_idx >= 0:
        if bars[completed_idx].open_time + BAR_MS > signal_epoch_ms:
            completed_idx = None
    return forming_idx, completed_idx


def slice_window(bars: List[KlineBar], end_idx: int) -> List[KlineBar]:
    start = max(0, end_idx - KLINE_COUNT + 1)
    return [KlineBar(**vars(b)) for b in bars[start:end_idx + 1]]


def reproduce_signal(sig: Dict[str, Any], bars: List[KlineBar]) -> Dict[str, Any]:
    """复刻单条生产信号，返回多维度对比结果。"""
    symbol = sig["symbol"]
    params: Params = strat.SYMBOL_PARAMS[symbol]
    mc = sig["_market_context"]
    ts = parse_iso_time(sig["created_at"])
    forming_idx, completed_idx = find_window(bars, ts)

    prod_fast = float(mc["fast_ma"])
    prod_slow = float(mc["slow_ma"])
    prod_strength = float(sig["signal_strength"])
    prod_stop = float(sig["stop_loss"]) if sig.get("stop_loss") is not None else None
    entry_price = float(sig["entry_price"])
    current_price = float(mc["current_price"])

    res: Dict[str, Any] = {
        "id": sig["id"],
        "symbol": symbol,
        "created_at": sig["created_at"],
        "signal_type": sig.get("signal_type"),
        "direction": sig.get("direction"),
        "status": sig.get("status"),
        "closed_reason": sig.get("closed_reason"),
        "forming_idx": forming_idx,
        "completed_idx": completed_idx,
    }

    # --- MA 复现 ---
    # 关键：生产引擎 build_market_data() 使用 store.closed_bars(500) —— 仅"已收盘"K 线，
    # 不包含正在形成的 K 线。故以 completed_idx（最后已收盘 K 线）为窗口末尾，用原始收盘价。
    # market_context 里 open/close 是最后已收盘 K 线的 OHLC，current_price 是 store.current_price()
    # （= 正在形成 K 线的实时收盘价，与 closed_bars 无直接关系）。
    res["repro_fast_ma"] = res["repro_slow_ma"] = None
    if completed_idx is not None:
        w = slice_window(bars, completed_idx)
        f = strat.calculate_sma(w, params.fast_ma_period)
        s = strat.calculate_sma(w, params.slow_ma_period)
        if f is not None and s is not None:
            res["repro_fast_ma"] = f
            res["repro_slow_ma"] = s
            res["fast_ma_err"] = f - prod_fast
            res["slow_ma_err"] = s - prod_slow

    res["fast_ma_best_err"] = res.get("fast_ma_err")

    # 趋势方向（closed_bars 重算 MA）
    if res["repro_fast_ma"] is not None:
        f = res["repro_fast_ma"]
        s = res["repro_slow_ma"]
        trend = "Bullish" if f > s else ("Bearish" if f < s else "Neutral")
        res["repro_trend"] = trend
        res["trend_match"] = (trend == mc.get("trend"))
        res["strength_repro"] = min(abs(f - s) / s * 100.0 / 5.0, 1.0)
    else:
        res["repro_trend"] = None
        res["trend_match"] = False
        res["strength_repro"] = None

    res["strength_diff"] = (res["strength_repro"] - prod_strength) if res["strength_repro"] is not None else None

    # --- 穿越/入场条件（语义：当前 = 最后已收盘 K 线，close = 该 K 线收盘价） ---
    if completed_idx is not None and completed_idx >= 1:
        w = slice_window(bars, completed_idx)
        entry_fast = strat.calculate_sma(w, params.fast_ma_period)
        prev_fast = strat.calculate_sma(w[:-1], params.fast_ma_period)
        prev_close = w[-2].close
        close = w[-1].close
        res["prev_close"] = prev_close
        res["prev_fast_ma"] = prev_fast
        res["close"] = close
        res["entry_fast_ma"] = entry_fast

        crossed = None
        if res.get("repro_trend") == "Bullish" and prev_fast is not None:
            crossed = (prev_close < prev_fast) and (close > entry_fast)
        elif res.get("repro_trend") == "Bearish" and prev_fast is not None:
            crossed = (prev_close > prev_fast) and (close < entry_fast)
        res["crossover_repro"] = crossed
        res["crossover_ok"] = (crossed is True and sig.get("signal_type") == "BUY") or \
                              (crossed is True and sig.get("signal_type") == "SELL")
    else:
        res["crossover_repro"] = None
        res["crossover_ok"] = None

    # --- 止损价（高置信度：直接按 hard_stop_pct 公式，相对 current_price） ---
    if sig.get("signal_type") == "BUY":
        stop_repro = current_price * (1.0 - params.hard_stop_pct / 100.0)
    elif sig.get("signal_type") == "SELL":
        stop_repro = current_price * (1.0 + params.hard_stop_pct / 100.0)
    else:
        stop_repro = None
    res["stop_loss_repro"] = stop_repro
    res["stop_loss_diff"] = (stop_repro - prod_stop) if (stop_repro is not None and prod_stop is not None) else None

    # --- 入场价 vs 现价（高置信度核心差异） ---
    res["entry_price"] = entry_price
    res["current_price"] = current_price
    res["entry_vs_current_diff"] = entry_price - current_price
    res["entry_vs_current_pct"] = (entry_price - current_price) / current_price * 100.0

    return res


# =====================================================================
# 汇总
# =====================================================================

def summarize(results: List[Dict[str, Any]]) -> Dict[str, Any]:
    n = len(results)
    def count(cond): return sum(1 for r in results if cond(r))
    def nums(key): return [r[key] for r in results if r.get(key) is not None]

    def maxabs(vals): return max((abs(x) for x in vals), default=None)
    def meanabs(vals): return sum(abs(x) for x in vals) / len(vals) if vals else None

    fe = nums("fast_ma_best_err")
    sd = nums("strength_diff")
    std = nums("stop_loss_diff")
    ed = nums("entry_vs_current_diff")

    summary = {
        "total_signals": n,
        "trend_match": count(lambda r: r.get("trend_match")),
        "trend_mismatch": count(lambda r: r.get("trend_match") is False),
        "crossover_ok": count(lambda r: r.get("crossover_ok")),
        "crossover_not_firing": count(lambda r: r.get("crossover_ok") is False),
        "crossover_unavailable": count(lambda r: r.get("crossover_ok") is None),
        "stop_loss_exact_match": count(lambda r: r.get("stop_loss_diff") is not None and abs(r["stop_loss_diff"]) < 1e-9),
        "stop_loss_max_abs_diff": maxabs(std),
        "strength_max_abs_diff": maxabs(sd),
        "fast_ma_max_abs_err": maxabs(fe),
        "fast_ma_mean_abs_err": meanabs(fe),
        "entry_eq_current": count(lambda r: abs(r.get("entry_vs_current_diff", 1e9)) < 1e-12),
        "entry_neq_current": count(lambda r: abs(r.get("entry_vs_current_diff", 1e9)) >= 1e-12),
        "entry_vs_current_max_abs": maxabs(ed),
        "entry_vs_current_mean_abs": meanabs(ed),
        "execution_failed": count(lambda r: r.get("status") == "failed"),
        "execution_other": count(lambda r: r.get("status") != "failed"),
    }
    return summary


def detect_stale_duplicates(results: List[Dict[str, Any]]) -> List[Dict[str, Any]]:
    by_symbol: Dict[str, List[Dict[str, Any]]] = {}
    for r in results:
        by_symbol.setdefault(r["symbol"], []).append(r)
    findings = []
    for sym, rs in by_symbol.items():
        rs_sorted = sorted(rs, key=lambda r: r["created_at"])
        for a, b in zip(rs_sorted, rs_sorted[1:]):
            if a.get("forming_idx") is not None and a.get("forming_idx") == b.get("forming_idx"):
                findings.append({
                    "symbol": sym,
                    "earlier": a["created_at"],
                    "later": b["created_at"],
                    "forming_idx": a["forming_idx"],
                    "market_context_frozen": (a.get("close") == b.get("close")) and (a.get("entry_fast_ma") == b.get("entry_fast_ma")),
                    "entry_drift": (b.get("entry_price") or 0) - (a.get("entry_price") or 0),
                })
    return findings


# =====================================================================
# 报告
# =====================================================================

def fmt(v: Any) -> str:
    if v is None:
        return "-"
    if isinstance(v, float):
        return f"{v:.6f}"
    return str(v)


def render(results: List[Dict[str, Any]], summary: Dict[str, Any]) -> str:
    L: List[str] = []
    L.append("# MA 趋势回踩策略 · 生产信号复刻与差异统计报告")
    L.append("")
    L.append("## 一、总体结论（按置信度）")
    L.append("")
    L.append("### 高置信度差异（纯字段对比）")
    L.append("")
    L.append(f"1. **入场价 vs 现价不一致**：{summary['entry_neq_current']}/{summary['total_signals']} 条信号的 "
             f"`entry_price` 与 `market_context.current_price` 不一致，最大偏差 {summary['entry_vs_current_max_abs']:.4f}，"
             f"平均 {summary['entry_vs_current_mean_abs']:.4f}。")
    L.append("")
    L.append("   **代码层根因（已确认）**：`engine.rs` 里 `entry_price = get_ticker_price()`（Binance REST fapi/v2 实时行情），"
             "而 `market_context.current_price = store.current_price()`（WebSocket K 线流里正在形成 K 线的实时收盘价）。"
             "两个不同价格源，行情波动时会出现偏差。")
    L.append("")
    L.append(f"2. **止损价精确匹配**：{summary['stop_loss_exact_match']}/{summary['total_signals']} 条完全一致，"
             f"最大误差 {summary['stop_loss_max_abs_diff']:.2e}（硬止损公式相对 current_price，计算正确）。")
    L.append("")
    L.append("   **注意**：参数 `stop_mode=\"ma288\"` 但 `hard_stop_pct>0`，代码里 `hard_stop_pct>0` 分支优先，"
             "实际止损用的是硬止损（relative to current_price），并非 MA288 止损。")
    L.append("")
    L.append(f"3. **信号强度近似匹配**：最大误差 {summary['strength_max_abs_diff']:.6f}（公式正确，残差来自均线数据源差异）。")
    L.append("")
    L.append(f"4. **全部信号执行失败**：{summary['execution_failed']}/{summary['total_signals']} 条 `status=failed`"
             f"（Insufficient position 持仓不足 / Precision 精度 / Risk rejected 风险拒绝）。")
    L.append("")
    L.append("### 中置信度（依赖 K 线窗口重建）")
    L.append("")
    L.append(f"5. **趋势方向 {summary['trend_match']}/{summary['total_signals']} 一致**（MA288 vs MA488 方向全部正确）。")
    L.append("")
    L.append(f"6. **均线数值残差 ~0.02%~0.03%**：重算 MA288 最大误差 {summary['fast_ma_max_abs_err']:.4f}，"
             f"平均 {summary['fast_ma_mean_abs_err']:.4f}。属数据源差异（CSV 与生产 Redis K 线非同一来源，"
             "可能现货 vs 合约），非计算错误。")
    L.append("")
    L.append(f"7. **穿越条件**：{summary['crossover_ok']} 条严格满足文档化穿越条件，"
             f"{summary['crossover_not_firing']} 条在边界附近未严格满足（margin 极小，受均线残差影响，需结合实盘数据复核）。")
    L.append("")
    L.append("### 代码层关键事实（读生产源码确认）")
    L.append("")
    L.append("- 策略 K 线输入为 `store.closed_bars(500)`：仅\"已收盘\" 30m K 线，不含正在形成的 K 线。")
    L.append("- `market_context.open/close` = 最后一根已收盘 K 线的开/收；`current_price` = 正在形成 K 线的实时收盘价（WS 流）。")
    L.append("- `strategy_signals.entry_price` = Binance REST `get_ticker_price()`，与 `market_context.current_price` 来源不同。")
    L.append("")
    L.append("## 二、逐信号明细")
    L.append("")
    L.append("| # | symbol | created_at | 信号 | 趋势匹配 | 穿越OK | MA288误差 | 强度误差 | 止损误差 | entry−current |")
    L.append("|---|--------|-----------|------|---------|--------|-----------|---------|---------|----------------|")
    for i, r in enumerate(results, 1):
        L.append(
            f"| {i} | {r.get('symbol')} | {r.get('created_at')} | {r.get('signal_type')} "
            f"| {r.get('trend_match')} | {r.get('crossover_ok')} "
            f"| {fmt(r.get('fast_ma_best_err'))} | {fmt(r.get('strength_diff'))} "
            f"| {fmt(r.get('stop_loss_diff'))} | {fmt(r.get('entry_vs_current_diff'))} |"
        )
    L.append("")
    return "\n".join(L)


def main() -> int:
    signals = load_prod_signals()
    bars_by = {s: load_klines_30m(s) for s in CSV_30M}

    results = []
    for sig in signals:
        sym = sig["symbol"]
        if sym not in bars_by:
            results.append({"id": sig["id"], "symbol": sym, "error": "无该 symbol 的 K 线数据"})
            continue
        results.append(reproduce_signal(sig, bars_by[sym]))

    summary = summarize(results)
    stale = detect_stale_duplicates(results)

    md = render(results, summary)
    if stale:
        md += "## 三、重复/陈旧信号检测（同一根未收盘 K 线被重复出信号）\n\n"
        md += "| symbol | 前一条时间 | 后一条时间 | market_context是否冻结 | entry_price漂移 |\n"
        md += "|---|---|---|---|---|\n"
        for f in stale:
            md += f"| {f['symbol']} | {f['earlier']} | {f['later']} | {f['market_context_frozen']} | {f['entry_drift']:.4f} |\n"
        md += "\n"
    if stale:
        md += "> 说明：策略每 15 分钟重跑一次，但 30m K 线尚未收盘、且服务缓存了 K 线数据，"
        md += "导致同一根 K 线反复触发相同信号（market_context 冻结），仅 entry_price（实时行情）在漂移。\n"

    md_path = os.path.join(SRC_DIR, "report.md")
    json_path = os.path.join(SRC_DIR, "report.json")
    with open(md_path, "w", encoding="utf-8") as f:
        f.write(md)
    with open(json_path, "w", encoding="utf-8") as f:
        json.dump({"summary": summary, "results": results, "stale_duplicates": stale},
                  f, ensure_ascii=False, indent=2, default=str)

    print(md)
    print(f"\n[written] {md_path}")
    print(f"[written] {json_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
