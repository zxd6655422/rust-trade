#!/usr/bin/env python3
"""
逐币诊断：为什么 BNB/SOL/SUI/HYPE 在 C2(BOLL_LOW+V3) 下效果不理想

每个币输出：
  1. 数据跨度 / 信号数 / 交易数 / 逐年盈亏结构(胜率/平均盈/平均亏/PF/多空拆分/离场原因)
  2. 全样本：胜率 / PF / 平均盈亏 / 平均持仓 / 手续费占本金pp
  3. 特征判别力：入场特征 赢家均值 vs 输家均值（找每个币真正有效的过滤器）
"""
import os
import sys
import pandas as pd
import numpy as np
import warnings
warnings.filterwarnings('ignore')

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from strategy_c_final_backtest import FinalBacktester
from btc_ma48_filter_analysis import DATA_DIR, PARAMS
from btc_filter_variants import build_reject_mask

FEATURES = ['compression_bars', 'boll_width_pct', 'rsi', 'atr_pct',
            'volume_ratio', 'momentum_5', 'ma192_slope', 'price_position']


def calc_features(df):
    """与 strategy_c_trade_analysis.calculate_indicators 一致的附加特征。"""
    df = df.copy()
    delta = df['close'].diff()
    gain = (delta.where(delta > 0, 0)).rolling(window=14).mean()
    loss = (-delta.where(delta < 0, 0)).rolling(window=14).mean()
    rs = gain / loss
    df['rsi'] = 100 - (100 / (1 + rs))
    high_low = df['high'] - df['low']
    high_close = np.abs(df['high'] - df['close'].shift())
    low_close = np.abs(df['low'] - df['close'].shift())
    ranges = pd.concat([high_low, high_close, low_close], axis=1)
    df['atr'] = np.max(ranges, axis=1).rolling(window=14).mean()
    df['atr_pct'] = df['atr'] / df['close'] * 100
    df['volume_ma'] = df['volume'].rolling(window=20).mean()
    df['volume_ratio'] = df['volume'] / df['volume_ma']
    df['momentum_5'] = df['close'].pct_change(periods=5) * 100
    df['price_position'] = (df['close'] - df['boll_lower']) / (df['boll_upper'] - df['boll_lower'])
    df['ma192_slope'] = df['ma192'].pct_change(periods=5) * 100
    return df


def backtest_c2_with_features(df, params, use_v3=True):
    """C2 = BOLL_LOW 出场 + V3(压缩时长<=60) 入场过滤，逐笔记录入场特征。"""
    df = df.copy()
    if use_v3:
        mask = build_reject_mask(df, 'V3')
        df.loc[mask, 'signal'] = 0

    hard_stop_pct = params.get('hard_stop_pct', 2.0)
    initial_capital = 10000.0
    capital = initial_capital
    position = 0
    entry_price = 0.0
    entry_time = None
    entry_idx = 0
    entry_feats = {}
    trades = []

    for i in range(1, len(df)):
        bar = df.iloc[i]

        if position != 0:
            if position == 1:
                cur_profit = (bar['close'] - entry_price) / entry_price * 100
            else:
                cur_profit = (entry_price - bar['close']) / entry_price * 100

            exit_reason = None
            exit_price = bar['close']
            exit_time = bar['open_time']
            bars_held = i - entry_idx

            if cur_profit <= -hard_stop_pct:
                exit_reason = 'hard_stop'
            elif position == 1 and bar['close'] < bar['boll_lower']:
                exit_reason = 'boll_lower_stop'
            elif position == -1 and bar['close'] > bar['boll_upper']:
                exit_reason = 'boll_lower_stop'

            if exit_reason:
                if position == 1:
                    pnl_pct = (exit_price - entry_price) / entry_price * 100
                else:
                    pnl_pct = (entry_price - exit_price) / entry_price * 100
                pnl_amount = capital * (pnl_pct / 100)
                capital += pnl_amount
                trade = {'entry_time': entry_time, 'exit_time': exit_time,
                         'direction': 'LONG' if position == 1 else 'SHORT',
                         'entry_price': entry_price, 'exit_price': exit_price,
                         'pnl_pct': pnl_pct, 'pnl_amount': pnl_amount,
                         'exit_reason': exit_reason, 'bars_held': bars_held}
                trade.update(entry_feats)
                trades.append(trade)
                position = 0
                entry_price = 0.0
                entry_time = None

        if position == 0 and bar['signal'] != 0:
            entry_price = bar['close']
            entry_time = bar['open_time']
            entry_idx = i
            entry_feats = {f: bar.get(f, np.nan) for f in FEATURES}
            position = bar['signal']

    return trades, initial_capital, capital


def analyze_coin(symbol):
    bt = FinalBacktester(DATA_DIR)
    df = bt.load_data(symbol)
    df = bt.calculate_indicators(df, PARAMS)
    df = calc_features(df)
    df = bt.generate_signals(df)

    n_signals = int((df['signal'] != 0).sum())
    trades, init_cap, final_cap = backtest_c2_with_features(df, PARAMS, use_v3=True)
    tdf = pd.DataFrame(trades)
    tdf['entry_time'] = pd.to_datetime(tdf['entry_time'])
    tdf['year'] = tdf['entry_time'].dt.year

    print(f"\n{'#'*90}\n{symbol}  (C2 = BOLL_LOW + 压缩≤60)\n{'#'*90}")
    print(f"数据: {df['open_time'].min()} ~ {df['open_time'].max()} | 原始信号 {n_signals} | "
          f"C2 交易 {len(tdf)} 笔 | 复利 {(final_cap-init_cap)/init_cap*100:.1f}%")

    # 逐年
    print(f"\n{'年份':<6}{'笔数':>5}{'胜率':>7}{'均盈':>8}{'均亏':>8}{'PF':>7} | "
          f"{'多:笔/胜率/盈亏':>24} | {'空:笔/胜率/盈亏':>24} | 离场(BL/硬)")
    for year, g in sorted(tdf.groupby('year')):
        wins = g[g['pnl_pct'] > 0]
        losses = g[g['pnl_pct'] <= 0]
        wr = len(wins) / len(g) * 100 if len(g) else 0
        aw = wins['pnl_pct'].mean() if len(wins) else 0
        al = losses['pnl_pct'].mean() if len(losses) else 0
        pf = abs(aw / al) if al else float('inf')
        longs = g[g['direction'] == 'LONG']; shorts = g[g['direction'] == 'SHORT']
        lw = len(longs[longs['pnl_pct'] > 0]) / len(longs) * 100 if len(longs) else 0
        sw = len(shorts[shorts['pnl_pct'] > 0]) / len(shorts) * 100 if len(shorts) else 0
        reasons = g['exit_reason'].value_counts().to_dict()
        bl = reasons.get('boll_lower_stop', 0); hd = reasons.get('hard_stop', 0)
        print(f"{year:<6}{len(g):>5}{wr:>6.1f}%{aw:>7.2f}%{al:>7.2f}%{pf:>7.2f} | "
              f"{len(longs):>3}/{lw:>4.1f}%/{longs['pnl_pct'].sum():>8.1f} | "
              f"{len(shorts):>3}/{sw:>4.1f}%/{shorts['pnl_pct'].sum():>8.1f} | {bl}/{hd}")

    # 全样本
    wins = tdf[tdf['pnl_pct'] > 0]; losses = tdf[tdf['pnl_pct'] <= 0]
    pf = abs(wins['pnl_pct'].mean() / losses['pnl_pct'].mean()) if len(losses) else float('inf')
    fee_pp = len(tdf) * 0.10  # taker 双边 0.1% / 笔，占本金pp
    print(f"\n全样本: 胜率 {len(wins)/len(tdf)*100:.1f}% | PF {pf:.2f} | 平均盈 {wins['pnl_pct'].mean():.2f}% | "
          f"平均亏 {losses['pnl_pct'].mean():.2f}% | 平均持仓 {tdf['bars_held'].mean():.1f}根 | "
          f"taker费合计≈{fee_pp:.0f}pp(本金)")

    # 特征判别力
    print(f"\n入场特征判别(赢家均值 vs 输家均值):")
    for f in FEATURES:
        wm = wins[f].mean(); lm = losses[f].mean()
        if pd.isna(wm) or pd.isna(lm) or lm == 0:
            diff = np.nan
        else:
            diff = (wm - lm) / abs(lm) * 100
        flag = '  <-- 强判别' if abs(diff) > 15 else ''
        print(f"  {f:<18} 赢 {wm:>10.3f} | 输 {lm:>10.3f} | 差 {diff:>7.1f}%{flag}")


def main():
    for sym in ['BTC', 'ETH', 'SOL', 'BNB', 'SUI', 'HYPE']:
        try:
            analyze_coin(sym)
        except Exception as e:
            print(f"{sym} 失败: {e}")


if __name__ == '__main__':
    main()
