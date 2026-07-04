// utils/retry.rs
// 重试机制和错误处理工具

use std::time::Duration;
use tokio::time::sleep;
use tracing::{warn, info};

/// 重试配置
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// 最大重试次数
    pub max_retries: u32,
    /// 初始延迟
    pub initial_delay: Duration,
    /// 最大延迟
    pub max_delay: Duration,
    /// 退避倍数
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
        }
    }
}

impl RetryConfig {
    /// 创建快速重试配置（用于轻量操作）
    pub fn fast() -> Self {
        Self {
            max_retries: 2,
            initial_delay: Duration::from_millis(50),
            max_delay: Duration::from_secs(1),
            backoff_multiplier: 2.0,
        }
    }

    /// 创建标准重试配置（用于 API 调用）
    pub fn standard() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 2.0,
        }
    }

    /// 创建持久重试配置（用于关键操作）
    pub fn persistent() -> Self {
        Self {
            max_retries: 5,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
        }
    }
}

/// 重试错误类型
#[derive(Debug)]
pub enum RetryError<E> {
    /// 最终失败（最后一次重试也失败）
    Exhausted { attempts: u32, last_error: E },
    /// 不可重试的错误
    NonRetryable(E),
}

/// 判断错误是否可重试
pub trait Retryable {
    fn is_retryable(&self) -> bool;
}

/// 执行带重试的异步操作
pub async fn with_retry<T, E, F, Fut>(
    config: &RetryConfig,
    operation_name: &str,
    mut operation: F,
) -> Result<T, RetryError<E>>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display + Retryable,
{
    let mut delay = config.initial_delay;
    let mut last_error = None;

    for attempt in 0..=config.max_retries {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    info!(
                        "{} succeeded after {} retries",
                        operation_name, attempt
                    );
                }
                return Ok(result);
            }
            Err(error) => {
                if !error.is_retryable() {
                    warn!(
                        "{} failed with non-retryable error: {}",
                        operation_name, error
                    );
                    return Err(RetryError::NonRetryable(error));
                }

                if attempt < config.max_retries {
                    warn!(
                        "{} failed (attempt {}/{}): {}. Retrying in {:?}...",
                        operation_name,
                        attempt + 1,
                        config.max_retries,
                        error,
                        delay
                    );
                    sleep(delay).await;

                    // 计算下一次延迟（指数退避）
                    delay = Duration::from_secs_f64(
                        (delay.as_secs_f64() * config.backoff_multiplier)
                            .min(config.max_delay.as_secs_f64())
                    );
                }

                last_error = Some(error);
            }
        }
    }

    Err(RetryError::Exhausted {
        attempts: config.max_retries + 1,
        last_error: last_error.unwrap(),
    })
}

/// 带超时的异步操作
pub async fn with_timeout<T, E>(
    timeout: Duration,
    operation_name: &str,
    operation: impl std::future::Future<Output = Result<T, E>>,
) -> Result<T, TimeoutError<E>>
where
    E: std::fmt::Display,
{
    match tokio::time::timeout(timeout, operation).await {
        Ok(result) => result.map_err(TimeoutError::Operation),
        Err(_) => Err(TimeoutError::Timeout {
            duration: timeout,
            operation: operation_name.to_string(),
        }),
    }
}

/// 超时错误
#[derive(Debug)]
pub enum TimeoutError<E> {
    Timeout {
        duration: Duration,
        operation: String,
    },
    Operation(E),
}

impl<E: std::fmt::Display> std::fmt::Display for TimeoutError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TimeoutError::Timeout { duration, operation } => {
                write!(f, "Operation '{}' timed out after {:?}", operation, duration)
            }
            TimeoutError::Operation(e) => write!(f, "{}", e),
        }
    }
}

/// 错误上下文包装
pub trait ErrorContext<T> {
    fn with_context(self, context: impl FnOnce() -> String) -> Result<T, ContextError>;
}

#[derive(Debug)]
pub struct ContextError {
    pub context: String,
    pub source: Box<dyn std::error::Error + Send + Sync>,
}

impl std::fmt::Display for ContextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.context, self.source)
    }
}

impl<T, E> ErrorContext<T> for Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn with_context(self, context: impl FnOnce() -> String) -> Result<T, ContextError> {
        self.map_err(|e| ContextError {
            context: context(),
            source: Box::new(e),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[derive(Debug)]
    struct TestError {
        message: String,
        retryable: bool,
    }

    impl std::fmt::Display for TestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.message)
        }
    }

    impl Retryable for TestError {
        fn is_retryable(&self) -> bool {
            self.retryable
        }
    }

    #[tokio::test]
    async fn test_retry_success_on_first_attempt() {
        let config = RetryConfig::fast();
        let result = with_retry(&config, "test", || async {
            Ok::<_, TestError>("success")
        }).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
    }

    #[tokio::test]
    async fn test_retry_success_after_failures() {
        let config = RetryConfig::fast();
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result = with_retry(&config, "test", move || {
            let attempts = attempts_clone.clone();
            async move {
                let count = attempts.fetch_add(1, Ordering::SeqCst);
                if count < 2 {
                    Err(TestError {
                        message: "not yet".to_string(),
                        retryable: true,
                    })
                } else {
                    Ok("success")
                }
            }
        }).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_exhausted() {
        let config = RetryConfig {
            max_retries: 2,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            backoff_multiplier: 1.0,
        };

        let result = with_retry(&config, "test", || async {
            Err::<(), TestError>(TestError {
                message: "always fail".to_string(),
                retryable: true,
            })
        }).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            RetryError::Exhausted { attempts, .. } => {
                assert_eq!(attempts, 3);
            }
            _ => panic!("Expected Exhausted error"),
        }
    }

    #[tokio::test]
    async fn test_retry_non_retryable_error() {
        let config = RetryConfig::fast();
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result = with_retry(&config, "test", move || {
            attempts_clone.fetch_add(1, Ordering::SeqCst);
            async {
                Err::<(), TestError>(TestError {
                    message: "fatal".to_string(),
                    retryable: false,
                })
            }
        }).await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 1); // 只尝试了一次
    }

    #[tokio::test]
    async fn test_timeout_success() {
        async fn operation() -> Result<String, String> {
            Ok("success".to_string())
        }

        let result = with_timeout(
            Duration::from_secs(1),
            "test",
            operation(),
        ).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_timeout_exceeded() {
        async fn operation() -> Result<String, String> {
            tokio::time::sleep(Duration::from_secs(1)).await;
            Ok("success".to_string())
        }

        let result = with_timeout(
            Duration::from_millis(50),
            "test",
            operation(),
        ).await;

        assert!(result.is_err());
    }
}
