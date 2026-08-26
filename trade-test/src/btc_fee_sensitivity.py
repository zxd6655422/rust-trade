#!/usr/bin/env python3
"""
手续费敏感性测试（ORIG / BUF5 / BOLL_LOW × 全样本 + BTC 25-26）

费率场景：
  FEE0   0%（原始口径）
  FEE1   双边 0.05% taker（单边0.05%，每笔扣0.10%）
  FEE2   双边 0.02% maker（每笔扣0.04%）
  FEE3   双边 0.10% taker（每笔扣0.20%）
"""
import os
import sys
import pandas as pd
import warnings
warnings.filterwarnings('ignore')

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from strategy_c_final_backtest import FinalBacktester
from btc_ma48_filter_analysis import DATA_DIR, PARAMS
from btc_exit_variants import backtest_with_exit


def apply_fee(trades, fee_per_round):
    """每笔交易扣 fee_per_round（百分比），重算 pnl_amount 与复利路径。"""
    cap = 10000.0
    curve = [cap]
    fee_impact_total = 0.0
    for t in trades:
        gross = t['pnl_amount']
        fee = cap * (fee_per_round / 100)
        t['fee'] = fee
        t['net_pnl_amount'] = gross - fee
        cap += t['net_pnl_amount']
        curve.append(cap)
        fee_impact_total += fee
    series = pd.Series(curve)
    dd = ((series - series.expanding().max()) / series.expanding().max() * 100).min()
    return (cap - 10000.0) / 10000.0 * 100, dd, fee_impact_total


def main():
    symbols = ['BTC', 'ETH', 'SOL', 'BNB', 'SUI', 'HYPE']
    modes = ['ORIG', 'BUF5', 'BOLL_LOW']
    fees = {'FEE0': 0.0, 'FEE1': 0.10, 'FEE2': 0.04, 'FEE3': 0.20}
    rows = []

    for sym in symbols:
        bt = FinalBacktester(DATA_DIR)
        df = bt.load_data(sym)
        df = bt.calculate_indicators(df, PARAMS)
        df = bt.generate_signals(df)

        print(f"\n===== {sym} =====")
        print(f"{'模式':<9}{'笔数':>6}" + "".join(f"{f:>16}" for f in fees))
        for m in modes:
            trades, _, _ = backtest_with_exit(df, PARAMS, m)
            cells = []
            for fname, fee in fees.items():
                ret, dd, fi = apply_fee(trades, fee)
                cells.append(f"复利{ret:>8.1f}% 回撤{dd:>7.1f}%")
                rows.append({'symbol': sym, 'mode': m, 'fee': fname, 'trades': len(trades),
                             'compound': round(ret, 1), 'max_dd': round(dd, 1),
                             'fee_total': round(fi, 1)})
            print(f"{m:<9}{len(trades):>6}" + "".join(f"{c:>16}" for c in cells))

    # BTC 2025-2026 重点
    print("\n===== BTC 2025-2026（近两年） =====")
    bt = FinalBacktester(DATA_DIR)
    df = bt.load_data('BTC')
    df = bt.calculate_indicators(df, PARAMS)
    df = bt.generate_signals(df)
    print(f"{'模式':<9}{'笔数':>6}" + "".join(f"{f:>16}" for f in fees))
    for m in modes:
        trades, _, _ = backtest_with_exit(df, PARAMS, m)
        tdf = pd.DataFrame(trades)
        tdf['entry_time'] = pd.to_datetime(tdf['entry_time'])
        recent = tdf[tdf['entry_time'].dt.year >= 2025].to_dict('records')
        cells = []
        for fname, fee in fees.items():
            ret, dd, fi = apply_fee(recent, fee)
            cells.append(f"复利{ret:>8.1f}% 回撤{dd:>7.1f}%")
        print(f"{m:<9}{len(recent):>6}" + "".join(f"{c:>16}" for c in cells))

    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'feature_report',
                       'strategy_c_fee_sensitivity.csv')
    pd.DataFrame(rows).to_csv(out, index=False, encoding='utf-8-sig')
    print(f"\n结果已保存: {out}")


if __name__ == '__main__':
    main()
