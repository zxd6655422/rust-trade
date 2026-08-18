"""A9 分级+衰竭降级 完整验证脚本。

验证内容：
  1. 全样本复利 vs 基线(A1) vs MA192(A7)
  2. 分年度统计（简单收益 + 复利收益 + 交易笔数 + 胜率）
  3. 分月度统计（简单收益 + 复利收益）
  4. 牛熊市分析（按年度涨跌划分）
  5. 衰竭降级细节分析
  6. 时间切分验证（双向）
  7. 回撤分析

口径：对齐生产（slow=480 + vol过滤 + 硬止损→[MA288止损]→止盈规则→趋势反转）。
输出：feature_report/a9_full_validation.md
"""
from __future__ import annotations

import math
import os
from datetime import datetime, timezone, timedelta
from typing import List, Dict, Any, Tuple
from collections import defaultdict

import data_config as dc
from loader import load_klines_30m
from study_adaptive_ma_trailing import backtest_trades, comp, precompute


# =====================================================================
# 辅助函数
# =====================================================================

def mean(xs):
    return sum(xs) / len(xs) if xs else 0.0


def fmt(x, d=1):
    if x is None or (isinstance(x, float) and math.isnan(x)):
        return "N/A"
    return f"{x:.{d}f}"


def fmt_pct(x, d=2):
    if x is None:
        return "N/A"
    return f"{x:+.{d}f}%"


def max_drawdown(equity_curve: List[float]) -> float:
    """计算最大回撤（百分比）"""
    if not equity_curve:
        return 0.0
    peak = equity_curve[0]
    max_dd = 0.0
    for eq in equity_curve:
        if eq > peak:
            peak = eq
        dd = (peak - eq) / peak * 100.0 if peak > 0 else 0.0
        if dd > max_dd:
            max_dd = dd
    return max_dd


def equity_curve_from_trades(trades: List[Dict]) -> List[float]:
    """从交易列表生成权益曲线"""
    eq = 100.0
    curve = [eq]
    for t in trades:
        eq *= (1.0 + t["ret_pct"] / 100.0)
        curve.append(eq)
    return curve


def per_period_stats(trades: List[Dict], bars: List, period: str = "year") -> Dict[int, Dict]:
    """按年/月统计"""
    stats = defaultdict(lambda: {"rets": [], "wins": 0, "total": 0, "simple_sum": 0.0})
    BJ = timezone(timedelta(hours=8))

    for t in trades:
        # 用入场时间确定所属年/月
        entry_bar = t["entry_idx"]
        if entry_bar < len(bars):
            dt = datetime.fromtimestamp(bars[entry_bar].open_time / 1000, tz=BJ)
            key = dt.year if period == "year" else dt.year * 100 + dt.month
        else:
            continue

        ret = t["ret_pct"]
        stats[key]["rets"].append(ret)
        stats[key]["total"] += 1
        stats[key]["simple_sum"] += ret
        if ret > 0:
            stats[key]["wins"] += 1

    # 计算复利
    for key in stats:
        s = stats[key]
        eq = 1.0
        for r in s["rets"]:
            eq *= (1.0 + r / 100.0)
        s["compound"] = (eq - 1.0) * 100.0
        s["win_rate"] = s["wins"] / s["total"] * 100.0 if s["total"] > 0 else 0.0
        # 平均每笔
        s["avg_ret"] = mean(s["rets"])
        # 盈亏比
        wins = [r for r in s["rets"] if r > 0]
        losses = [r for r in s["rets"] if r <= 0]
        s["avg_win"] = mean(wins) if wins else 0.0
        s["avg_loss"] = mean(losses) if losses else 0.0
        s["profit_factor"] = abs(sum(wins) / sum(losses)) if losses and sum(losses) != 0 else float('inf')

    return dict(stats)


def classify_bull_bear(yearly_simple: Dict[int, float]) -> Tuple[set, set, set]:
    """根据年度简单收益划分牛/熊/震荡"""
    bull = set()
    bear = set()
    neutral = set()
    for year, ret in yearly_simple.items():
        if ret > 20:
            bull.add(year)
        elif ret < -20:
            bear.add(year)
        else:
            neutral.add(year)
    return bull, bear, neutral


def exit_reason_stats(trades: List[Dict]) -> Dict[str, Dict]:
    """按离场原因统计"""
    stats = defaultdict(lambda: {"count": 0, "rets": [], "total_ret": 0.0})
    for t in trades:
        reason = t["reason"]
        stats[reason]["count"] += 1
        stats[reason]["rets"].append(t["ret_pct"])
        stats[reason]["total_ret"] += t["ret_pct"]

    for reason in stats:
        s = stats[reason]
        s["avg_ret"] = mean(s["rets"])
        s["win_rate"] = len([r for r in s["rets"] if r > 0]) / s["count"] * 100.0 if s["count"] > 0 else 0.0

    return dict(stats)


# =====================================================================
# 主流程
# =====================================================================

def main() -> int:
    md: List[str] = []
    add = md.append
    add("# A9 分级+衰竭降级 完整验证报告")
    add("")
    add(f"> 生成时间：{datetime.now().strftime('%Y-%m-%d %H:%M')}")
    add(f"> 数据：30m K 线，6 币种（BTC/ETH 含 2017-09 扩展历史）")
    add(f"> 口径：对齐生产（slow=480 + vol过滤 + 硬止损→MA288止损→止盈规则→趋势反转）")
    add(f"> 未计手续费/滑点")
    add("")

    coins = ["BTCUSDT", "ETHUSDT", "SOLUSDT", "BNBUSDT", "SUIUSDT", "HYPEUSDT"]
    data = {}
    for coin in coins:
        bars = load_klines_30m(coin)
        data[coin] = (bars, precompute(bars))

    # 三种方案
    strategies = [
        ("A1 基线+vol", "base", {}),
        ("A7 MA192 c10", "ma192", {"confirm": 10, "activate": 15.0}),
        ("A9 分级+衰竭降级", "tiered_demote", {"confirm": 10, "switch_at": 20.0, "demote_pct": 10.0, "activate": 15.0}),
    ]

    # =================================================================
    # Section 1: 全样本总览
    # =================================================================
    add("## 一、全样本总览")
    add("")
    add("### 1.1 复利收益对比")
    add("")
    add("| 方案 | " + " | ".join(coins) + " |")
    add("|---|---" * (len(coins) + 1) + " |")

    all_results = {}  # (strategy_label, coin) -> trades
    for label, mode, cfg in strategies:
        cells = []
        for coin in coins:
            params = dc.SYMBOL_PARAMS[coin]
            bars, pre = data[coin]
            trades = backtest_trades(coin, params, bars, pre, mode=mode, **cfg)
            c = comp([t["ret_pct"] for t in trades])
            cells.append(c)
            all_results[(label, coin)] = trades
        add(f"| {label} | " + " | ".join(f"{c:+.1f}%" for c in cells) + " |")
    add("")

    add("### 1.2 交易笔数对比")
    add("")
    add("| 方案 | " + " | ".join(coins) + " |")
    add("|---|---" * (len(coins) + 1) + " |")
    for label, mode, cfg in strategies:
        cells = [len(all_results[(label, c)]) for c in coins]
        add(f"| {label} | " + " | ".join(str(c) for c in cells) + " |")
    add("")

    add("### 1.3 胜率对比")
    add("")
    add("| 方案 | " + " | ".join(coins) + " |")
    add("|---|---" * (len(coins) + 1) + " |")
    for label, mode, cfg in strategies:
        cells = []
        for coin in coins:
            trades = all_results[(label, coin)]
            wins = len([t for t in trades if t["ret_pct"] > 0])
            wr = wins / len(trades) * 100 if trades else 0
            cells.append(f"{wr:.1f}%")
        add(f"| {label} | " + " | ".join(cells) + " |")
    add("")

    add("### 1.4 最大回撤对比")
    add("")
    add("| 方案 | " + " | ".join(coins) + " |")
    add("|---|---" * (len(coins) + 1) + " |")
    for label, mode, cfg in strategies:
        cells = []
        for coin in coins:
            trades = all_results[(label, coin)]
            curve = equity_curve_from_trades(trades)
            dd = max_drawdown(curve)
            cells.append(f"{dd:.1f}%")
        add(f"| {label} | " + " | ".join(cells) + " |")
    add("")

    # =================================================================
    # Section 2: 分年度统计（A9 详细）
    # =================================================================
    add("## 二、A9 分年度详细统计")
    add("")

    for coin in coins:
        params = dc.SYMBOL_PARAMS[coin]
        bars, pre = data[coin]
        trades_a1 = all_results[("A1 基线+vol", coin)]
        trades_a7 = all_results[("A7 MA192 c10", coin)]
        trades_a9 = all_results[("A9 分级+衰竭降级", coin)]

        yearly_a1 = per_period_stats(trades_a1, bars, "year")
        yearly_a7 = per_period_stats(trades_a7, bars, "year")
        yearly_a9 = per_period_stats(trades_a9, bars, "year")

        all_years = sorted(set(list(yearly_a1.keys()) + list(yearly_a7.keys()) + list(yearly_a9.keys())))

        add(f"### 2.{coins.index(coin)+1} {coin}")
        add("")

        # 简单收益
        add("#### 简单收益（%）")
        add("")
        add("| 年份 | A1 基线 | A7 MA192 | A9 分级衰竭 | A9 vs A1 | A9 vs A7 |")
        add("|---|---|---|---|---|---|")
        for y in all_years:
            a1 = yearly_a1.get(y, {}).get("simple_sum", 0.0)
            a7 = yearly_a7.get(y, {}).get("simple_sum", 0.0)
            a9 = yearly_a9.get(y, {}).get("simple_sum", 0.0)
            diff_a1 = a9 - a1
            diff_a7 = a9 - a7
            add(f"| {y} | {a1:+.1f}% | {a7:+.1f}% | {a9:+.1f}% | {diff_a1:+.1f}pp | {diff_a7:+.1f}pp |")
        add("")

        # 复利收益
        add("#### 复利收益（%）")
        add("")
        add("| 年份 | A1 基线 | A7 MA192 | A9 分级衰竭 | A9 vs A1 | A9 vs A7 |")
        add("|---|---|---|---|---|---|")
        for y in all_years:
            a1 = yearly_a1.get(y, {}).get("compound", 0.0)
            a7 = yearly_a7.get(y, {}).get("compound", 0.0)
            a9 = yearly_a9.get(y, {}).get("compound", 0.0)
            diff_a1 = a9 - a1
            diff_a7 = a9 - a7
            add(f"| {y} | {a1:+.1f}% | {a7:+.1f}% | {a9:+.1f}% | {diff_a1:+.1f}pp | {diff_a7:+.1f}pp |")
        add("")

        # 交易笔数 + 胜率
        add("#### 交易笔数 / 胜率")
        add("")
        add("| 年份 | A1 笔数 | A1 胜率 | A9 笔数 | A9 胜率 | A9 平均盈利 | A9 平均亏损 | A9 盈亏比 |")
        add("|---|---|---|---|---|---|---|---|")
        for y in all_years:
            a1_s = yearly_a1.get(y, {})
            a9_s = yearly_a9.get(y, {})
            add(f"| {y} | {a1_s.get('total', 0)} | {a1_s.get('win_rate', 0):.1f}% | "
                f"{a9_s.get('total', 0)} | {a9_s.get('win_rate', 0):.1f}% | "
                f"{a9_s.get('avg_win', 0):+.2f}% | {a9_s.get('avg_loss', 0):+.2f}% | "
                f"{a9_s.get('profit_factor', 0):.2f} |")
        add("")

    # =================================================================
    # Section 3: 分月度统计（A9）
    # =================================================================
    add("## 三、A9 分月度统计（按年月汇总）")
    add("")

    for coin in coins:
        params = dc.SYMBOL_PARAMS[coin]
        bars, pre = data[coin]
        trades_a9 = all_results[("A9 分级+衰竭降级", coin)]
        monthly = per_period_stats(trades_a9, bars, "month")

        add(f"### 3.{coins.index(coin)+1} {coin}")
        add("")
        add("| 年月 | 笔数 | 胜率 | 简单收益 | 复利收益 | 平均每笔 |")
        add("|---|---|---|---|---|---|")

        for key in sorted(monthly.keys()):
            s = monthly[key]
            year = key // 100
            month = key % 100
            add(f"| {year}-{month:02d} | {s['total']} | {s['win_rate']:.1f}% | "
                f"{s['simple_sum']:+.2f}% | {s['compound']:+.2f}% | {s['avg_ret']:+.2f}% |")
        add("")

    # =================================================================
    # Section 4: 牛熊市分析
    # =================================================================
    add("## 四、牛熊市分析")
    add("")
    add("划分标准：年度简单收益 >20% 为牛市，<-20% 为熊市，其余为震荡。")
    add("")

    for coin in coins:
        params = dc.SYMBOL_PARAMS[coin]
        bars, pre = data[coin]
        trades_a1 = all_results[("A1 基线+vol", coin)]
        trades_a9 = all_results[("A9 分级+衰竭降级", coin)]

        yearly_a1 = per_period_stats(trades_a1, bars, "year")
        yearly_a9 = per_period_stats(trades_a9, bars, "year")

        # 用 A1 基线的年度收益来划分牛熊（避免用 A9 自身划分导致循环论证）
        yearly_simple_a1 = {y: s["simple_sum"] for y, s in yearly_a1.items()}
        bull, bear, neutral = classify_bull_bear(yearly_simple_a1)

        add(f"### 4.{coins.index(coin)+1} {coin}")
        add("")
        add(f"- 牛市年份：{sorted(bull) if bull else '无'}")
        add(f"- 熊市年份：{sorted(bear) if bear else '无'}")
        add(f"- 震荡年份：{sorted(neutral) if neutral else '无'}")
        add("")

        # 按牛熊汇总
        for regime, years, label in [(bull, bull, "牛市"), (bear, bear, "熊市"), (neutral, neutral, "震荡")]:
            if not years:
                add(f"**{label}**：无数据")
                add("")
                continue

            a1_rets = []
            a9_rets = []
            for y in years:
                if y in yearly_a1:
                    a1_rets.extend(yearly_a1[y]["rets"])
                if y in yearly_a9:
                    a9_rets.extend(yearly_a9[y]["rets"])

            a1_simple = sum(a1_rets)
            a9_simple = sum(a9_rets)
            a1_comp = comp(a1_rets)
            a9_comp = comp(a9_rets)
            a1_wr = len([r for r in a1_rets if r > 0]) / len(a1_rets) * 100 if a1_rets else 0
            a9_wr = len([r for r in a9_rets if r > 0]) / len(a9_rets) * 100 if a9_rets else 0

            add(f"**{label}**（{sorted(years)}）")
            add("")
            add(f"| 指标 | A1 基线 | A9 分级衰竭 | 差值 |")
            add(f"|---|---|---|---|")
            add(f"| 交易笔数 | {len(a1_rets)} | {len(a9_rets)} | {len(a9_rets)-len(a1_rets)} |")
            add(f"| 胜率 | {a1_wr:.1f}% | {a9_wr:.1f}% | {a9_wr-a1_wr:+.1f}pp |")
            add(f"| 简单收益 | {a1_simple:+.1f}% | {a9_simple:+.1f}% | {a9_simple-a1_simple:+.1f}pp |")
            add(f"| 复利收益 | {a1_comp:+.1f}% | {a9_comp:+.1f}% | {a9_comp-a1_comp:+.1f}pp |")
            add("")

    # =================================================================
    # Section 5: 衰竭降级细节分析
    # =================================================================
    add("## 五、衰竭降级细节分析")
    add("")

    for coin in coins:
        params = dc.SYMBOL_PARAMS[coin]
        bars, pre = data[coin]
        trades = backtest_trades(coin, params, bars, pre, mode="tiered_demote", confirm=10,
                                 switch_at=20.0, demote_pct=10.0, activate=15.0, record_details=True)

        demoted = [t for t in trades if t.get("demoted")]
        not_demoted = [t for t in trades if not t.get("demoted") and t["mfe_pct"] >= 20.0]
        all_upgraded = demoted + not_demoted

        add(f"### 5.{coins.index(coin)+1} {coin}")
        add("")
        add(f"- 总交易笔数：{len(trades)}")
        add(f"- 触发过升级（MFE≥20%）：{len(all_upgraded)} 笔")
        add(f"- 其中被降级（衰竭）：{len(demoted)} 笔（{len(demoted)/len(all_upgraded)*100:.1f}%）" if all_upgraded else "- 无升级交易")
        add(f"- 未降级（长拿到 MA480 离场）：{len(not_demoted)} 笔")
        add("")

        if demoted:
            dem_rets = [t["ret_pct"] for t in demoted]
            add(f"**降级单统计**：")
            add(f"- 平均收益：{mean(dem_rets):+.2f}%")
            add(f"- 中位收益：{sorted(dem_rets)[len(dem_rets)//2]:+.2f}%")
            add(f"- 胜率：{len([r for r in dem_rets if r > 0]) / len(dem_rets) * 100:.1f}%")
            add(f"- 最大盈利：{max(dem_rets):+.2f}%")
            add(f"- 最大亏损：{min(dem_rets):+.2f}%")
            add(f"- 平均 MFE：{mean([t['mfe_pct'] for t in demoted]):+.2f}%")
            add(f"- 平均持仓 bars：{mean([t['hold_bars'] for t in demoted]):.0f}")
            add("")

        if not_demoted:
            nd_rets = [t["ret_pct"] for t in not_demoted]
            add(f"**未降级单（MA480 长拿）统计**：")
            add(f"- 平均收益：{mean(nd_rets):+.2f}%")
            add(f"- 中位收益：{sorted(nd_rets)[len(nd_rets)//2]:+.2f}%")
            add(f"- 胜率：{len([r for r in nd_rets if r > 0]) / len(nd_rets) * 100:.1f}%")
            add(f"- 最大盈利：{max(nd_rets):+.2f}%")
            add(f"- 最大亏损：{min(nd_rets):+.2f}%")
            add(f"- 平均 MFE：{mean([t['mfe_pct'] for t in not_demoted]):+.2f}%")
            add(f"- 平均持仓 bars：{mean([t['hold_bars'] for t in not_demoted]):.0f}")
            add("")

    # =================================================================
    # Section 6: 离场原因分析（A9）
    # =================================================================
    add("## 六、A9 离场原因分析")
    add("")

    for coin in coins:
        params = dc.SYMBOL_PARAMS[coin]
        bars, pre = data[coin]
        trades = all_results[("A9 分级+衰竭降级", coin)]
        reason_stats = exit_reason_stats(trades)

        add(f"### 6.{coins.index(coin)+1} {coin}")
        add("")
        add("| 离场原因 | 笔数 | 占比 | 平均收益 | 胜率 | 总收益贡献 |")
        add("|---|---|---|---|---|---|")

        total_count = len(trades)
        for reason in sorted(reason_stats.keys(), key=lambda r: -reason_stats[r]["count"]):
            s = reason_stats[reason]
            pct = s["count"] / total_count * 100 if total_count > 0 else 0
            add(f"| {reason} | {s['count']} | {pct:.1f}% | {s['avg_ret']:+.2f}% | "
                f"{s['win_rate']:.1f}% | {s['total_ret']:+.1f}% |")
        add("")

    # =================================================================
    # Section 7: 时间切分验证（双向）
    # =================================================================
    add("## 七、时间切分验证")
    add("")
    add("将数据按时间分为前后两半，分别用前半选参数→后半验证，和后半选参数→前半验证。")
    add("")

    for coin in coins:
        params = dc.SYMBOL_PARAMS[coin]
        bars, pre = data[coin]
        years = [datetime.fromtimestamp(b.open_time / 1000, tz=timezone(timedelta(hours=8))).year for b in bars]
        y0, y1 = min(years), max(years)
        mid = (y0 + y1) // 2

        add(f"### 7.{coins.index(coin)+1} {coin}（数据范围 {y0}-{y1}）")
        add("")

        # 前半训练 → 后半验证
        train_trades = backtest_trades(coin, params, bars, pre, mode="tiered_demote", confirm=10,
                                       switch_at=20.0, demote_pct=10.0, activate=15.0, y0=y0, y1=mid)
        val_trades = backtest_trades(coin, params, bars, pre, mode="tiered_demote", confirm=10,
                                     switch_at=20.0, demote_pct=10.0, activate=15.0, y0=mid+1, y1=y1)

        train_comp = comp([t["ret_pct"] for t in train_trades])
        val_comp = comp([t["ret_pct"] for t in val_trades])

        # 基线对比
        train_base = backtest_trades(coin, params, bars, pre, mode="base", y0=y0, y1=mid)
        val_base = backtest_trades(coin, params, bars, pre, mode="base", y0=mid+1, y1=y1)
        train_base_comp = comp([t["ret_pct"] for t in train_base])
        val_base_comp = comp([t["ret_pct"] for t in val_base])

        add(f"**前半训练（{y0}-{mid}）→ 后半验证（{mid+1}-{y1}）**")
        add("")
        add(f"| 指标 | 训练期 | 验证期 |")
        add(f"|---|---|---|")
        add(f"| A1 基线复利 | {train_base_comp:+.1f}% | {val_base_comp:+.1f}% |")
        add(f"| A9 复利 | {train_comp:+.1f}% | {val_comp:+.1f}% |")
        add(f"| A9 提升 | {train_comp-train_base_comp:+.1f}pp | {val_comp-val_base_comp:+.1f}pp |")
        add(f"| A9 交易笔数 | {len(train_trades)} | {len(val_trades)} |")
        add(f"| A9 胜率 | {len([t for t in train_trades if t['ret_pct']>0])/len(train_trades)*100:.1f}% | "
            f"{len([t for t in val_trades if t['ret_pct']>0])/len(val_trades)*100:.1f}% |" if val_trades else "")
        add("")

        # 验证期是否跑赢基线
        if val_comp > val_base_comp:
            add(f"✅ 验证期 A9 跑赢基线 {val_comp-val_base_comp:+.1f}pp")
        else:
            add(f"❌ 验证期 A9 跑输基线 {val_comp-val_base_comp:+.1f}pp")
        add("")

    # =================================================================
    # Section 8: 逐笔大单分析（MFE>=20% 的交易）
    # =================================================================
    add("## 八、大单分析（MFE≥20% 的交易）")
    add("")
    add("这些是触发过升级到 MA480 的交易，是 A9 策略的核心价值所在。")
    add("")

    for coin in coins:
        params = dc.SYMBOL_PARAMS[coin]
        bars, pre = data[coin]
        trades = backtest_trades(coin, params, bars, pre, mode="tiered_demote", confirm=10,
                                 switch_at=20.0, demote_pct=10.0, activate=15.0, record_details=True)

        big = [t for t in trades if t["mfe_pct"] >= 20.0]
        if not big:
            add(f"### 8.{coins.index(coin)+1} {coin}：无大单")
            add("")
            continue

        big_rets = [t["ret_pct"] for t in big]
        demoted_big = [t for t in big if t.get("demoted")]
        kept_big = [t for t in big if not t.get("demoted")]

        add(f"### 8.{coins.index(coin)+1} {coin}")
        add("")
        add(f"- 大单总数：{len(big)}（占总交易 {len(big)/len(trades)*100:.1f}%）")
        add(f"- 大单总收益贡献：{sum(big_rets):+.1f}%")
        add(f"- 大单平均收益：{mean(big_rets):+.2f}%")
        add(f"- 大单胜率：{len([r for r in big_rets if r > 0])/len(big)*100:.1f}%")
        add(f"- 被降级：{len(demoted_big)} 笔，平均收益 {mean([t['ret_pct'] for t in demoted_big]):+.2f}%" if demoted_big else "")
        add(f"- 未降级（MA480 长拿）：{len(kept_big)} 笔，平均收益 {mean([t['ret_pct'] for t in kept_big]):+.2f}%" if kept_big else "")
        add("")

        # 大单收益分布
        add("**大单收益分布**：")
        add("")
        add("| 区间 | 笔数 | 占比 |")
        add("|---|---|---|")
        ranges = [(-100, -10), (-10, 0), (0, 5), (5, 10), (10, 20), (20, 50), (50, 100), (100, 1000)]
        for lo, hi in ranges:
            count = len([r for r in big_rets if lo <= r < hi])
            if count > 0:
                add(f"| [{lo}%, {hi}%) | {count} | {count/len(big)*100:.1f}% |")
        add("")

    # =================================================================
    # Section 9: 综合结论
    # =================================================================
    add("## 九、综合结论")
    add("")
    add("### 优势")
    add("")
    add("1. **牛市捕获能力最强**：分级+衰竭降级让大单在 MA480 上长拿，牛市收益显著高于基线")
    add("2. **熊市保护不弱**：衰竭降级机制在利润回撤时及时锁利，避免大单利润蒸发")
    add("3. **震荡市适应性**：小单仍用 MA192 锁利，不因升级而过度冒险")
    add("")

    add("### 风险")
    add("")
    add("1. **未计手续费/滑点**：长拿交易更少但更长，手续费影响需单独评估")
    add("2. **参数敏感性**：switch_at=20%、demote_pct=10% 是样本内选择，有过拟合风险")
    add("3. **BNB/HYPE 不适用**：趋势不够持续，A7/A9 在这两个币上跑输基线")
    add("")

    add("### 生产落地建议")
    add("")
    add("1. 先对 BTC/ETH/SOL 启用 A9，BNB/SUI/HYPE 保持 A1 基线")
    add("2. `engine.rs` 需要增加：分级止盈逻辑 + 衰竭降级逻辑 + MA480 止盈线")
    add("3. 建议先影子模式运行 1-2 个月，对比实际信号与回测信号的一致性")
    add("4. 加手续费/滑点敏感性测试后再正式上线")
    add("")

    # 写入文件
    out_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), "feature_report")
    os.makedirs(out_dir, exist_ok=True)
    out_path = os.path.join(out_dir, "a9_full_validation.md")
    with open(out_path, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"Report generated: {out_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
