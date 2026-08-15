"""MA 趋势回踩策略回测（BTC/ETH/SOL）。

复用 ma_trend_pullback.py 的复刻策略参数与 SMA 计算，对 30m K 线做完整回测：
- 入场：趋势(MA288 vs MA488) + 收盘价穿越 MA288（与 analyze() 一致）。
- 平仓优先级（对齐生产 check_exit_conditions）：
    1. 硬止损（hard_stop_pct，盘中触及 hard_stop 价）
    2. MA288 止损（收盘价反向穿越 MA288）
    3. 移动止盈（盈利>=activate 后回撤>=callback）
    4. 趋势反转（MA288 与 MA488 交叉）
- 统计：交易次数、胜率、总收益、平均/最大单笔盈亏、盈亏比、最大回撤等。

数据：30m 现货 K 线（data_2026-08-13/*.csv）。未计手续费/滑点。

运行：cd F:/rust-projects/trade-test/src && python backtest.py
"""

from __future__ import annotations

import csv
import os
from datetime import datetime, timezone, timedelta
from typing import List, Dict, Any, Optional

import ma_trend_pullback as strat
from ma_trend_pullback import KlineBar, Params

BASE_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DATA_DIR = os.path.join(BASE_DIR, "data_2026-08-13")
SRC_DIR = os.path.join(BASE_DIR, "src")

CSV_30M = {
    "BTCUSDT": os.path.join(DATA_DIR, "kline_30m_202608131242_BTC.csv"),
    "ETHUSDT": os.path.join(DATA_DIR, "kline_30m_202608131245_ETH.csv"),
    "SOLUSDT": os.path.join(DATA_DIR, "kline_30m_202608131247_SOL.csv"),
}

BJ = timezone(timedelta(hours=8))  # 北京时间


def load_klines_30m(symbol: str) -> List[KlineBar]:
    """加载 30m K 线 CSV，返回升序（最旧→最新）。"""
    path = CSV_30M[symbol]
    bars: List[KlineBar] = []
    with open(path, "r", encoding="utf-8", newline="") as f:
        reader = csv.DictReader(f)
        for row in reader:
            dt = datetime.strptime(row["open_time"], "%Y-%m-%d %H:%M:%S.%f %z")
            bars.append(KlineBar(
                open_time=int(dt.timestamp() * 1000),
                open=float(row["open"]),
                high=float(row["high"]),
                low=float(row["low"]),
                close=float(row["close"]),
                volume=float(row["volume"]),
            ))
    bars.reverse()  # CSV 最新在前 → 升序
    return bars


def fmt_time(ms: int) -> str:
    return datetime.fromtimestamp(ms / 1000, tz=BJ).strftime("%Y-%m-%d %H:%M")


def backtest(symbol: str, params: Params, bars: List[KlineBar]) -> List[Dict[str, Any]]:
    """回测单币种，返回交易列表。"""
    fast = params.fast_ma_period
    slow = params.slow_ma_period
    n = len(bars)
    closes = [b.close for b in bars]

    # 前缀和，O(1) 算 SMA
    prefix = [0.0] * (n + 1)
    for i in range(n):
        prefix[i + 1] = prefix[i] + closes[i]

    def sma_at(idx: int, period: int) -> Optional[float]:
        if idx + 1 < period:
            return None
        return (prefix[idx + 1] - prefix[idx + 1 - period]) / period

    trades: List[Dict[str, Any]] = []
    pos: Optional[Dict[str, Any]] = None  # side/entry_price/entry_time/hard_stop_price/max_profit

    for i in range(n):
        if i + 1 < slow:  # 需 slow 根才能算慢速 MA
            continue

        close = closes[i]
        prev_close = closes[i - 1]
        fast_ma = sma_at(i, fast)
        slow_ma = sma_at(i, slow)
        prev_fast_ma = sma_at(i - 1, fast)

        # ---- 持仓中：检查平仓（优先级：硬止损 > MA288止损 > 移动止盈 > 趋势反转） ----
        if pos is not None:
            bar = bars[i]
            side = pos["side"]
            entry = pos["entry_price"]
            # 用收盘价更新当前盈亏与最大盈利
            pnl = (close - entry) / entry * 100.0 if side == "LONG" else (entry - close) / entry * 100.0
            pos["max_profit"] = max(pos["max_profit"], pnl)

            exit_price: Optional[float] = None
            reason = ""

            # 1. 硬止损（盘中触及）
            if params.hard_stop_pct > 0.0:
                if side == "LONG" and bar.low <= pos["hard_stop_price"]:
                    exit_price, reason = pos["hard_stop_price"], "硬止损"
                elif side == "SHORT" and bar.high >= pos["hard_stop_price"]:
                    exit_price, reason = pos["hard_stop_price"], "硬止损"

            # 2. MA288 止损（收盘价反向穿越 MA288）
            if exit_price is None and params.stop_mode == "ma288" and prev_fast_ma is not None:
                if side == "LONG" and prev_close > prev_fast_ma and close < fast_ma:
                    exit_price, reason = close, "MA288止损"
                elif side == "SHORT" and prev_close < prev_fast_ma and close > fast_ma:
                    exit_price, reason = close, "MA288止损"

            # 3. 移动止盈
            if exit_price is None and params.take_profit_mode == "trailing":
                if pos["max_profit"] >= params.trailing_activate_pct:
                    if pos["max_profit"] - pnl >= params.trailing_callback_pct:
                        exit_price, reason = close, "移动止盈"

            # 4. 趋势反转 — 已禁用（生产系统无此逻辑）
            # if exit_price is None:
            #     if side == "LONG" and fast_ma < slow_ma:
            #         exit_price, reason = close, "趋势反转"
            #     elif side == "SHORT" and fast_ma > slow_ma:
            #         exit_price, reason = close, "趋势反转"

            if exit_price is not None:
                ret = (exit_price - entry) / entry if side == "LONG" else (entry - exit_price) / entry
                trades.append({
                    "symbol": symbol,
                    "side": "多" if side == "LONG" else "空",
                    "entry_time": fmt_time(pos["entry_time"]),
                    "exit_time": fmt_time(bars[i].open_time),
                    "entry_price": entry,
                    "exit_price": exit_price,
                    "ret": ret,
                    "ret_pct": ret * 100.0,
                    "reason": reason,
                    "bars": i - pos["entry_idx"],
                })
                pos = None
                continue  # 平仓后本根不再开新仓

        # ---- 无持仓：检查入场 ----
        if pos is None and fast_ma is not None and slow_ma is not None and prev_fast_ma is not None:
            if fast_ma > slow_ma:  # 多头趋势：收盘上穿 MA288
                if prev_close < prev_fast_ma and close > fast_ma:
                    hard_stop = close * (1.0 - params.hard_stop_pct / 100.0) if params.hard_stop_pct > 0.0 else fast_ma * 0.98
                    pos = {
                        "side": "LONG", "entry_price": close,
                        "entry_time": bars[i].open_time, "entry_idx": i,
                        "hard_stop_price": hard_stop, "max_profit": 0.0,
                    }
            elif fast_ma < slow_ma:  # 空头趋势：收盘下穿 MA288
                if prev_close > prev_fast_ma and close < fast_ma:
                    hard_stop = close * (1.0 + params.hard_stop_pct / 100.0) if params.hard_stop_pct > 0.0 else fast_ma * 1.02
                    pos = {
                        "side": "SHORT", "entry_price": close,
                        "entry_time": bars[i].open_time, "entry_idx": i,
                        "hard_stop_price": hard_stop, "max_profit": 0.0,
                    }

    # 末尾仍持仓：用最后一根 close 平仓
    if pos is not None:
        entry = pos["entry_price"]
        side = pos["side"]
        exit_price = closes[-1]
        ret = (exit_price - entry) / entry if side == "LONG" else (entry - exit_price) / entry
        trades.append({
            "symbol": symbol, "side": "多" if side == "LONG" else "空",
            "entry_time": fmt_time(pos["entry_time"]),
            "exit_time": fmt_time(bars[-1].open_time),
            "entry_price": entry, "exit_price": exit_price,
            "ret": ret, "ret_pct": ret * 100.0, "reason": "持仓到结束", "bars": n - 1 - pos["entry_idx"],
        })

    return trades


def compute_metrics(trades: List[Dict[str, Any]]) -> Dict[str, Any]:
    """计算统计指标。"""
    n = len(trades)
    if n == 0:
        return {"n": 0, "win_rate": 0.0, "total_ret": 0.0, "max_drawdown": 0.0}

    wins = [t for t in trades if t["ret"] > 0]
    losses = [t for t in trades if t["ret"] <= 0]
    win_rate = len(wins) / n * 100.0

    rets = [t["ret"] for t in trades]
    total_ret = sum(rets) * 100.0  # 简单相加收益率 %
    avg_ret = total_ret / n

    max_win = max(rets) * 100.0
    max_loss = min(rets) * 100.0

    avg_win = sum(t["ret"] for t in wins) / len(wins) * 100.0 if wins else 0.0
    avg_loss = sum(t["ret"] for t in losses) / len(losses) * 100.0 if losses else 0.0
    payoff_ratio = avg_win / abs(avg_loss) if avg_loss != 0 else float("inf")
    profit_factor = (sum(t["ret"] for t in wins)) / abs(sum(t["ret"] for t in losses)) if losses else float("inf")

    # 复利资金曲线 + 最大回撤
    equity = 1.0
    peak = 1.0
    max_dd = 0.0
    for r in rets:
        equity *= (1.0 + r)
        peak = max(peak, equity)
        dd = (peak - equity) / peak
        max_dd = max(max_dd, dd)

    # 平仓原因分布
    reason_cnt: Dict[str, int] = {}
    for t in trades:
        reason_cnt[t["reason"]] = reason_cnt.get(t["reason"], 0) + 1

    avg_bars = sum(t["bars"] for t in trades) / n

    return {
        "n": n,
        "win_rate": win_rate,
        "total_ret": total_ret,
        "compound_ret": (equity - 1.0) * 100.0,
        "avg_ret": avg_ret,
        "max_win": max_win,
        "max_loss": max_loss,
        "avg_win": avg_win,
        "avg_loss": avg_loss,
        "payoff_ratio": payoff_ratio,
        "profit_factor": profit_factor,
        "max_drawdown": max_dd * 100.0,
        "avg_bars": avg_bars,
        "reason_cnt": reason_cnt,
    }


def main() -> int:
    rows = []
    for symbol in ["BTCUSDT", "ETHUSDT", "SOLUSDT"]:
        params = strat.SYMBOL_PARAMS[symbol]
        bars = load_klines_30m(symbol)
        trades = backtest(symbol, params, bars)
        m = compute_metrics(trades)
        rows.append((symbol, params, bars, trades, m))

    from collections import defaultdict

    # 打印汇总
    print("=" * 110)
    print("MA 趋势回踩策略回测报告（30m，现货数据，未计手续费/滑点）")
    print("=" * 110)
    print()
    print(f"{'币种':8} {'交易数':>6} {'胜率':>8} {'总收益%':>9} {'复利%':>9} {'最大回撤%':>10} "
          f"{'盈亏比':>7} {'利润因子':>8} {'最大盈利%':>9} {'最大亏损%':>9}")
    print("-" * 110)
    for symbol, params, bars, trades, m in rows:
        pr = f"{m['payoff_ratio']:.2f}" if m['payoff_ratio'] != float('inf') else "∞"
        pf = f"{m['profit_factor']:.2f}" if m['profit_factor'] != float('inf') else "∞"
        print(f"{symbol:8} {m['n']:>6} {m['win_rate']:>7.1f}% {m['total_ret']:>8.2f}% {m['compound_ret']:>8.2f}% "
              f"{m['max_drawdown']:>9.2f}% {pr:>7} {pf:>8} {m['max_win']:>8.2f}% {m['max_loss']:>8.2f}%")

    # 分年度收益（三币种合并，简单相加）
    print()
    print("分年度收益（三币种合并，简单相加 %）：")
    year_ret = defaultdict(float)
    year_cnt = defaultdict(int)
    for symbol, params, bars, trades, m in rows:
        for t in trades:
            y = t["entry_time"][:4]
            year_ret[y] += t["ret_pct"]
            year_cnt[y] += 1
    for y in sorted(year_ret):
        print(f"  {y}: {year_ret[y]:+8.2f}%  ({year_cnt[y]} 笔)")

    # 写 markdown 报告
    md = ["# MA 趋势回踩策略回测报告", ""]
    md.append("- 数据：30m 现货 K 线（`data_2026-08-13/`），未计手续费/滑点")
    md.append("- 策略：趋势 MA288 vs MA488 + 收盘穿越 MA288 入场；平仓优先级 硬止损 → MA288止损 → 移动止盈 → 趋势反转")
    md.append("- 口径说明：**总收益(简单)**=每笔收益率直接相加；**总收益(复利)**=按资金曲线连乘；两者差异反映连续亏损的复利损耗")
    md.append("")
    md.append("## 汇总")
    md.append("")
    md.append("| 币种 | 交易数 | 胜率 | 总收益(简单) | 总收益(复利) | 最大回撤 | 盈亏比(avg) | 利润因子 | 最大盈利 | 最大亏损 | 平均持仓(bar) |")
    md.append("|---|---|---|---|---|---|---|---|---|---|---|")
    for symbol, params, bars, trades, m in rows:
        pr = f"{m['payoff_ratio']:.2f}" if m['payoff_ratio'] != float('inf') else "∞"
        pf = f"{m['profit_factor']:.2f}" if m['profit_factor'] != float('inf') else "∞"
        md.append(f"| {symbol} | {m['n']} | {m['win_rate']:.1f}% | {m['total_ret']:.2f}% | {m['compound_ret']:.2f}% | "
                  f"{m['max_drawdown']:.2f}% | {pr} | {pf} | {m['max_win']:.2f}% | {m['max_loss']:.2f}% | {m['avg_bars']:.1f} |")
    md.append("")

    # 分年度收益表
    md.append("## 分年度收益（三币种合并，简单相加）")
    md.append("")
    md.append("| 年份 | 收益% | 笔数 |")
    md.append("|---|---|---|")
    for y in sorted(year_ret):
        md.append(f"| {y} | {year_ret[y]:+.2f}% | {year_cnt[y]} |")
    md.append("")

    # 逐笔明细
    for symbol, params, bars, trades, m in rows:
        md.append(f"## {symbol}（{m['n']} 笔）")
        md.append("")
        md.append("| # | 方向 | 入场时间 | 出场时间 | 入场价 | 出场价 | 收益% | 平仓原因 |")
        md.append("|---|---|---|---|---|---|---|---|")
        for i, t in enumerate(trades, 1):
            md.append(f"| {i} | {t['side']} | {t['entry_time']} | {t['exit_time']} | "
                      f"{t['entry_price']:.4f} | {t['exit_price']:.4f} | {t['ret_pct']:+.2f}% | {t['reason']} |")
        md.append("")

    md_path = os.path.join(SRC_DIR, "backtest_report.md")
    with open(md_path, "w", encoding="utf-8") as f:
        f.write("\n".join(md))

    print()
    print(f"[written] {md_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
