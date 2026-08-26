#!/usr/bin/env python3
"""
方案C + MA48过滤条件分析（BTC及其他币种）

新增过滤条件：
  - 做多：MA48 需在 BOLL中轨之上（ma48 > boll_mid）
  - 做空：MA48 需在 BOLL中轨之下（ma48 < boll_mid）

输出：
  1. 过滤前后完整回测对比（交易数/胜率/复利/回撤）
  2. 信号层面：原始交易中多少亏损单被过滤、多少盈利单被误过滤（受影响）
  回测逻辑与 strategy_c_final_backtest.py 一致。
"""
import os
import sys
import pandas as pd
import numpy as np
import warnings
warnings.filterwarnings('ignore')

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from strategy_c_final_backtest import FinalBacktester

DATA_DIR = r"D:\dev-projects\data_2026-08-13"

PARAMS = {
    'ma_period': 192,
    'boll_period': 100,
    'boll_std': 2.0,
    'compression_threshold': 0.3,
    'min_compression_bars': 10,
    'hard_stop_pct': 2.0,
    'boll_stop_enabled': True,
}


def run_backtest_with_features(df, params):
    """与 FinalBacktester.run_backtest 相同逻辑，但每笔交易额外记录入场bar的 ma48 和 boll_mid。"""
    df = df.copy()
    hard_stop_pct = params.get('hard_stop_pct', 2.0)
    boll_stop_enabled = params.get('boll_stop_enabled', True)

    initial_capital = 10000.0
    capital = initial_capital
    position = 0
    entry_price = 0.0
    entry_time = None
    entry_ma48 = None
    entry_boll_mid = None
    trades = []

    for i in range(1, len(df)):
        bar = df.iloc[i]

        # --- 出场检查 ---
        if position != 0:
            if position == 1:
                cur_profit = (bar['close'] - entry_price) / entry_price * 100
            else:
                cur_profit = (entry_price - bar['close']) / entry_price * 100

            exit_reason = None
            exit_price = bar['close']
            exit_time = bar['open_time']

            if cur_profit <= -hard_stop_pct:
                exit_reason = 'hard_stop'
            elif boll_stop_enabled:
                if position == 1 and bar['close'] < bar['boll_mid']:
                    exit_reason = 'boll_mid_stop'
                elif position == -1 and bar['close'] > bar['boll_mid']:
                    exit_reason = 'boll_mid_stop'

            if exit_reason:
                if position == 1:
                    pnl_pct = (exit_price - entry_price) / entry_price * 100
                else:
                    pnl_pct = (entry_price - exit_price) / entry_price * 100
                pnl_amount = capital * (pnl_pct / 100)
                capital += pnl_amount

                trades.append({
                    'entry_time': entry_time,
                    'exit_time': exit_time,
                    'direction': 'LONG' if position == 1 else 'SHORT',
                    'entry_price': entry_price,
                    'exit_price': exit_price,
                    'entry_ma48': entry_ma48,
                    'entry_boll_mid': entry_boll_mid,
                    'pnl_pct': pnl_pct,
                    'pnl_amount': pnl_amount,
                    'exit_reason': exit_reason,
                })

                position = 0
                entry_price = 0.0
                entry_time = None
                entry_ma48 = None
                entry_boll_mid = None

        # --- 入场检查 ---
        if position == 0 and bar['signal'] != 0:
            entry_price = bar['close']
            entry_time = bar['open_time']
            entry_ma48 = bar['ma48']
            entry_boll_mid = bar['boll_mid']
            position = bar['signal']

    return trades, initial_capital, capital


def summarize(trades, initial_capital):
    """计算交易统计（复利按顺序累加 pnl_amount，含最大回撤）。"""
    if not trades:
        return {'trades': 0, 'wins': 0, 'losses': 0, 'win_rate': 0.0,
                'compound_return': 0.0, 'max_drawdown': 0.0, 'total_pnl_sum': 0.0}
    df = pd.DataFrame(trades)
    n = len(df)
    wins = int((df['pnl_pct'] > 0).sum())
    losses = int((df['pnl_pct'] <= 0).sum())
    capital = initial_capital
    curve = [capital]
    for t in trades:
        capital += t['pnl_amount']
        curve.append(capital)
    series = pd.Series(curve)
    rolling_max = series.expanding().max()
    drawdowns = (series - rolling_max) / rolling_max * 100
    return {
        'trades': n, 'wins': wins, 'losses': losses,
        'win_rate': wins / n * 100,
        'compound_return': (capital - initial_capital) / initial_capital * 100,
        'max_drawdown': drawdowns.min(),
        'total_pnl_sum': df['pnl_pct'].sum(),
    }


def apply_ma48_filter(df):
    """返回过滤后的信号序列：做多需 ma48>boll_mid，做空需 ma48<boll_mid。"""
    df = df.copy()
    long_cond = df['signal'] == 1
    short_cond = df['signal'] == -1
    reject = (long_cond & (df['ma48'] <= df['boll_mid'])) | \
             (short_cond & (df['ma48'] >= df['boll_mid']))
    df.loc[reject, 'signal'] = 0
    return df


def analyze_symbol(symbol):
    backtester = FinalBacktester(DATA_DIR)
    df = backtester.load_data(symbol)
    df = backtester.calculate_indicators(df, PARAMS)
    df['ma48'] = df['close'].rolling(window=48).mean()
    df = backtester.generate_signals(df)

    # 原始回测
    orig_trades, init_cap, _ = run_backtest_with_features(df, PARAMS)
    orig_stats = summarize(orig_trades, init_cap)

    # 过滤后的回测
    df_f = apply_ma48_filter(df)
    filt_trades, init_cap2, _ = run_backtest_with_features(df_f, PARAMS)
    filt_stats = summarize(filt_trades, init_cap2)

    # 信号层面：原始交易中会被过滤掉的
    tdf = pd.DataFrame(orig_trades)
    tdf['entry_time'] = pd.to_datetime(tdf['entry_time'])
    reject_mask = (
        ((tdf['direction'] == 'LONG') & (tdf['entry_ma48'] <= tdf['entry_boll_mid'])) |
        ((tdf['direction'] == 'SHORT') & (tdf['entry_ma48'] >= tdf['entry_boll_mid']))
    )
    rejected = tdf[reject_mask]
    kept = tdf[~reject_mask]

    n_orig_losers = int((tdf['pnl_pct'] <= 0).sum())
    n_orig_winners = int((tdf['pnl_pct'] > 0).sum())
    n_rej_losers = int((rejected['pnl_pct'] <= 0).sum())
    n_rej_winners = int((rejected['pnl_pct'] > 0).sum())
    rej_losers = rejected[rejected['pnl_pct'] <= 0]
    rej_winners = rejected[rejected['pnl_pct'] > 0]

    # 方向拆分
    rej_long = int((rejected['direction'] == 'LONG').sum())
    rej_short = int((rejected['direction'] == 'SHORT').sum())

    # 2025-2026 分年
    tdf['entry_time'] = pd.to_datetime(tdf['entry_time'])
    tdf['year'] = tdf['entry_time'].dt.year
    recent = tdf[tdf['year'] >= 2025]
    rej_recent = rejected[rejected['entry_time'].dt.year >= 2025]

    return {
        'symbol': symbol,
        'orig': orig_stats,
        'filt': filt_stats,
        'n_orig': len(tdf),
        'n_orig_losers': n_orig_losers,
        'n_orig_winners': n_orig_winners,
        'n_rejected': len(rejected),
        'n_rej_losers': n_rej_losers,
        'n_rej_winners': n_rej_winners,
        'rej_loss_pnl_sum': float(rej_losers['pnl_pct'].sum()) if n_rej_losers else 0.0,
        'rej_win_pnl_sum': float(rej_winners['pnl_pct'].sum()) if n_rej_winners else 0.0,
        'rej_long': rej_long,
        'rej_short': rej_short,
        'kept_win_rate': len(kept[kept['pnl_pct'] > 0]) / len(kept) * 100 if len(kept) else 0.0,
        'kept_count': len(kept),
        'recent_n': len(recent),
        'recent_rej': len(rej_recent),
        'recent_rej_losers': int((rej_recent['pnl_pct'] <= 0).sum()) if len(rej_recent) else 0,
        'recent_rej_winners': int((rej_recent['pnl_pct'] > 0).sum()) if len(rej_recent) else 0,
        'rejected_detail': rejected[['entry_time', 'direction', 'entry_price', 'pnl_pct', 'exit_reason']]
    }


def main():
    symbols = ['BTC', 'ETH', 'SOL', 'BNB', 'SUI', 'HYPE']
    all_rows = []
    btc_rejected = None

    for sym in symbols:
        try:
            r = analyze_symbol(sym)
        except Exception as e:
            print(f"{sym} 分析失败: {e}")
            continue

        o, f = r['orig'], r['filt']
        n_orig = r['n_orig']
        rej_lose_pct = r['n_rej_losers'] / r['n_orig_losers'] * 100 if r['n_orig_losers'] else 0
        rej_win_pct = r['n_rej_winners'] / r['n_orig_winners'] * 100 if r['n_orig_winners'] else 0

        all_rows.append({
            'symbol': sym,
            'orig_trades': o['trades'], 'orig_winrate': round(o['win_rate'], 1),
            'orig_compound': round(o['compound_return'], 1),
            'filt_trades': f['trades'], 'filt_winrate': round(f['win_rate'], 1),
            'filt_compound': round(f['compound_return'], 1),
            'rejected': r['n_rejected'],
            'rej_losers': r['n_rej_losers'], 'rej_losers_pct': round(rej_lose_pct, 1),
            'rej_winners': r['n_rej_winners'], 'rej_winners_pct': round(rej_win_pct, 1),
        })

        print(f"\n{'='*70}\n{sym}\n{'='*70}")
        print(f"原始: 交易 {o['trades']} | 胜率 {o['win_rate']:.1f}% | 复利 {o['compound_return']:.1f}% | 回撤 {o['max_drawdown']:.1f}%")
        print(f"过滤后(重模拟): 交易 {f['trades']} | 胜率 {f['win_rate']:.1f}% | 复利 {f['compound_return']:.1f}% | 回撤 {f['max_drawdown']:.1f}%")
        print(f"信号层: 共拒 {r['n_rejected']} 笔 (做多{r['rej_long']}/做空{r['rej_short']}) | "
              f"亏损单被拒 {r['n_rej_losers']} ({rej_lose_pct:.1f}%) | 盈利单被误杀 {r['n_rej_winners']} ({rej_win_pct:.1f}%)")
        print(f"被拒单盈亏合计: 亏损单 {r['rej_loss_pnl_sum']:.1f}pp | 盈利单 {r['rej_win_pnl_sum']:.1f}pp | "
              f"净 {r['rej_loss_pnl_sum'] + r['rej_win_pnl_sum']:.1f}pp")
        print(f"信号层保留交易: {r['kept_count']} 笔, 胜率 {r['kept_win_rate']:.1f}%")
        if r['recent_n']:
            print(f"2025-2026: 原始 {r['recent_n']} 笔, 被拒 {r['recent_rej']} 笔 "
                  f"(亏损单 {r['recent_rej_losers']} / 盈利单 {r['recent_rej_winners']})")

        if sym == 'BTC':
            btc_rejected = r['rejected_detail']

    print("\n\n===== 汇总表 =====")
    hdr = f"{'币种':<6}{'原始笔数':>8}{'原始胜率':>9}{'过滤笔数':>8}{'过滤胜率':>9}{'拒单数':>7}{'拒亏损单':>9}{'拒盈利单':>9}"
    print(hdr)
    for row in all_rows:
        print(f"{row['symbol']:<6}{row['orig_trades']:>8}{row['orig_winrate']:>8.1f}%{row['filt_trades']:>8}"
              f"{row['filt_winrate']:>8.1f}%{row['rejected']:>7}{row['rej_losers']:>9}{row['rej_winners']:>9}")

    # 保存 BTC 被拒交易明细
    if btc_rejected is not None:
        out = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'feature_report',
                           'strategy_c_btc_ma48_rejected_trades.csv')
        btc_rejected.to_csv(out, index=False, encoding='utf-8-sig')
        print(f"\nBTC 被 MA48 过滤拒掉的交易明细已保存: {out}")


if __name__ == '__main__':
    main()
