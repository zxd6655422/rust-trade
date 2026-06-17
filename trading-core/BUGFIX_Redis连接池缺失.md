# Redis 连接池缺失导致数据写入失败

## 问题描述

```
Redis LPUSH failed: 由于以前的关闭调用，套接字在那个方向已经关闭，发送或接收数据的请求没有被接受。 (os error 10058)
```

## 影响

- 长时间运行后，Redis 缓存写入失败
- 数据无法写入缓存层

## 原因分析

**错误码 10058**：Windows 的 WSAENOTCONN 错误，表示套接字未连接

**根本原因**：
1. 代码使用单个 Redis `Connection` 对象，存储在 `Arc<Mutex<Connection>>` 中
2. 没有连接池和重试机制
3. 长时间运行后，Redis 空闲连接超时断开

## 代码位置

`trading-common/src/data/cache.rs` - `RedisTickCache` 结构体

## 修复方案

### 修改前

```rust
pub struct RedisTickCache {
    #[allow(dead_code)]
    client: RedisClient,
    connection: Arc<Mutex<Connection>>,  // 单个连接，无重试
    max_ticks_per_symbol: usize,
    ttl_seconds: u64,
}
```

### 修改后

```rust
pub struct RedisTickCache {
    client: RedisClient,
    max_ticks_per_symbol: usize,
    ttl_seconds: u64,
    max_retries: u32,           // 重试次数
    retry_delay: Duration,      // 重试延迟
}

impl RedisTickCache {
    /// 获取连接（带重试）
    fn get_connection_with_retry(&self) -> DataResult<redis::Connection> {
        // 最多重试 3 次，每次间隔 100ms
    }

    /// 执行 Redis 命令（带自动重连）
    fn execute_with_retry<F, T>(&self, mut f: F) -> DataResult<T>
    where
        F: FnMut(&mut redis::Connection) -> redis::RedisResult<T>,
    {
        // 自动识别连接错误并重试
    }
}
```

### 关键改进

1. **每次操作创建新连接**：避免使用断开的连接
2. **连接错误检测**：自动识别错误码 10058、connection refused、broken pipe
3. **重试机制**：最多重试 3 次，每次间隔 100ms
4. **启动时 PING 测试**：验证连接正常

## 验证

```bash
cd F:/rust_projects/rust-trade/trading-common
cargo check
```

## 修复日期

2026-06-17
