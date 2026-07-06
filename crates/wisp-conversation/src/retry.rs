use std::future::Future;
use std::time::Duration;

pub async fn retry_with_backoff<T, E, F, Fut>(
    mut operation: F,
    attempts: u32,
    delay_ms: u64,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut last_err: Option<E> = None;
    let total_calls = attempts + 1;

    for _ in 0..total_calls {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(err) => {
                last_err = Some(err);
                if delay_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
            },
        }
    }

    Err(last_err.expect("at least one call was made"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn retry_returns_success_on_first_try() {
        let call_count = Arc::new(AtomicU32::new(0));
        let cc = call_count.clone();

        let result: Result<i32, String> = retry_with_backoff(
            || {
                let cc = cc.clone();
                async move {
                    cc.fetch_add(1, Ordering::SeqCst);
                    Ok(42)
                }
            },
            2,
            1,
        )
        .await;

        assert_eq!(result, Ok(42));
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retry_succeeds_on_final_attempt() {
        let call_count = Arc::new(AtomicU32::new(0));
        let cc = call_count.clone();

        let result: Result<i32, String> = retry_with_backoff(
            || {
                let cc = cc.clone();
                async move {
                    let n = cc.fetch_add(1, Ordering::SeqCst);
                    if n < 2 {
                        Err("fail".to_string())
                    } else {
                        Ok(99)
                    }
                }
            },
            2,
            1,
        )
        .await;

        assert_eq!(result, Ok(99));
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_exhausted_returns_last_error() {
        let call_count = Arc::new(AtomicU32::new(0));
        let cc = call_count.clone();

        let result: Result<i32, String> = retry_with_backoff(
            || {
                let cc = cc.clone();
                async move {
                    cc.fetch_add(1, Ordering::SeqCst);
                    Err("always fails".to_string())
                }
            },
            2,
            1,
        )
        .await;

        assert_eq!(result, Err("always fails".to_string()));
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_zero_attempts_calls_once() {
        let call_count = Arc::new(AtomicU32::new(0));
        let cc = call_count.clone();

        let _result: Result<i32, String> = retry_with_backoff(
            || {
                let cc = cc.clone();
                async move {
                    cc.fetch_add(1, Ordering::SeqCst);
                    Err("nope".to_string())
                }
            },
            0,
            1,
        )
        .await;

        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }
}
