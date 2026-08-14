"""多维度监控指标预计算（纯 Python，无第三方依赖）。

对整段 30m K 线一次性预计算各指标序列（与 bars 对齐，长度 = n，窗口不足处为 None），
供回测在每笔交易的入场/出场时打快照，用于事后分析：
  - 哪些指标与盈亏相关
  - 哪些指标可以规避亏损入场
  - 盈利交易 / 大幅盈利交易的共同特征
  - 震荡/交织/箱体行情（MA288 与 MA488 反复交叉、长时间贴合的行情）检测

指标分组：
  A. 趋势分离与交织（核心假设：MA288/MA488 反复交叉、长期贴合的震荡行情里交易大多亏损）
  B. 趋势强度/动量（斜率、ADX、DI、Kaufman 效率比、价格离均线距离）
  C. 波动率（ATR、布林带宽、已实现波动、量比）
  D. 区间/箱体/摆动（Donchian 带宽、区间位置、RSI）

说明：
  - 所有 SMA 用简单平均（与前缀和 O(1) 计算），与生产策略一致。
  - ADX/RSI 用简单平滑（非 Wilder 平滑），仅作分析用，不影响策略逻辑。
  - 回测入场最早发生在第 slow(488) 根之后，因此各窗口指标在入场点基本都已就绪。
"""
from __future__ import annotations

from collections import deque
from typing import List, Optional

from ma_trend_pullback import KlineBar

FAST = 288
SLOW = 488

# MA288 与 MA488 视为"交织/贴合"的相对分离阈值（%）。震荡时两均线反复交叉、间距极小。
INTERWEAVE_THRESH = 0.5


def _prefix(values: List[float]) -> List[float]:
    p = [0.0] * (len(values) + 1)
    for i, v in enumerate(values):
        p[i + 1] = p[i] + v
    return p


def sma_series(values: List[float], period: int) -> List[Optional[float]]:
    """SMA 序列（窗口不足处 None）。"""
    n = len(values)
    p = _prefix(values)
    out: List[Optional[float]] = [None] * n
    if period <= 0 or n < period:
        return out
    for i in range(period - 1, n):
        out[i] = (p[i + 1] - p[i + 1 - period]) / period
    return out


def _rolling_sum(prefix: List[float], i: int, window: int) -> Optional[float]:
    if i < window - 1:
        return None
    return prefix[i + 1] - prefix[i + 1 - window]


def _rolling_max(values: List[float], window: int) -> List[Optional[float]]:
    n = len(values)
    out: List[Optional[float]] = [None] * n
    dq = deque()  # 存下标，值单调递减
    for i in range(n):
        v = values[i]
        while dq and values[dq[-1]] <= v:
            dq.pop()
        dq.append(i)
        if dq[0] <= i - window:
            dq.popleft()
        if i >= window - 1:
            out[i] = values[dq[0]]
    return out


def _rolling_min(values: List[float], window: int) -> List[Optional[float]]:
    n = len(values)
    out: List[Optional[float]] = [None] * n
    dq = deque()  # 存下标，值单调递增
    for i in range(n):
        v = values[i]
        while dq and values[dq[-1]] >= v:
            dq.pop()
        dq.append(i)
        if dq[0] <= i - window:
            dq.popleft()
        if i >= window - 1:
            out[i] = values[dq[0]]
    return out


class IndicatorSet:
    """预计算全部指标序列，并可按 bar 下标打快照。"""

    def __init__(self, bars: List[KlineBar]):
        self.bars = bars
        self.n = len(bars)
        self.closes = [b.close for b in bars]
        self.highs = [b.high for b in bars]
        self.lows = [b.low for b in bars]
        self.volumes = [b.volume for b in bars]
        self._build()

    # ------------------------------------------------------------------
    # 内部构建
    # ------------------------------------------------------------------
    def _build(self) -> None:
        c = self.closes
        n = self.n

        # --- 均线 ---
        self.sma_fast: List[Optional[float]] = sma_series(c, FAST)
        self.sma_slow: List[Optional[float]] = sma_series(c, SLOW)

        # --- 分离度 / 交织 ---
        spread_pct: List[Optional[float]] = [None] * n
        signed_spread_pct: List[Optional[float]] = [None] * n
        cross: List[float] = [0.0] * n
        interweave: List[float] = [0.0] * n
        trend_age: List[Optional[float]] = [None] * n
        last_cross: Optional[int] = None
        for i in range(n):
            f = self.sma_fast[i]
            s = self.sma_slow[i]
            if f is None or s is None or s == 0.0:
                continue
            signed_spread_pct[i] = (f - s) / s * 100.0
            spread_pct[i] = abs(f - s) / s * 100.0
            if spread_pct[i] < INTERWEAVE_THRESH:
                interweave[i] = 1.0
            if i >= 1 and self.sma_fast[i - 1] is not None and self.sma_slow[i - 1] is not None:
                prev_f = self.sma_fast[i - 1]
                prev_s = self.sma_slow[i - 1]
                d_prev = prev_f - prev_s
                d_now = f - s
                if d_prev * d_now < 0.0:  # 严格穿越（符号翻转）
                    cross[i] = 1.0
                    last_cross = i
            if last_cross is not None:
                trend_age[i] = float(i - last_cross)
        self.spread_pct = spread_pct
        self.signed_spread_pct = signed_spread_pct
        self.cross = cross
        self.interweave = interweave
        self.trend_age = trend_age

        # --- 滚动计数（前缀和） ---
        def rolling_count(bool_arr: List[float], window: int) -> List[Optional[float]]:
            p = _prefix(bool_arr)
            out: List[Optional[float]] = [None] * n
            for i in range(window - 1, n):
                out[i] = p[i + 1] - p[i + 1 - window]
            return out

        def rolling_mean(values: List[Optional[float]], window: int) -> List[Optional[float]]:
            # values 含 None，先把 None 当 0 累加，再按有效窗口近似（入场点后无 None，足够）
            v = [x if x is not None else 0.0 for x in values]
            p = _prefix(v)
            out: List[Optional[float]] = [None] * n
            for i in range(window - 1, n):
                out[i] = (p[i + 1] - p[i + 1 - window]) / window
            return out

        self.cross_count_48 = rolling_count(self.cross, 48)
        self.cross_count_96 = rolling_count(self.cross, 96)
        self.cross_count_288 = rolling_count(self.cross, 288)
        self.interweave_bars_48 = rolling_count(self.interweave, 48)
        self.interweave_bars_96 = rolling_count(self.interweave, 96)
        self.interweave_bars_288 = rolling_count(self.interweave, 288)
        self.mean_spread_96 = rolling_mean(self.spread_pct, 96)
        self.mean_spread_288 = rolling_mean(self.spread_pct, 288)

        # --- 斜率（%/根） ---
        self.ma288_slope_5 = self._slope(self.sma_fast, 5)
        self.ma288_slope_20 = self._slope(self.sma_fast, 20)
        self.ma488_slope_20 = self._slope(self.sma_slow, 20)

        # --- 价格离均线距离 ---
        self.close_to_ma288_pct: List[Optional[float]] = [None] * n
        self.close_to_ma488_pct: List[Optional[float]] = [None] * n
        for i in range(n):
            f = self.sma_fast[i]
            s = self.sma_slow[i]
            if f:
                self.close_to_ma288_pct[i] = (c[i] - f) / f * 100.0
            if s:
                self.close_to_ma488_pct[i] = (c[i] - s) / s * 100.0

        # --- 波动率：ATR / BBW / 已实现波动 / 量比 ---
        tr = [0.0] * n
        for i in range(n):
            h = self.highs[i]
            l = self.lows[i]
            pc = c[i - 1] if i > 0 else c[i]
            tr[i] = max(h - l, abs(h - pc), abs(l - pc))
        atr14 = sma_series(tr, 14)
        self.atr_pct_14: List[Optional[float]] = [None] * n
        for i in range(n):
            if atr14[i] is not None and c[i] != 0.0:
                self.atr_pct_14[i] = atr14[i] / c[i] * 100.0

        self.bbw_100 = self._rolling_bbw(c, 100)
        self.realized_vol_48 = self._rolling_std_returns(c, 48)

        vol_ratio: List[Optional[float]] = [None] * n
        for i in range(21, n):
            ma20 = sum(self.volumes[i - 20:i]) / 20.0
            if ma20 > 0.0:
                vol_ratio[i] = self.volumes[i] / ma20
        self.vol_ratio = vol_ratio

        # --- 区间/箱体/摆动 ---
        hi96 = _rolling_max(self.highs, 96)
        lo96 = _rolling_min(self.lows, 96)
        hi288 = _rolling_max(self.highs, 288)
        lo288 = _rolling_min(self.lows, 288)
        self.donchian_width_96: List[Optional[float]] = [None] * n
        self.donchian_width_288: List[Optional[float]] = [None] * n
        self.position_in_range_96: List[Optional[float]] = [None] * n
        for i in range(n):
            if hi96[i] is not None and lo96[i] is not None and c[i] != 0.0:
                self.donchian_width_96[i] = (hi96[i] - lo96[i]) / c[i] * 100.0
                rng = hi96[i] - lo96[i]
                if rng > 0.0:
                    self.position_in_range_96[i] = (c[i] - lo96[i]) / rng * 100.0
            if hi288[i] is not None and lo288[i] is not None and c[i] != 0.0:
                self.donchian_width_288[i] = (hi288[i] - lo288[i]) / c[i] * 100.0

        self.rsi14 = self._rsi(c, 14)
        self.adx14, self.di_spread = self._adx(14)

        # --- Kaufman 效率比（趋势效率，低=震荡/箱体） ---
        abs_d = [0.0] * n
        for i in range(1, n):
            abs_d[i] = abs(c[i] - c[i - 1])
        p_abs = _prefix(abs_d)
        self.efficiency_ratio_96: List[Optional[float]] = [None] * n
        for i in range(96, n):
            net = abs(c[i] - c[i - 96])
            path = p_abs[i + 1] - p_abs[i + 1 - 96]
            if path > 0.0:
                self.efficiency_ratio_96[i] = net / path

    @staticmethod
    def _slope(series: List[Optional[float]], k: int) -> List[Optional[float]]:
        n = len(series)
        out: List[Optional[float]] = [None] * n
        for i in range(k, n):
            cur = series[i]
            prev = series[i - k]
            if cur is not None and prev not in (None, 0.0):
                out[i] = (cur - prev) / prev * 100.0
        return out

    @staticmethod
    def _rolling_bbw(closes: List[float], period: int) -> List[Optional[float]]:
        n = len(closes)
        out: List[Optional[float]] = [None] * n
        p = _prefix(closes)
        p2 = _prefix([x * x for x in closes])
        for i in range(period - 1, n):
            mean = (p[i + 1] - p[i + 1 - period]) / period
            msq = (p2[i + 1] - p2[i + 1 - period]) / period
            var = msq - mean * mean
            if var < 0.0:
                var = 0.0
            if mean > 0.0:
                out[i] = 4.0 * (var ** 0.5) / mean * 100.0
        return out

    @staticmethod
    def _rolling_std_returns(closes: List[float], period: int) -> List[Optional[float]]:
        n = len(closes)
        out: List[Optional[float]] = [None] * n
        rets = [0.0] * n
        for i in range(1, n):
            if closes[i - 1] != 0.0:
                rets[i] = closes[i] / closes[i - 1] - 1.0
        p = _prefix(rets)
        p2 = _prefix([r * r for r in rets])
        for i in range(period, n):
            mean = (p[i + 1] - p[i + 1 - period]) / period
            msq = (p2[i + 1] - p2[i + 1 - period]) / period
            var = msq - mean * mean
            if var < 0.0:
                var = 0.0
            out[i] = (var ** 0.5) * 100.0
        return out

    @staticmethod
    def _rsi(closes: List[float], period: int) -> List[Optional[float]]:
        n = len(closes)
        gains = [0.0] * n
        losses = [0.0] * n
        for i in range(1, n):
            d = closes[i] - closes[i - 1]
            if d > 0.0:
                gains[i] = d
            elif d < 0.0:
                losses[i] = -d
        pg = _prefix(gains)
        pl = _prefix(losses)
        out: List[Optional[float]] = [None] * n
        for i in range(period, n):
            ag = (pg[i + 1] - pg[i + 1 - period]) / period
            al = (pl[i + 1] - pl[i + 1 - period]) / period
            if al == 0.0:
                out[i] = 100.0 if ag > 0.0 else 50.0
            else:
                rs = ag / al
                out[i] = 100.0 - 100.0 / (1.0 + rs)
        return out

    def _adx(self, period: int):
        """返回 (adx, di_spread=+DI - -DI)。简单平滑（非 Wilder）。"""
        n = self.n
        c = self.closes
        h = self.highs
        l = self.lows
        plus_dm = [0.0] * n
        minus_dm = [0.0] * n
        tr = [0.0] * n
        for i in range(1, n):
            up = h[i] - h[i - 1]
            down = l[i - 1] - l[i]
            plus_dm[i] = up if (up > down and up > 0.0) else 0.0
            minus_dm[i] = down if (down > up and down > 0.0) else 0.0
            pc = c[i - 1]
            tr[i] = max(h[i] - l[i], abs(h[i] - pc), abs(l[i] - pc))
        sma_pdm = sma_series(plus_dm, period)
        sma_mdm = sma_series(minus_dm, period)
        sma_tr = sma_series(tr, period)
        adx: List[Optional[float]] = [None] * n
        di_spread: List[Optional[float]] = [None] * n
        dx: List[Optional[float]] = [None] * n
        for i in range(period - 1, n):
            a = sma_pdm[i]
            b = sma_mdm[i]
            t = sma_tr[i]
            if t is None or t == 0.0:
                continue
            pdi = 100.0 * (a if a is not None else 0.0) / t
            mdi = 100.0 * (b if b is not None else 0.0) / t
            di_spread[i] = pdi - mdi
            denom = pdi + mdi
            if denom != 0.0:
                dx[i] = 100.0 * abs(pdi - mdi) / denom
        # ADX = SMA(dx, period)
        p_dx = _prefix([x if x is not None else 0.0 for x in dx])
        for i in range(2 * period - 2, n):
            adx[i] = (p_dx[i + 1] - p_dx[i + 1 - period]) / period
        return adx, di_spread

    # ------------------------------------------------------------------
    # 快照
    # ------------------------------------------------------------------
    FEATURE_NAMES = [
        # A. 趋势分离与交织
        "spread_pct", "signed_spread_pct", "trend_age",
        "cross_count_48", "cross_count_96", "cross_count_288",
        "interweave_bars_48", "interweave_bars_96", "interweave_bars_288",
        "mean_spread_96", "mean_spread_288",
        # B. 趋势强度/动量
        "ma288_slope_5", "ma288_slope_20", "ma488_slope_20",
        "adx14", "di_spread", "efficiency_ratio_96",
        "close_to_ma288_pct", "close_to_ma488_pct",
        # C. 波动率
        "atr_pct_14", "bbw_100", "realized_vol_48", "vol_ratio",
        # D. 区间/箱体/摆动
        "donchian_width_96", "donchian_width_288", "position_in_range_96", "rsi14",
    ]

    def snapshot(self, i: int) -> dict:
        out = {}
        for name in self.FEATURE_NAMES:
            out[name] = getattr(self, name)[i]
        return out
