"""参数矩阵回测：为 BNB/SUI/HYPE 寻找合适的 MA趋势回踩 参数。

固定 fast=288 / slow=488（策略身份不变），在硬止损、移动止盈激活线、回撤回调三个维度扫网格。
同时给出已有 BTC/ETH/SOL 三套模板在这三个新币上的表现（检验参数是否可直接迁移）。

排序口径：以「总收益(复利)」为主（真实资金曲线），并同时列出简单收益、回撤、利润因子。

运行：cd F:/rust-projects/trade-test/src && python backtest_matrix.py
输出：param_matrix_report.md
"""
from __future__ import annotations

import os
from itertools import product
from typing import List, Dict, Any

import backtest as bt
import ma_trend_pullback as strat
from loader import load_klines_30m

SRC_DIR = os.path.dirname(os.path.abspath(__file__))
MD_PATH = os.path.join(SRC_DIR, "param_matrix_report.md")

NEW_COINS = ["BNBUSDT", "SUIUSDT", "HYPEUSDT"]

HARD_STOPS = [1.0, 1.5, 2.0, 2.5, 3.0]
ACTIVATES = [3.0, 4.0, 5.0, 6.0]
CALLBACKS = [0.5, 1.0, 1.5, 2.0]

TEMPLATES = {
    "BTC模板(1.5/4/1)": (1.5, 4.0, 1.0),
    "ETH模板(1.5/5/1)": (1.5, 5.0, 1.0),
    "SOL模板(2.0/4/1)": (2.0, 4.0, 1.0),
}


def make_params(hs, act, cb) -> strat.Params:
    return strat.Params(
        fast_ma_period=288, slow_ma_period=488,
        stop_mode="ma288", hard_stop_pct=hs,
        take_profit_mode="trailing", trailing_activate_pct=act, trailing_callback_pct=cb,
        slope_threshold=0.0, bbw_threshold=0.0, vol_threshold=0.0,
        use_30m_expanding=False, use_5m_expanding=False, min_angle_5m=0.0,
        entry_timeframe="30m",
    )


def fmt_pf(pf):
    return "∞" if pf == float("inf") else f"{pf:.2f}"


def main() -> int:
    md: List[str] = []
    add = md.append
    add("# BNB / SUI / HYPE 参数矩阵回测报告")
    add("")
    add("- 固定 fast=288 / slow=488；网格 = 硬止损 × 移动止盈激活线 × 回撤回调")
    add("- 网格范围：硬止损 {1.0,1.5,2.0,2.5,3.0}%；激活线 {3,4,5,6}%；回调 {0.5,1,1.5,2}%")
    add("- 排序以**总收益(复利)**为准（真实资金曲线）；未计手续费/滑点")
    add("")

    per_coin: Dict[str, List[Any]] = {}

    for coin in NEW_COINS:
        bars = load_klines_30m(coin)
        add(f"## {coin}（{len(bars)} 根 30m K线）")
        add("")

        # 已有模板
        add("### 已有模板在新币上的表现（参数是否可迁移）")
        add("")
        add("| 模板 | 硬止损 | 激活 | 回调 | 交易数 | 胜率 | 总收益(简单) | 总收益(复利) | 最大回撤 | 利润因子 |")
        add("|---|---|---|---|---|---|---|---|---|---|")
        for name, (hs, act, cb) in TEMPLATES.items():
            m = bt.compute_metrics(bt.backtest(coin, make_params(hs, act, cb), bars))
            add(f"| {name} | {hs} | {act} | {cb} | {m['n']} | {m['win_rate']:.1f}% | "
                f"{m['total_ret']:+.2f}% | {m['compound_ret']:+.2f}% | {m['max_drawdown']:.2f}% | {fmt_pf(m['profit_factor'])} |")
        add("")

        # 网格
        grid = []
        for hs, act, cb in product(HARD_STOPS, ACTIVATES, CALLBACKS):
            m = bt.compute_metrics(bt.backtest(coin, make_params(hs, act, cb), bars))
            grid.append(((hs, act, cb), m))
        per_coin[coin] = grid

        grid_sorted = sorted(grid, key=lambda x: -x[1]["compound_ret"])
        add(f"### Top 15 参数（按复利收益）")
        add("")
        add("| 硬止损 | 激活 | 回调 | 交易数 | 胜率 | 总收益(简单) | 总收益(复利) | 最大回撤 | 利润因子 |")
        add("|---|---|---|---|---|---|---|---|---|")
        for (hs, act, cb), m in grid_sorted[:15]:
            add(f"| {hs} | {act} | {cb} | {m['n']} | {m['win_rate']:.1f}% | "
                f"{m['total_ret']:+.2f}% | {m['compound_ret']:+.2f}% | {m['max_drawdown']:.2f}% | {fmt_pf(m['profit_factor'])} |")
        add("")

    # 跨币种稳健配置
    add("## 跨新币稳健配置（三个新币上都尽量不差）")
    add("")
    add("对每个网格点，取其在 BNB/SUI/HYPE 上的「最小复利收益」作为稳健度，选出最稳健的 Top 10。")
    add("")
    add("| 硬止损 | 激活 | 回调 | BNB复利 | SUI复利 | HYPE复利 | 最差(最小) | 三币合计(简单) |")
    add("|---|---|---|---|---|---|---|---|")
    robust = []
    for hs, act, cb in product(HARD_STOPS, ACTIVATES, CALLBACKS):
        vals = {}
        for coin in NEW_COINS:
            for (p, m) in per_coin[coin]:
                if p == (hs, act, cb):
                    vals[coin] = m
                    break
        comp = [vals[c]["compound_ret"] for c in NEW_COINS]
        simple_sum = sum(vals[c]["total_ret"] for c in NEW_COINS)
        robust.append(((hs, act, cb), comp, simple_sum))
    robust.sort(key=lambda x: -min(x[1]))
    for (hs, act, cb), comp, simple_sum in robust[:10]:
        add(f"| {hs} | {act} | {cb} | {comp[0]:+.2f}% | {comp[1]:+.2f}% | {comp[2]:+.2f}% | "
            f"{min(comp):+.2f}% | {simple_sum:+.2f}% |")
    add("")

    with open(MD_PATH, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
    print(f"[written] {MD_PATH}")

    # 控制台打印 Top 配置，便于阅读
    print("\n==== 各新币 Top5（复利） ====")
    for coin in NEW_COINS:
        print(f"\n--- {coin} ---")
        for (hs, act, cb), m in sorted(per_coin[coin], key=lambda x: -x[1]["compound_ret"])[:5]:
            print(f"  hs={hs} act={act} cb={cb}  n={m['n']} 简单={m['total_ret']:+.2f}% 复利={m['compound_ret']:+.2f}% 回撤={m['max_drawdown']:.2f}%")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
