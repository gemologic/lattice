use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::State;
use axum::http::header::{AUTHORIZATION, RETRY_AFTER};
use axum::http::{HeaderMap, HeaderValue, Method, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::config::RateLimitConfig;
use crate::state::AppState;

const CLEANUP_INTERVAL: Duration = Duration::from_secs(300);
const STALE_BUCKET_AGE: Duration = Duration::from_secs(3600);
const SSE_CAP_RETRY_AFTER_SECS: u64 = 10;

#[derive(Clone, Debug)]
pub struct RateLimiter {
    inner: Arc<Mutex<RateLimiterInner>>,
    settings: RateLimitConfig,
}

impl RateLimiter {
    pub fn new(settings: RateLimitConfig) -> Self {
        Self {
            inner: Arc::new(Mutex::new(RateLimiterInner::default())),
            settings,
        }
    }

    pub fn check(&self, scope: RateScope, identity: &str) -> RateDecision {
        self.check_with_now(scope, identity, Instant::now())
    }

    fn check_with_now(&self, scope: RateScope, identity: &str, now: Instant) -> RateDecision {
        let settings = bucket_settings(&self.settings, scope);
        self.with_inner(|inner| {
            inner.cleanup_if_needed(now);

            let bucket = inner
                .buckets
                .entry((scope, identity.to_string()))
                .or_insert_with(|| RateBucket::new(settings.burst, now));

            bucket.refill(settings.per_minute, settings.burst, now);

            if bucket.tokens >= 1.0 {
                bucket.tokens -= 1.0;
                let remaining = bucket.tokens.floor().clamp(0.0, u32::MAX as f64) as u32;
                let reset_after_secs =
                    reset_after_seconds(bucket.tokens, settings.per_minute, settings.burst);

                RateDecision::Allow(RateAllowance {
                    limit: settings.per_minute,
                    remaining,
                    reset_after_secs,
                })
            } else {
                let retry_after_secs = retry_after_seconds(bucket.tokens, settings.per_minute);
                RateDecision::Deny(RateDenial {
                    limit: settings.per_minute,
                    remaining: 0,
                    reset_after_secs: retry_after_secs,
                    retry_after_secs,
                    message: format!("rate limit exceeded for {}", scope.description()),
                })
            }
        })
    }

    pub fn try_acquire_sse_slot(&self, identity: &str) -> Result<SseConnectionLease, SseCapDenied> {
        self.with_inner(|inner| {
            let current_for_identity = inner
                .sse_active_by_identity
                .get(identity)
                .copied()
                .unwrap_or(0);
            if current_for_identity >= self.settings.sse_max_per_identity {
                return Err(SseCapDenied {
                    limit: self.settings.sse_max_per_identity,
                    retry_after_secs: SSE_CAP_RETRY_AFTER_SECS,
                    message: "too many active SSE streams for this client identity".to_string(),
                });
            }

            if inner.sse_active_global >= self.settings.sse_max_global {
                return Err(SseCapDenied {
                    limit: self.settings.sse_max_global,
                    retry_after_secs: SSE_CAP_RETRY_AFTER_SECS,
                    message: "SSE stream capacity reached for this instance".to_string(),
                });
            }

            inner.sse_active_global += 1;
            inner
                .sse_active_by_identity
                .entry(identity.to_string())
                .and_modify(|count| *count += 1)
                .or_insert(1);

            Ok(SseConnectionLease {
                limiter: self.clone(),
                identity: identity.to_string(),
                released: false,
            })
        })
    }

    fn release_sse_slot(&self, identity: &str) {
        self.with_inner(|inner| {
            inner.sse_active_global = inner.sse_active_global.saturating_sub(1);
            match inner.sse_active_by_identity.get_mut(identity) {
                Some(count) if *count > 1 => {
                    *count -= 1;
                }
                Some(_) => {
                    inner.sse_active_by_identity.remove(identity);
                }
                None => {}
            }
        });
    }

    fn with_inner<F, T>(&self, f: F) -> T
    where
        F: FnOnce(&mut RateLimiterInner) -> T,
    {
        let mut guard = lock_or_recover(&self.inner);
        f(&mut guard)
    }
}

pub async fn enforce_limits(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let Some(scope) = classify_scope(request.method(), request.uri().path()) else {
        return next.run(request).await;
    };

    let identity = request_identity(request.headers(), state.config.auth_enabled());
    let decision = state.rate_limiter.check(scope, &identity);

    let allowance = match decision {
        RateDecision::Allow(allowance) => allowance,
        RateDecision::Deny(denial) => {
            tracing::warn!(
                scope = %scope.description(),
                identity = %identity,
                retry_after_secs = denial.retry_after_secs,
                "request denied by rate limiter"
            );
            return rate_limited_response(denial);
        }
    };

    let sse_lease = if scope == RateScope::Sse {
        match state.rate_limiter.try_acquire_sse_slot(&identity) {
            Ok(lease) => Some(lease),
            Err(denial) => {
                tracing::warn!(identity = %identity, "request denied by sse stream capacity");
                return sse_capacity_response(denial);
            }
        }
    } else {
        None
    };

    let mut response = next.run(request).await;
    set_rate_limit_headers(&mut response, &allowance);

    if response.status().is_success() {
        if let Some(lease) = sse_lease {
            response.extensions_mut().insert(lease);
        }
    }

    response
}

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub enum RateScope {
    Read,
    Write,
    Attachment,
    WebhookTest,
    Mcp,
    Sse,
}

impl RateScope {
    fn description(self) -> &'static str {
        match self {
            Self::Read => "read requests",
            Self::Write => "write requests",
            Self::Attachment => "attachment requests",
            Self::WebhookTest => "webhook test requests",
            Self::Mcp => "mcp requests",
            Self::Sse => "sse connect requests",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SseConnectionLease {
    limiter: RateLimiter,
    identity: String,
    released: bool,
}

impl Drop for SseConnectionLease {
    fn drop(&mut self) {
        if self.released {
            return;
        }
        self.released = true;
        self.limiter.release_sse_slot(&self.identity);
    }
}

#[derive(Debug)]
pub enum RateDecision {
    Allow(RateAllowance),
    Deny(RateDenial),
}

#[derive(Debug)]
pub struct RateAllowance {
    pub limit: u32,
    pub remaining: u32,
    pub reset_after_secs: u64,
}

#[derive(Debug)]
pub struct RateDenial {
    pub limit: u32,
    pub remaining: u32,
    pub reset_after_secs: u64,
    pub retry_after_secs: u64,
    pub message: String,
}

#[derive(Debug)]
pub struct SseCapDenied {
    pub limit: u32,
    pub retry_after_secs: u64,
    pub message: String,
}

#[derive(Clone, Copy, Debug)]
struct BucketSettings {
    per_minute: u32,
    burst: u32,
}

fn bucket_settings(settings: &RateLimitConfig, scope: RateScope) -> BucketSettings {
    match scope {
        RateScope::Read => BucketSettings {
            per_minute: settings.read_per_min,
            burst: settings.read_burst,
        },
        RateScope::Write => BucketSettings {
            per_minute: settings.write_per_min,
            burst: settings.write_burst,
        },
        RateScope::Attachment => BucketSettings {
            per_minute: settings.attachment_per_min,
            burst: settings.attachment_burst,
        },
        RateScope::WebhookTest => BucketSettings {
            per_minute: settings.webhook_test_per_min,
            burst: settings.webhook_test_burst,
        },
        RateScope::Mcp => BucketSettings {
            per_minute: settings.mcp_per_min,
            burst: settings.mcp_burst,
        },
        RateScope::Sse => BucketSettings {
            per_minute: settings.sse_connect_per_min,
            burst: settings.sse_connect_burst,
        },
    }
}

#[derive(Debug)]
struct RateBucket {
    tokens: f64,
    last_refill: Instant,
    last_seen: Instant,
}

impl RateBucket {
    fn new(burst: u32, now: Instant) -> Self {
        Self {
            tokens: burst as f64,
            last_refill: now,
            last_seen: now,
        }
    }

    fn refill(&mut self, per_minute: u32, burst: u32, now: Instant) {
        self.last_seen = now;
        if per_minute == 0 {
            return;
        }

        let elapsed = now
            .saturating_duration_since(self.last_refill)
            .as_secs_f64();
        if elapsed <= 0.0 {
            return;
        }

        let refill_rate = per_minute as f64 / 60.0;
        self.tokens = (self.tokens + elapsed * refill_rate).min(burst as f64);
        self.last_refill = now;
    }
}

#[derive(Debug, Default)]
struct RateLimiterInner {
    buckets: HashMap<(RateScope, String), RateBucket>,
    sse_active_by_identity: HashMap<String, u32>,
    sse_active_global: u32,
    last_cleanup: Option<Instant>,
}

impl RateLimiterInner {
    fn cleanup_if_needed(&mut self, now: Instant) {
        let should_cleanup = self
            .last_cleanup
            .is_none_or(|previous| now.saturating_duration_since(previous) >= CLEANUP_INTERVAL);

        if !should_cleanup {
            return;
        }

        self.buckets
            .retain(|_, bucket| now.saturating_duration_since(bucket.last_seen) < STALE_BUCKET_AGE);
        self.last_cleanup = Some(now);
    }
}

fn classify_scope(method: &Method, path: &str) -> Option<RateScope> {
    if path.starts_with("/mcp") {
        return Some(RateScope::Mcp);
    }

    if !path.starts_with("/api/v1") {
        return None;
    }

    if is_sse_route(path) {
        return Some(RateScope::Sse);
    }

    if path.starts_with("/api/v1/files/") || path.contains("/attachments") {
        return Some(RateScope::Attachment);
    }

    if path.contains("/webhooks/") && path.ends_with("/test") {
        return Some(RateScope::WebhookTest);
    }

    if method == Method::GET || method == Method::HEAD || method == Method::OPTIONS {
        return Some(RateScope::Read);
    }

    Some(RateScope::Write)
}

fn is_sse_route(path: &str) -> bool {
    let normalized = path.trim_end_matches('/');
    normalized == "/api/v1/events"
        || (normalized.starts_with("/api/v1/projects/") && normalized.ends_with("/events"))
}

fn request_identity(headers: &HeaderMap, auth_enabled: bool) -> String {
    if auth_enabled {
        if let Some(token) = headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(parse_bearer_token)
        {
            return format!("token:{}", hash_prefix(token));
        }

        return "token:missing".to_string();
    }

    if let Some(ip) = first_forwarded_ip(headers) {
        return format!("ip:{ip}");
    }

    "ip:anonymous".to_string()
}

fn parse_bearer_token(value: &str) -> Option<&str> {
    let mut parts = value.splitn(2, ' ');
    let scheme = parts.next()?;
    let token = parts.next()?.trim();

    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }

    if token.is_empty() {
        return None;
    }

    Some(token)
}

fn first_forwarded_ip(headers: &HeaderMap) -> Option<String> {
    extract_first_ip(headers, "x-forwarded-for").or_else(|| extract_first_ip(headers, "x-real-ip"))
}

fn extract_first_ip(headers: &HeaderMap, header_name: &'static str) -> Option<String> {
    let value = headers.get(header_name)?.to_str().ok()?;
    let first = value
        .split(',')
        .next()
        .map(str::trim)
        .filter(|ip| !ip.is_empty())?;
    Some(first.to_string())
}

fn hash_prefix(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let digest = hasher.finalize();

    let mut output = String::with_capacity(24);
    for byte in digest.iter().take(12) {
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            tracing::warn!("rate limiter mutex poisoned, recovering");
            poisoned.into_inner()
        }
    }
}

fn retry_after_seconds(tokens: f64, per_minute: u32) -> u64 {
    if per_minute == 0 {
        return 60;
    }

    let refill_rate = per_minute as f64 / 60.0;
    let missing = (1.0 - tokens).max(0.0);
    (missing / refill_rate).ceil().max(1.0) as u64
}

fn reset_after_seconds(tokens: f64, per_minute: u32, burst: u32) -> u64 {
    if per_minute == 0 {
        return 60;
    }

    let refill_rate = per_minute as f64 / 60.0;
    let missing = (burst as f64 - tokens).max(0.0);
    if missing <= 0.0 {
        return 0;
    }

    (missing / refill_rate).ceil() as u64
}

#[derive(Debug, Serialize)]
struct RateLimitBody {
    error: &'static str,
    message: String,
}

fn rate_limited_response(denial: RateDenial) -> Response {
    let body = Json(RateLimitBody {
        error: "rate_limited",
        message: denial.message,
    });
    let mut response = (StatusCode::TOO_MANY_REQUESTS, body).into_response();

    set_rate_limit_headers(
        &mut response,
        &RateAllowance {
            limit: denial.limit,
            remaining: denial.remaining,
            reset_after_secs: denial.reset_after_secs,
        },
    );
    set_header_u64(&mut response, RETRY_AFTER.as_str(), denial.retry_after_secs);

    response
}

fn sse_capacity_response(denial: SseCapDenied) -> Response {
    let body = Json(RateLimitBody {
        error: "rate_limited",
        message: denial.message,
    });
    let mut response = (StatusCode::TOO_MANY_REQUESTS, body).into_response();
    set_header_u64(&mut response, "x-ratelimit-limit", denial.limit as u64);
    set_header_u64(&mut response, "x-ratelimit-remaining", 0);
    set_header_u64(&mut response, "x-ratelimit-reset", denial.retry_after_secs);
    set_header_u64(&mut response, RETRY_AFTER.as_str(), denial.retry_after_secs);
    response
}

fn set_rate_limit_headers(response: &mut Response, allowance: &RateAllowance) {
    set_header_u64(response, "x-ratelimit-limit", allowance.limit as u64);
    set_header_u64(
        response,
        "x-ratelimit-remaining",
        allowance.remaining as u64,
    );
    set_header_u64(response, "x-ratelimit-reset", allowance.reset_after_secs);
}

fn set_header_u64(response: &mut Response, key: &'static str, value: u64) {
    if let Ok(header_value) = HeaderValue::from_str(&value.to_string()) {
        response.headers_mut().insert(key, header_value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_scope_uses_expected_buckets() {
        assert_eq!(
            classify_scope(&Method::GET, "/api/v1/projects/ROADMAP/tasks"),
            Some(RateScope::Read)
        );
        assert_eq!(
            classify_scope(&Method::POST, "/api/v1/projects/ROADMAP/tasks"),
            Some(RateScope::Write)
        );
        assert_eq!(
            classify_scope(&Method::GET, "/api/v1/files/123"),
            Some(RateScope::Attachment)
        );
        assert_eq!(
            classify_scope(&Method::POST, "/api/v1/projects/ROADMAP/webhooks/a/test"),
            Some(RateScope::WebhookTest)
        );
        assert_eq!(
            classify_scope(&Method::GET, "/api/v1/projects/ROADMAP/events"),
            Some(RateScope::Sse)
        );
        assert_eq!(classify_scope(&Method::POST, "/mcp"), Some(RateScope::Mcp));
        assert_eq!(classify_scope(&Method::GET, "/"), None);
    }

    #[test]
    fn token_bucket_denies_after_burst_and_recovers() {
        let write_burst = 10;
        let limiter = RateLimiter::new(RateLimitConfig {
            write_per_min: 30,
            write_burst,
            ..RateLimitConfig::default()
        });
        let start = Instant::now();

        for _ in 0..write_burst {
            assert!(matches!(
                limiter.check_with_now(RateScope::Write, "token:a", start),
                RateDecision::Allow(_)
            ));
        }

        assert!(matches!(
            limiter.check_with_now(RateScope::Write, "token:a", start),
            RateDecision::Deny(_)
        ));

        let later = start + Duration::from_secs(2);
        assert!(matches!(
            limiter.check_with_now(RateScope::Write, "token:a", later),
            RateDecision::Allow(_)
        ));
    }

    #[test]
    fn sse_connection_slots_release_on_drop() {
        let sse_max_per_identity = 5;
        let limiter = RateLimiter::new(RateLimitConfig {
            sse_max_per_identity,
            ..RateLimitConfig::default()
        });
        let mut leases = Vec::new();

        for _ in 0..sse_max_per_identity {
            leases.push(
                limiter
                    .try_acquire_sse_slot("token:a")
                    .expect("sse slot should be available"),
            );
        }

        assert!(
            limiter.try_acquire_sse_slot("token:a").is_err(),
            "sixth slot should be denied"
        );

        drop(leases.pop());

        assert!(
            limiter.try_acquire_sse_slot("token:a").is_ok(),
            "slot should become available after drop"
        );
    }
}
