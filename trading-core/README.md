# Trading Core

A professional-grade cryptocurrency data collection and backtesting system built in Rust, designed for real-time market data processing, storage, and quantitative strategy analysis.

## 🏗️ Architecture

### **System Overview**
```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Exchange      │───▶│    Service      │───▶│   Repository    │
│   (WebSocket)   │    │  (Processing)   │    │   (Storage)     │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                       │
         │                       ▼                       ▼
    Binance API           ┌─────────────┐         ┌─────────────┐
    - Real-time data      │   Cache     │         │ PostgreSQL  │
    - Historical data     │ (L1 + L2)   │         │ Database    │
                          └─────────────┘         └─────────────┘
                                    │
                                    ▼
                          ┌─────────────────┐
                          │   Backtest      │
                          │   Engine        │
                          └─────────────────┘
```

### **Dual-Mode Operation**

#### **Live Trading Mode**
```
Exchange API → Service → Repository → Database + Cache
```

#### **Backtesting Mode**
```
Database → Repository → Backtest Engine → Strategy → Portfolio → Metrics
```


## ✨ Features

### 🚀 **High Performance**
- **Asynchronous Architecture**: Built with Tokio for maximum concurrency
- **Optimized Database Operations**: 
  - Single tick insert: ~390µs
  - Batch insert (100 ticks): ~13ms
  - Batch insert (1000 ticks): ~116ms
- **Multi-level Caching**: L1 (Memory) + L2 (Redis) with microsecond access times
- **Smart Query Optimization**: Cache hit ~10µs vs cache miss ~11.6ms

### 🛡️ **Reliability**
- **Automatic Retry**: Database failures with exponential backoff
- **Data Integrity**: Duplicate detection using unique constraints
- **Graceful Shutdown**: Zero data loss during termination
- **Error Isolation**: Cache failures don't impact main data flow

### 📊 **Backtesting System**
- **Multi-Strategy Framework**: Built-in SMA and RSI strategies
- **Professional Metrics**: Sharpe ratio, max drawdown, win rate, profit factor
- **Portfolio Management**: Real-time P&L tracking and position management
- **Interactive CLI**: User-friendly backtesting interface
- **Historical Data Processing**: ~450µs per query with optimized indexing

### 🔧 **Flexible Configuration**
- **Dual Mode Operation**: Live data collection and backtesting
- **Multi-Environment Support**: Development, production configurations
- **Environment Variable Overrides**: Secure configuration management
- **Symbol Configuration**: Easily configure trading pairs to monitor

## 🚀 Quick Start

### **Prerequisites**
- Rust 1.70+
- PostgreSQL 12+
- Redis 6+

### **Installation**

1. **Clone and setup**
   ```bash
   git clone https://github.com/Erio-Harrison/rust-trade.git
   cd trading-core
   ```

2. **Database setup**
   ```sql
   CREATE DATABASE trading_core;
   \i database/schema.sql
   ```

3. **Environment configuration**
   ```bash
   # .env file
   DATABASE_URL=postgresql://user:password@localhost/trading_core
   REDIS_URL=redis://127.0.0.1:6379
   RUN_MODE=development
   ```

4. **Symbol configuration**
   ```toml
   # config/development.toml
   symbols = ["BTCUSDT", "ETHUSDT", "ADAUSDT"]
   ```

### **Running the Application**

#### **Service Mode (Recommended for 24/7)**
```bash
# Start full service with data collection + API + WebSocket
cargo run service
```

This will start:
- Data collection (candle1m/tick based on config)
- HTTP API server on port 8080
- WebSocket for real-time data

#### **API Endpoints**
```bash
# Health check
curl http://localhost:8080/health

# Get strategies
curl http://localhost:8080/api/strategies

# Get data info
curl http://localhost:8080/api/data/info

# Run backtest
curl -X POST http://localhost:8080/api/backtest \
  -H "Content-Type: application/json" \
  -d '{"strategy": "rsi", "symbol": "BTCUSDT", "capital": 10000, "data_count": 10000}'

# WebSocket connection
ws://localhost:8080/ws
```

#### **Collector Mode (Data Collection Only)**
```bash
# Start data collector only
cargo run collector
```

#### **Backtesting (CLI)**
```bash
# Start interactive backtesting
cargo run backtest
```

#### **Live Mode (Legacy)**
```bash
# Start real-time data collection (legacy)
cargo run live
```

#### **Help**
```bash
cargo run -- --help
```

## 📊 Performance Benchmarks

Based on comprehensive benchmarking results:

| Operation | Performance | Notes |
|-----------|-------------|-------|
| Single tick insert | ~390µs | Individual database writes |
| Batch insert (100) | ~13ms | Optimized bulk operations |
| Batch insert (1000) | ~116ms | Large batch processing |
| Cache hit | ~10µs | Memory/Redis retrieval |
| Cache miss | ~11.6ms | Database fallback |
| Historical query | ~450µs | Backtest data retrieval |
| Cache operations | ~17-104µs | Push/pull operations |

## 🏗️ Project Structure

This crate is part of a workspace with `trading-common` (shared library) and `src-tauri` (desktop app).

```
trading-core/
├── src/
│   ├── main.rs                # CLI entry point with live/backtest modes
│   ├── lib.rs                 # Library entry (re-exports trading-common)
│   ├── config.rs              # Configuration management (Settings, env vars)
│   ├── exchange/              # Exchange integrations
│   │   ├── mod.rs             # Module exports
│   │   ├── traits.rs          # Exchange interface definition
│   │   ├── types.rs           # Exchange-specific data structures
│   │   ├── errors.rs          # Exchange error types
│   │   ├── utils.rs           # Conversion and validation utilities
│   │   └── binance.rs         # Binance WebSocket implementation
│   ├── service/               # Business logic layer (Live trading)
│   │   ├── mod.rs             # Module exports
│   │   ├── types.rs           # Service types (BatchConfig, stats)
│   │   ├── errors.rs          # Service error types
│   │   └── market_data.rs     # Main data processing service
│   └── live_trading/          # Live trading system
│       ├── mod.rs             # Module exports
│       └── paper_trading.rs   # Paper trading implementation
├── benches/                   # Performance benchmarks
└── Cargo.toml

trading-common/                # Shared library (separate crate)
├── src/
│   ├── lib.rs                 # Library entry point
│   ├── data/                  # Data layer
│   │   ├── types.rs           # Core data types (TickData, OHLC, errors)
│   │   ├── repository.rs      # Database operations
│   │   └── cache.rs           # Multi-level caching (L1 + L2)
│   └── backtest/              # Backtesting system
│       ├── engine.rs          # Core backtesting engine
│       ├── portfolio.rs       # Portfolio management, P&L tracking
│       ├── metrics.rs         # Performance metrics (Sharpe, drawdown)
│       └── strategy/          # Trading strategies (SMA, RSI)
└── Cargo.toml
```

## ⚙️ Configuration

### **Environment Variables**
| Variable | Description | Example |
|----------|-------------|---------|
| `DATABASE_URL` | PostgreSQL connection | `postgresql://user:pass@localhost/trading_core` |
| `REDIS_URL` | Redis connection | `redis://127.0.0.1:6379` |
| `RUN_MODE` | Environment mode | `development` / `production` |
| `RUST_LOG` | Logging level | `trading_core=info` |

### **Configuration Structure**
```
config/
├── development.toml    # Development settings
├── production.toml     # Production settings
└── test.toml          # Test environment
```

## 🔧 Backtesting Usage

### **Interactive Flow**
1. **Data Analysis**: View available symbols and data ranges
2. **Strategy Selection**: Choose from built-in strategies (SMA, RSI)
3. **Parameter Configuration**: Set initial capital, commission rates, data range
4. **Execution**: Real-time progress tracking and results
5. **Analysis**: Comprehensive performance metrics and trade analysis

### **Example Session**
```bash
$ cargo run backtest

🎯 TRADING CORE BACKTESTING SYSTEM
================================================
📊 Loading data statistics...

📈 Available Data:
  Total Records: 1,245,678
  Available Symbols: 15
  Earliest Data: 2024-01-01 00:00:00 UTC
  Latest Data: 2024-08-09 23:59:59 UTC

🎯 Available Strategies:
  1) Simple Moving Average - Trading strategy based on moving average crossover
  2) RSI Strategy - Trading strategy based on Relative Strength Index (RSI)

Select strategy (1-2): 1
✅ Selected Strategy: Simple Moving Average

📊 Symbol Selection:
  1) BTCUSDT (456,789 records)
  2) ETHUSDT (234,567 records)
  ...

Select symbol: 1
✅ Selected Symbol: BTCUSDT

Enter initial capital (default: $10000): $50000
Enter commission rate % (default: 0.1%): 0.1

🔍 Loading historical data: BTCUSDT latest 10000 records...
✅ Loaded 10000 data points

Starting backtest...
Strategy: Simple Moving Average
Initial capital: $50000
Progress: 100% (10000/10000) | Portfolio Value: $52,450 | P&L: $2,450

BACKTEST RESULTS SUMMARY
============================================================
Strategy: Simple Moving Average
Initial Capital: $50000
Final Value: $52450
Total P&L: $2450
Return: 4.90%
...
```