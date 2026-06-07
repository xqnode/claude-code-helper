//! 上游瞬时故障的有限重试（429 / 502 / 503 / 504 与连接错误）。

use std::time::Duration;

pub const MAX_UPSTREAM_ATTEMPTS: usize = 3;
const UPSTREAM_RETRY_BASE_MS: u64 = 500;
const MAX_RETRY_AFTER_SECS: u64 = 30;

pub fn is_retryable_upstream_status(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 429 | 502 | 503 | 504)
}

pub fn is_retryable_upstream_error(err: &reqwest::Error) -> bool {
    err.is_timeout() || err.is_connect() || err.is_request()
}

pub fn retry_backoff(attempt: usize) -> Duration {
    let multiplier = 1u64.checked_shl(attempt as u32).unwrap_or(8);
    Duration::from_millis(UPSTREAM_RETRY_BASE_MS * multiplier)
}

pub fn retry_delay_from_headers(headers: &reqwest::header::HeaderMap, attempt: usize) -> Duration {
    let Some(value) = headers.get("retry-after").and_then(|v| v.to_str().ok()) else {
        return retry_backoff(attempt);
    };
    let value = value.trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Duration::from_secs(seconds.min(MAX_RETRY_AFTER_SECS));
    }
    retry_backoff(attempt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};

    #[test]
    fn retryable_statuses_include_rate_limit_and_gateway_errors() {
        assert!(is_retryable_upstream_status(
            reqwest::StatusCode::TOO_MANY_REQUESTS
        ));
        assert!(is_retryable_upstream_status(reqwest::StatusCode::BAD_GATEWAY));
        assert!(!is_retryable_upstream_status(reqwest::StatusCode::BAD_REQUEST));
    }

    #[test]
    fn retry_after_header_caps_delay() {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("120"));
        let delay = retry_delay_from_headers(&headers, 0);
        assert_eq!(delay, Duration::from_secs(30));
    }

    #[test]
    fn retry_backoff_grows_exponentially() {
        assert_eq!(retry_backoff(0), Duration::from_millis(500));
        assert_eq!(retry_backoff(1), Duration::from_millis(1000));
        assert_eq!(retry_backoff(2), Duration::from_millis(2000));
    }
}
