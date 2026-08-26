#!/usr/bin/env python3
"""
方案A：BOLL_LOW 出场 × 入场削减 组合测试

组合：
  BL         BOLL_LOW（下轨止损，基线参照）
  C1         BOLL_LOW + V2（MA48斜率过滤）
  C2         BOLL_LOW + V3（压缩时长<=60根）
  C3         BOLL_LOW + V2 + V3

输出（全样本 + 2024-2026分年 + 手续费敏感性）：
  笔数 / 胜率 / 复利 / 最大回撤 / 平均持仓
  含费：FEE0 / FEE1(taker 0.05%/边) / FEE2(maker 0.02%/边)
"""
import os
import sys
import pandas as pd
import numpy as np
import warnings
warnings.filterwarnings('ignore')

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from strategy_c_final_backtest import FinalBacktester
from btc_ma48_filter_analysis import DATA_DIR, PARAMS, summarize
from btc_exit_variants import backtest_with_exit
from btc_filter_variants import build_reject_mask

FEES = {'FEE0': 0.0, 'FEE1': 0.10, 'FEE2': 0.04}

COMBOS = [
    ('BL', []),
    ('C1', ['V2']),
    ('C2', ['V3']),
    ('C3', ['V2', 'V3']),
]


def apply_fee(trades, fee_per_round):
    cap = 10000.0
    curve = [cap]
    for t in trades:
        fee = cap * (fee_per_round / 100)
        cap += t['pnl_amount'] - fee
        curve.append(cap)
    series = pd.Series(curve)
    dd = ((series - series.expanding().max()) / series.expanding().max() * 100).min()
    return (cap - 10000.0) / 10000.0 * 100, dd


def yearly_compound(trades):
    df = pd.DataFrame(trades)
    df['entry_time'] = pd.to_datetime(df['entry_time'])
    df['year'] = df['entry_time'].dt.year
    out = {}
    for year, g in df.groupby('year'):
        cap = 10000.0
        for t in g.to_dict('records'):
            cap += t['pnl_amount']
        out[year] = (cap - 10000.0) / 10000.0 * 100
    return out


def main():
    symbols = ['BTC', 'ETH', 'SOL', 'BNB', 'SUI', 'HYPE']
    rows = []
    summaries = {}

    for sym in symbols:
        bt = FinalBacktester(DATA_DIR)
        df = bt.load_data(sym)
        df = bt.calculate_indicators(df, PARAMS)
        df['ma48'] = df['close'].rolling(window=48).mean()
        df = bt.generate_signals(df)

        print(f"\n{'='*100}\n{sym}\n{'='*100}")
        print(f"{'组合':<6}{'笔数':>6}{'胜率':>8}{'复利':>9}{'回撤':>9}{'持仓':>7} | "
              f"{'FEE1复利':>9}{'FEE1回撤':>9}{'FEE2复利':>9}{'FEE2回撤':>9} | 2024 / 2025 / 2026")
        for name, filters in COMBOS:
            df_v = df.copy()
            for f in filters:
                mask = build_reject_mask(df_v, f)
                df_v.loc[mask, 'signal'] = 0
            trades, init_cap, _ = backtest_with_exit(df_v, PARAMS, 'BOLL_LOW')
            st = summarize(trades, init_cap)
            tdf = pd.DataFrame(trades)
            avg_hold = tdf['bars_held'].mean() if len(tdf) else 0
            ret1, dd1 = apply_fee(trades, FEES['FEE1'])
            ret2, dd2 = apply_fee(trades, FEES['FEE2'])
            yrs = yearly_compound(trades)
            y24 = yrs.get(2024, 0); y25 = yrs.get(2025, 0); y26 = yrs.get(2026, 0)
            print(f"{name:<6}{st['trades']:>6}{st['win_rate']:>7.1f}%{st['compound_return']:>8.1f}%"
                  f"{st['max_drawdown']:>8.1f}%{avg_hold:>6.1f}根 | "
                  f"{ret1:>8.1f}%{dd1:>8.1f}%{ret2:>8.1f}%{dd2:>8.1f}% | "
                  f"{y24:>6.1f}% / {y25:>6.1f}% / {y26:>6.1f}%")
            rows.append({'symbol': sym, 'combo': name, 'trades': st['trades'],
                         'win_rate': round(st['win_rate'], 1),
                         'compound': round(st['compound_return'], 1),
                         'max_dd': round(st['max_drawdown'], 1),
                         'avg_hold': round(avg_hold, 1),
                         'fee1_compound': round(ret1, 1), 'fee1_dd': round(dd1, 1),
                         'fee2_compound': round(ret2, 1), 'fee2_dd': round(dd2, 1),
                         'y2024': round(y24, 1), 'y2025': round(y25, 1), 'y2026': round(y26, 1)})

        # 2025-2026 单独（BTC 重点）
        if sym == 'BTC':
            print("\n--- BTC 2025-2026 单独验证 ---")
            for name, filters in COMBOS:
                df_v = df.copy()
                for f in filters:
                    mask = build_reject_mask(df_v, f)
                    df_v.loc[mask, 'signal'] = 0
                trades, _, _ = backtest_with_exit(df_v, PARAMS, 'BOLL_LOW')
                tdf = pd.DataFrame(trades)
                tdf['entry_time'] = pd.to_datetime(tdf['entry_time'])
                recent = tdf[tdf['entry_time'].dt.year >= 2025].to_dict('records')
                if not recent:
                    print(f"{name}: 无交易")
                    continue
                r0, d0 = apply_fee(recent, 0.0)
                r1, d1 = apply_fee(recent, FEES['FEE1'])
                print(f"{name}: 近2年{len(recent)}笔 | 复利 {r0:.1f}% (回撤{d0:.1f}%) | "
                      f"taker后 {r1:.1f}% (回撤{d1:.1f}%)")

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'feature_report',
                       'strategy_c_combo_variants_results.csv')
    pd.DataFrame(rows).to_csv(out, index=False, encoding='utf-8-sig')
    print(f"\n结果已保存: {out}")


if __name__ == '__main__':
    main()
