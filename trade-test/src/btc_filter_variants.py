#!/usr/bin/env python3
"""
方案C 入场过滤变体批量测试（6币种 × 5变体）

变体（叠加在原始信号之上，入场时判定）：
  V1 MA48排列确认:  做多 ma48>ma192，做空 ma48<ma192
  V2 MA48斜率:      做多 ma48>ma48(前4根)，做空 ma48<ma48(前4根)
  V3 压缩时长上限:  compression_bars<=60（输家平均90根，赢家20根）
  V4 带宽百分位下限: boll_width_pct>=0.15（赢家0.44，输家0.14）
  V5 V1+V3 组合

每个变体输出：
  - 信号层: 拒单数 / 拒亏损单(占原始亏损%) / 误杀盈利单(占原始盈利%)
  - 重模拟: 交易数 / 胜率 / 复利 / 最大回撤
"""
import os
import sys
import pandas as pd
import numpy as np
import warnings
warnings.filterwarnings('ignore')

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from strategy_c_final_backtest import FinalBacktester
from btc_ma48_filter_analysis import run_backtest_with_features, summarize, DATA_DIR, PARAMS


def build_reject_mask(df, variant):
    """返回布尔 mask：True = 该bar的信号被拒绝。"""
    sig = df['signal']
    is_long = sig == 1
    is_short = sig == -1
    if variant == 'V1':
        return (is_long & (df['ma48'] <= df['ma192'])) | (is_short & (df['ma48'] >= df['ma192']))
    if variant == 'V2':
        m48_prev = df['ma48'].shift(4)
        return (is_long & (df['ma48'] <= m48_prev)) | (is_short & (df['ma48'] >= m48_prev))
    if variant == 'V3':
        return (sig != 0) & (df['compression_bars'] > 60)
    if variant == 'V4':
        return (sig != 0) & (df['boll_width_pct'] < 0.15)
    if variant == 'V5':
        m1 = (is_long & (df['ma48'] <= df['ma192'])) | (is_short & (df['ma48'] >= df['ma192']))
        m3 = (sig != 0) & (df['compression_bars'] > 60)
        return m1 | m3
    raise ValueError(variant)


def run_variant(symbol, variant):
    bt = FinalBacktester(DATA_DIR)
    df = bt.load_data(symbol)
    df = bt.calculate_indicators(df, PARAMS)
    df['ma48'] = df['close'].rolling(window=48).mean()
    df = bt.generate_signals(df)

    # 原始
    orig_trades, init_cap, _ = run_backtest_with_features(df, PARAMS)
    orig_stats = summarize(orig_trades, init_cap)
    tdf = pd.DataFrame(orig_trades)
    n_orig_losers = int((tdf['pnl_pct'] <= 0).sum())
    n_orig_winners = int((tdf['pnl_pct'] > 0).sum())

    # 过滤
    mask = build_reject_mask(df, variant)
    df_v = df.copy()
    df_v.loc[mask, 'signal'] = 0
    v_trades, _, _ = run_backtest_with_features(df_v, PARAMS)
    v_stats = summarize(v_trades, init_cap)

    # 信号层被拒（原始交易中入场bar被 mask 覆盖的）
    signal_idx = set(tdf.index)  # 占位，实际需按入场时间匹配
    # 精确匹配：为每笔原始交易找入场 bar 的 mask 值
    # 构造 entry_time -> mask 映射（用入场时间精确匹配 df 的 open_time）
    df_index = df[['open_time', 'signal']].copy()
    df_index['rejected'] = mask
    entry_rej = []
    for _, t in tdf.iterrows():
        row = df_index[df_index['open_time'] == t['entry_time']]
        entry_rej.append(bool(row['rejected'].iloc[0]) if len(row) else False)
    tdf['rejected'] = entry_rej
    rej = tdf[tdf['rejected']]
    n_rej = len(rej)
    n_rej_losers = int((rej['pnl_pct'] <= 0).sum())
    n_rej_winners = int((rej['pnl_pct'] > 0).sum())

    return {
        'symbol': symbol, 'variant': variant,
        'orig': orig_stats, 'filt': v_stats,
        'n_orig': len(tdf), 'n_orig_losers': n_orig_losers, 'n_orig_winners': n_orig_winners,
        'n_rej': n_rej, 'n_rej_losers': n_rej_losers, 'n_rej_winners': n_rej_winners,
    }


def main():
    variants = ['V1', 'V2', 'V3', 'V4', 'V5']
    symbols = ['BTC', 'ETH', 'SOL', 'BNB', 'SUI', 'HYPE']
    rows = []

    for sym in symbols:
        for v in variants:
            try:
                r = run_variant(sym, v)
            except Exception as e:
                print(f"{sym} {v} 失败: {e}")
                continue
            o, f = r['orig'], r['filt']
            rl_pct = r['n_rej_losers'] / r['n_orig_losers'] * 100 if r['n_orig_losers'] else 0
            rw_pct = r['n_rej_winners'] / r['n_orig_winners'] * 100 if r['n_orig_winners'] else 0
            rows.append({
                'symbol': sym, 'variant': v,
                'orig_trades': o['trades'], 'orig_winrate': round(o['win_rate'], 1),
                'orig_compound': round(o['compound_return'], 1), 'orig_dd': round(o['max_drawdown'], 1),
                'filt_trades': f['trades'], 'filt_winrate': round(f['win_rate'], 1),
                'filt_compound': round(f['compound_return'], 1), 'filt_dd': round(f['max_drawdown'], 1),
                'rej': r['n_rej'], 'rej_losers': r['n_rej_losers'],
                'rej_losers_pct': round(rl_pct, 1), 'rej_winners': r['n_rej_winners'],
                'rej_winners_pct': round(rw_pct, 1),
            })
            print(f"{sym} {v}: 过滤后 {f['trades']}笔/胜率{f['win_rate']:.1f}%/复利{f['compound_return']:.1f}%/回撤{f['max_drawdown']:.1f}% | "
                  f"拒{r['n_rej']}(亏{r['n_rej_losers']}[{rl_pct:.0f}%]/盈{r['n_rej_winners']}[{rw_pct:.0f}%])")

    # 汇总表
    print("\n===== 汇总（每币种一行，按变体） =====")
    for v in variants:
        print(f"\n--- {v} ---")
        print(f"{'币种':<6}{'原始笔数':>8}{'原始胜率':>9}{'原始复利':>9}{'原始回撤':>9} | "
              f"{'过滤笔数':>8}{'过滤胜率':>9}{'过滤复利':>9}{'过滤回撤':>9} | {'拒单':>5}{'拒亏损%':>8}{'误杀盈利%':>9}")
        for r in rows:
            if r['variant'] != v:
                continue
            print(f"{r['symbol']:<6}{r['orig_trades']:>8}{r['orig_winrate']:>8.1f}%{r['orig_compound']:>8.1f}%{r['orig_dd']:>8.1f}% | "
                  f"{r['filt_trades']:>8}{r['filt_winrate']:>8.1f}%{r['filt_compound']:>8.1f}%{r['filt_dd']:>8.1f}% | "
                  f"{r['rej']:>5}{r['rej_losers_pct']:>7.1f}%{r['rej_winners_pct']:>8.1f}%")

    # 保存
    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'feature_report',
                       'strategy_c_filter_variants_results.csv')
    pd.DataFrame(rows).to_csv(out, index=False, encoding='utf-8-sig')
    print(f"\n结果已保存: {out}")


if __name__ == '__main__':
    main()
