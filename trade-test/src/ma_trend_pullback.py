"""MA 趋势回踩策略 —— Rust 源码的忠实 Python 复刻（用于测试/校验）。

源文件（唯一事实来源）：
  F:/rust-projects/rust-trade/trading-common/src/strategy/ma_trend_pullback.rs

本模块只做一件事：把 Rust 版 `analyze()` 的算术逻辑逐行翻译成 Python，
不引入任何"优化"或"修正"。这样测试脚本就能用同一份 K 线数据，重新计算
策略"本应"给出的信号字段，再与生产环境的 strategy_signals 表逐项对比，
从而统计出"数据（K线）"与"信号"之间的差异。

关键复刻点（与 Rust 完全一致）：
  1. SMA 取"最后 period 根 close 的简单平均"（`calculate_sma`）。
  2. 趋势用 30m MA288 vs MA488 判断（`>` 多头，`<` 空头，`==` 中性跳过）。
  3. 入场（entry_timeframe="30m"）穿越判定使用：
       前一根 K 线 close（prev_close）对比"前一根窗口"的 fast_ma，
       当前 K 线 close 对比"当前窗口"的 fast_ma。
       多头: prev_close < prev_fast_ma 且 close > fast_ma
       空头: prev_close > prev_fast_ma 且 close < fast_ma
  4. 止损：hard_stop_pct > 0 时用硬止损（相对 current_price），否则用 MA288*0.98/1.02。
  5. 信号强度 signal_strength = min(|fast_ma - slow_ma| / slow_ma * 100 / 5.0, 1.0)。
  6. confidence 恒为 0.75，take_profit 恒为 None（移动止盈不写入信号字段）。
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import List, Optional, Dict, Any


# =====================================================================
# 数据结构（对应 Rust 的 KlineBar / MarketData / Signal / Params）
# =====================================================================

@dataclass
class KlineBar:
    """简化 K 线，字段与 Rust KlineBar 一一对应。"""
    open_time: int = 0            # 附加字段：K 线开盘时间（UTC epoch ms），Rust 里没有，测试用
    open: float = 0.0
    high: float = 0.0
    low: float = 0.0
    close: float = 0.0
    volume: float = 0.0


@dataclass
class Params:
    """对应 Rust MATrendPullbackParams。默认值严格对齐 Rust 的 Default 实现。"""
    fast_ma_period: int = 288
    slow_ma_period: int = 488
    stop_mode: str = "ma288"          # "ma288" | "fixed"
    fixed_stop_pct: float = 2.0
    hard_stop_pct: float = 0.0        # >0 时启用硬止损（覆盖 ma288 止损分支）
    take_profit_mode: str = "trailing"
    trailing_activate_pct: float = 5.0
    trailing_callback_pct: float = 5.0
    slope_threshold: float = 0.0
    bbw_threshold: float = 0.0
    vol_threshold: float = 0.0
    realized_vol_threshold: float = 0.0  # 48周期已实现波动率过滤（>=阈值跳过入场），对齐生产
    use_30m_expanding: bool = False
    use_5m_expanding: bool = False
    min_angle_5m: float = 0.0
    entry_timeframe: str = "30m"      # "30m" | "5m"


@dataclass
class Signal:
    """对应 Rust Signal。"""
    signal_type: str = "HOLD"          # "BUY" | "SELL" | "HOLD"
    signal_strength: float = 0.0
    entry_price: float = 0.0
    stop_loss: Optional[float] = None
    take_profit: Optional[float] = None
    confidence: float = 0.75
    reason: str = ""
    market_context: Dict[str, Any] = field(default_factory=dict)


# =====================================================================
# 指标计算（逐行对齐 Rust）
# =====================================================================

def extract_closes(klines: List[KlineBar]) -> List[float]:
    return [k.close for k in klines]


def calculate_sma(klines: List[KlineBar], period: int) -> Optional[float]:
    """简单移动平均 —— 只返回"最新一根"的值。对应 Rust calculate_sma。"""
    if len(klines) < period or period == 0:
        return None
    closes = extract_closes(klines)
    start = len(closes) - period
    return sum(closes[start:]) / period


def calculate_sma_series(closes: List[float], period: int) -> List[Optional[float]]:
    """SMA 序列。对应 Rust calculate_sma_series。"""
    if len(closes) < period or period == 0:
        return [None] * len(closes)
    result: List[Optional[float]] = [None] * len(closes)
    for i in range(period - 1, len(closes)):
        result[i] = sum(closes[i + 1 - period: i + 1]) / period
    return result


def calculate_slope(ma_values: List[Optional[float]], lookback: int) -> Optional[float]:
    """MA 斜率（万分比）。对应 Rust calculate_slope。"""
    if len(ma_values) < lookback + 1:
        return None
    current = ma_values[-1]
    prev = ma_values[len(ma_values) - 1 - lookback]
    if current is None or prev is None:
        return None
    if prev != 0.0:
        return (current - prev) / prev * 10000.0
    return None


def calculate_bbw(klines: List[KlineBar], period: int) -> Optional[float]:
    """布林带宽度百分比。对应 Rust calculate_bbw。"""
    if len(klines) < period:
        return None
    closes = extract_closes(klines)
    start = len(closes) - period
    recent = closes[start:]
    mean = sum(recent) / period
    variance = sum((x - mean) ** 2 for x in recent) / period
    std = variance ** 0.5
    if mean > 0.0:
        return 4.0 * std / mean * 100.0
    return None


def calculate_vol_ratio(klines: List[KlineBar]) -> Optional[float]:
    """当前量 / MA20 量。对应 Rust calculate_vol_ratio。"""
    if len(klines) < 21:
        return None
    volumes = [k.volume for k in klines]
    current = volumes[-1]
    ma20 = sum(volumes[-21:-1]) / 20.0
    if ma20 > 0.0:
        return current / ma20
    return None


def calculate_spread(klines: List[KlineBar], fast: int, slow: int) -> Optional[float]:
    fast_ma = calculate_sma(klines, fast)
    slow_ma = calculate_sma(klines, slow)
    if fast_ma is None or slow_ma is None:
        return None
    return fast_ma - slow_ma


def is_expanding(klines: List[KlineBar], fast: int, slow: int, lookback: int) -> Optional[bool]:
    if len(klines) < slow + lookback:
        return None
    current = calculate_spread(klines, fast, slow)
    prev = calculate_spread(klines[:len(klines) - lookback], fast, slow)
    if current is None or prev is None:
        return None
    return abs(current) > abs(prev)


# =====================================================================
# 趋势方向
# =====================================================================

class TrendDirection:
    Bullish = "Bullish"
    Bearish = "Bearish"
    Neutral = "Neutral"


# =====================================================================
# 策略主体（对应 Rust MATrendPullbackStrategy::analyze）
# =====================================================================

def analyze(
    params: Params,
    klines: List[KlineBar],
    current_price: float,
    symbol: str = "",
    timeframe: str = "30m",
    klines_5m: Optional[List[KlineBar]] = None,
) -> Optional[Signal]:
    """复刻 Rust analyze()：返回 Option<Signal>。

    注意：本函数要求传入的 `klines` 已经是"升序（最旧→最新）"且末尾为当前 K 线。
    生产环境策略服务传入的 K 线数量为 500 根（market_context.kline_count=500）。
    """
    min_bars = max(params.slow_ma_period, params.fast_ma_period) + 10  # 488 + 10 = 498
    if len(klines) < min_bars:
        return None

    closes = extract_closes(klines)

    # 30m 趋势判断
    fast_ma = calculate_sma(klines, params.fast_ma_period)
    slow_ma = calculate_sma(klines, params.slow_ma_period)
    if fast_ma is None or slow_ma is None:
        return None

    if fast_ma > slow_ma:
        trend = TrendDirection.Bullish
    elif fast_ma < slow_ma:
        trend = TrendDirection.Bearish
    else:
        trend = TrendDirection.Neutral

    spread_pct = abs(fast_ma - slow_ma) / slow_ma * 100.0

    if trend == TrendDirection.Neutral:
        return None

    # 过滤器（本批次参数全部为 0 / false，逐一保留以便复刻分支，但实际不会触发）
    if params.slope_threshold > 0.0:
        fast_ma_series = calculate_sma_series(closes, params.fast_ma_period)
        slope = calculate_slope(fast_ma_series, 5)
        if slope is not None and abs(slope) < params.slope_threshold:
            return None

    if params.bbw_threshold > 0.0:
        bbw = calculate_bbw(klines, 100)
        if bbw is not None and bbw < params.bbw_threshold:
            return None

    if params.vol_threshold > 0.0:
        vol_ratio = calculate_vol_ratio(klines)
        if vol_ratio is not None and vol_ratio < params.vol_threshold:
            return None

    if params.use_30m_expanding:
        expanding = is_expanding(klines, params.fast_ma_period, params.slow_ma_period, 5)
        if expanding is not None and not expanding:
            return None

    if params.use_5m_expanding and klines_5m is not None:
        expanding = is_expanding(klines_5m, params.fast_ma_period, params.slow_ma_period, 5)
        if expanding is not None and not expanding:
            return None
        # min_angle_5m 本批次为 0，略过角度过滤（复刻占位）

    # 入场 K 线（entry_timeframe）
    entry_klines: List[KlineBar] = klines
    if params.entry_timeframe == "5m" and klines_5m is not None and len(klines_5m) >= 2:
        entry_klines = klines_5m

    if len(entry_klines) < 2:
        return None

    entry_fast_ma = calculate_sma(entry_klines, params.fast_ma_period)
    if entry_fast_ma is None:
        return None
    prev_entry_klines = entry_klines[:-1]
    prev_entry_fast_ma = calculate_sma(prev_entry_klines, params.fast_ma_period)

    # 用"前一根 K 线 close"判断穿越前状态（与 JS 回测保持一致，见 Rust 注释）
    prev_close = entry_klines[-2].close
    open_ = entry_klines[-1].open
    close_ = entry_klines[-1].close

    signal_type = None
    reason = ""

    if trend == TrendDirection.Bullish:
        if prev_entry_fast_ma is not None:
            crossed = (prev_close < prev_entry_fast_ma) and (close_ > entry_fast_ma)
            if crossed:
                signal_type = "BUY"
                reason = (
                    f"Bullish trend pullback: price crossed above MA{params.fast_ma_period} "
                    f"on {params.entry_timeframe} "
                    f"(trend: MA{params.fast_ma_period} > MA{params.slow_ma_period})"
                )
    elif trend == TrendDirection.Bearish:
        if prev_entry_fast_ma is not None:
            crossed = (prev_close > prev_entry_fast_ma) and (close_ < entry_fast_ma)
            if crossed:
                signal_type = "SELL"
                reason = (
                    f"Bearish trend pullback: price crossed below MA{params.fast_ma_period} "
                    f"on {params.entry_timeframe} "
                    f"(trend: MA{params.fast_ma_period} < MA{params.slow_ma_period})"
                )

    if signal_type is None:
        return None

    # 止损（对应 Rust：hard_stop_pct>0 走硬止损分支，否则 ma288 分支）
    if params.hard_stop_pct > 0.0:
        if signal_type == "BUY":
            stop_loss = current_price * (1.0 - params.hard_stop_pct / 100.0)
        elif signal_type == "SELL":
            stop_loss = current_price * (1.0 + params.hard_stop_pct / 100.0)
        else:
            stop_loss = None
    else:
        if signal_type == "BUY":
            stop_loss = fast_ma * 0.98
        elif signal_type == "SELL":
            stop_loss = fast_ma * 1.02
        else:
            stop_loss = None

    take_profit = None

    ma_separation = abs(fast_ma - slow_ma) / slow_ma * 100.0
    signal_strength = min(ma_separation / 5.0, 1.0)

    market_context = {
        "fast_ma": fast_ma,
        "slow_ma": slow_ma,
        "fast_ma_period": params.fast_ma_period,
        "slow_ma_period": params.slow_ma_period,
        "trend": trend,
        "current_price": current_price,
        "open": open_,
        "close": close_,
        "timeframe": timeframe,
        "entry_timeframe": params.entry_timeframe,
        "kline_count": len(klines),
        "stop_mode": "Ma288" if params.stop_mode == "ma288" else "Fixed",
        "hard_stop_pct": params.hard_stop_pct,
        "take_profit_mode": "Trailing" if params.take_profit_mode == "trailing" else "None",
        "trailing_activate_pct": params.trailing_activate_pct,
        "trailing_callback_pct": params.trailing_callback_pct,
        "use_30m_expanding": params.use_30m_expanding,
        "use_5m_expanding": params.use_5m_expanding,
        "min_angle_5m": params.min_angle_5m,
    }

    return Signal(
        signal_type=signal_type,
        signal_strength=signal_strength,
        entry_price=current_price,   # Rust: entry_price = current_price
        stop_loss=stop_loss,
        take_profit=take_profit,
        confidence=0.75,
        reason=reason,
        market_context=market_context,
    )


# =====================================================================
# 各币种参数（以生产信号表中的 instance_id 为准，修正 CLAUDE.md 中 ETH/SOL 标题错位）
# =====================================================================

# CLAUDE.md 里 "ETH:" 标题下的行其实是 SOLUSDT 的配置（id=6beb73a5、display_name=MA趋势回踩-SOL），
# "SOL:" 标题下的行其实是 ETHUSDT 的配置（id=f56ad8cc、display_name=MA趋势回踩-ETH）。
# 生产信号表 market_context 是权威数据，据此以 symbol 建立映射：
SYMBOL_PARAMS: Dict[str, Params] = {
    "BTCUSDT": Params(
        fast_ma_period=288, slow_ma_period=480,
        stop_mode="ma288", hard_stop_pct=1.5,
        take_profit_mode="trailing", trailing_activate_pct=4.0, trailing_callback_pct=1.0,
        slope_threshold=0.0, bbw_threshold=0.0, vol_threshold=0.0, realized_vol_threshold=0.426,
        use_30m_expanding=False, use_5m_expanding=False, min_angle_5m=0.0,
        entry_timeframe="30m",
    ),
    "ETHUSDT": Params(
        fast_ma_period=288, slow_ma_period=480,
        stop_mode="ma288", hard_stop_pct=1.5,
        take_profit_mode="trailing", trailing_activate_pct=5.0, trailing_callback_pct=1.0,
        slope_threshold=0.0, bbw_threshold=0.0, vol_threshold=0.0, realized_vol_threshold=0.445,
        use_30m_expanding=False, use_5m_expanding=False, min_angle_5m=0.0,
        entry_timeframe="30m",
    ),
    "SOLUSDT": Params(
        fast_ma_period=288, slow_ma_period=480,
        stop_mode="ma288", hard_stop_pct=2.0,
        take_profit_mode="trailing", trailing_activate_pct=4.0, trailing_callback_pct=1.0,
        slope_threshold=0.0, bbw_threshold=0.0, vol_threshold=0.0, realized_vol_threshold=0.790,
        use_30m_expanding=False, use_5m_expanding=False, min_angle_5m=0.0,
        entry_timeframe="30m",
    ),
}

# instance_id -> symbol（来自生产信号表，用于交叉校验）
INSTANCE_SYMBOL: Dict[str, str] = {
    "32eba113-71ee-4718-b322-e2efb849ecc3": "BTCUSDT",
    "f56ad8cc-adb4-42b9-b141-990165e29b9c": "ETHUSDT",
    "6beb73a5-a532-4023-aef7-b7819cde33fb": "SOLUSDT",
}
