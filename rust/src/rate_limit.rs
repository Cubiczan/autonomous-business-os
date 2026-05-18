//! # Rate Limit & Circuit Breaker Module
//!
//! Token-bucket rate limiter, circuit breaker (closed/open/half-open),
//! and a combined `RequestGuard` that checks both before allowing a request.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ===========================================================================
// Token Bucket Rate Limiter
// ===========================================================================

/// Configuration for a single token bucket.
#[derive(Debug, Clone)]
pub struct BucketConfig {
    /// Maximum number of tokens the bucket can hold (capacity).
    pub max_tokens: u32,
    /// Tokens added per refill interval.
    pub refill_rate: u32,
    /// Milliseconds between refill ticks.
    pub refill_interval_ms: u64,
}

impl Default for BucketConfig {
    fn default() -> Self {
        Self {
            max_tokens: 100,
            refill_rate: 10,
            refill_interval_ms: 100,
        }
    }
}

/// Runtime state for a single bucket identified by a string key.
#[derive(Debug, Clone)]
pub struct BucketState {
    /// Current token count (may be fractional during refill).
    pub tokens: f64,
    /// Timestamp of the last refill calculation.
    pub last_refill: DateTime<Utc>,
}

/// Errors produced by the rate limiter.
#[derive(Debug, Error, PartialEq)]
pub enum RateLimitError {
    #[error("rate limit exceeded for key: {key} (retry after {retry_after_ms}ms)")]
    Exceeded { key: String, retry_after_ms: u64 },
}

/// Multi-key token-bucket rate limiter.
///
/// Each unique string key gets its own independent bucket that refills
/// over time according to `BucketConfig`.
pub struct RateLimiter {
    buckets: HashMap<String, BucketState>,
    config: BucketConfig,
}

impl RateLimiter {
    /// Create a new rate limiter with the given configuration.
    pub fn new(config: BucketConfig) -> Self {
        Self {
            buckets: HashMap::new(),
            config,
        }
    }

    /// Ensure a bucket exists for `key`, initialising it with `max_tokens`.
    fn ensure_bucket(&mut self, key: &str) {
        if !self.buckets.contains_key(key) {
            self.buckets.insert(
                key.to_string(),
                BucketState {
                    tokens: self.config.max_tokens as f64,
                    last_refill: Utc::now(),
                },
            );
        }
    }

    /// Refill tokens for `key` based on elapsed time since last refill.
    fn _refill(&mut self, key: &str) {
        if let Some(state) = self.buckets.get_mut(key) {
            let now = Utc::now();
            let elapsed_ms = (now - state.last_refill).num_milliseconds().max(0) as u64;
            if elapsed_ms >= self.config.refill_interval_ms {
                let intervals = elapsed_ms / self.config.refill_interval_ms;
                let tokens_to_add = intervals as f64 * self.config.refill_rate as f64;
                state.tokens = (state.tokens + tokens_to_add).min(self.config.max_tokens as f64);
                // Advance last_refill by the consumed intervals to avoid drift
                state.last_refill = state.last_refill
                    + Duration::milliseconds((intervals * self.config.refill_interval_ms) as i64);
            }
        }
    }

    /// Try to consume **one** token for `key`.
    ///
    /// Returns `Ok(())` if a token was available, or `Err(RateLimitError::Exceeded)`
    /// with an estimated retry-after in milliseconds.
    pub fn check(&mut self, key: &str) -> Result<(), RateLimitError> {
        self.check_n(key, 1)
    }

    /// Try to consume `tokens` tokens for `key`.
    ///
    /// Returns `Ok(())` if enough tokens were available.
    pub fn check_n(&mut self, key: &str, tokens: u32) -> Result<(), RateLimitError> {
        self.ensure_bucket(key);
        self._refill(key);

        let state = self.buckets.get(key).unwrap();
        let available = state.tokens.floor() as u32;

        if available >= tokens {
            // Consume tokens
            let state = self.buckets.get_mut(key).unwrap();
            state.tokens -= tokens as f64;
            Ok(())
        } else {
            let deficit = tokens as f64 - state.tokens;
            let retry_after_ms = if self.config.refill_rate == 0 {
                u64::MAX // No refills ever — infinite wait
            } else {
                let intervals_needed = (deficit / self.config.refill_rate as f64).ceil() as u64;
                intervals_needed.saturating_mul(self.config.refill_interval_ms)
            };
            Err(RateLimitError::Exceeded {
                key: key.to_string(),
                retry_after_ms,
            })
        }
    }

    /// Return the number of currently available tokens for `key`.
    ///
    /// The count is refilled before reporting.
    pub fn available(&mut self, key: &str) -> u32 {
        self.ensure_bucket(key);
        self._refill(key);
        let state = self.buckets.get(key).unwrap();
        state.tokens.floor() as u32
    }

    /// Reset the bucket for `key` back to full capacity.
    pub fn reset(&mut self, key: &str) {
        self.buckets.insert(
            key.to_string(),
            BucketState {
                tokens: self.config.max_tokens as f64,
                last_refill: Utc::now(),
            },
        );
    }

    /// Return the number of tracked buckets.
    pub fn bucket_count(&self) -> usize {
        self.buckets.len()
    }
}

// ===========================================================================
// Circuit Breaker
// ===========================================================================

/// The three states of a circuit breaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitState {
    /// Normal operation – requests flow through.
    Closed,
    /// Failing – all requests are rejected.
    Open,
    /// Testing whether the downstream has recovered.
    HalfOpen,
}

/// Configuration for a circuit breaker instance.
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Consecutive failures before the circuit opens. Default: 5.
    pub failure_threshold: u32,
    /// Consecutive successes in half-open before closing. Default: 3.
    pub success_threshold: u32,
    /// Seconds to remain open before transitioning to half-open. Default: 60.
    pub timeout_secs: i64,
    /// Maximum test calls allowed while in half-open. Default: 3.
    pub half_open_max_calls: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            timeout_secs: 60,
            half_open_max_calls: 3,
        }
    }
}

/// Errors produced by the circuit breaker.
#[derive(Debug, Error, PartialEq)]
pub enum CircuitBreakerError {
    #[error("circuit breaker '{name}' is open (opened at {opened_at:?})")]
    Open {
        name: String,
        opened_at: Option<DateTime<Utc>>,
    },
    #[error("circuit breaker '{name}' is half-open with no capacity")]
    HalfOpenExhausted { name: String },
}

/// A single circuit breaker that tracks failures/successes for a named
/// downstream dependency.
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    /// Logical name of the circuit (e.g. "email-service").
    pub name: String,
    pub config: CircuitBreakerConfig,
    pub state: CircuitState,
    pub failure_count: u32,
    pub success_count: u32,
    pub last_failure: Option<DateTime<Utc>>,
    pub opened_at: Option<DateTime<Utc>>,
    pub half_open_calls: u32,
}

impl CircuitBreaker {
    /// Create a new circuit breaker.
    pub fn new(name: &str, config: CircuitBreakerConfig) -> Self {
        Self {
            name: name.to_string(),
            config,
            state: CircuitState::Closed,
            failure_count: 0,
            success_count: 0,
            last_failure: None,
            opened_at: None,
            half_open_calls: 0,
        }
    }

    /// Check whether the Open → HalfOpen timeout has elapsed and transition
    /// automatically if so.
    fn check_timeout(&mut self) {
        if self.state == CircuitState::Open {
            if let Some(opened_at) = self.opened_at {
                let elapsed = Utc::now() - opened_at;
                if elapsed >= Duration::seconds(self.config.timeout_secs) {
                    self.state = CircuitState::HalfOpen;
                    self.half_open_calls = 0;
                    self.success_count = 0;
                }
            }
        }
    }

    /// Decide whether to allow the next request through.
    ///
    /// * Closed → allow
    /// * Open → reject (or transition to HalfOpen if timeout elapsed)
    /// * HalfOpen → allow up to `half_open_max_calls` test calls
    pub fn allow_request(&mut self) -> Result<(), CircuitBreakerError> {
        self.check_timeout();

        match self.state {
            CircuitState::Closed => Ok(()),
            CircuitState::Open => Err(CircuitBreakerError::Open {
                name: self.name.clone(),
                opened_at: self.opened_at,
            }),
            CircuitState::HalfOpen => {
                if self.half_open_calls < self.config.half_open_max_calls {
                    self.half_open_calls += 1;
                    Ok(())
                } else {
                    Err(CircuitBreakerError::HalfOpenExhausted {
                        name: self.name.clone(),
                    })
                }
            }
        }
    }

    /// Record a successful call.
    ///
    /// * Closed → reset failure count
    /// * HalfOpen → increment success count; close if threshold reached
    pub fn record_success(&mut self) {
        match self.state {
            CircuitState::Closed => {
                self.failure_count = 0;
            }
            CircuitState::HalfOpen => {
                self.success_count += 1;
                if self.success_count >= self.config.success_threshold {
                    self.state = CircuitState::Closed;
                    self.failure_count = 0;
                    self.success_count = 0;
                    self.half_open_calls = 0;
                }
            }
            CircuitState::Open => {
                // Shouldn't happen, but handle gracefully
            }
        }
    }

    /// Record a failed call.
    ///
    /// * Closed → increment failure count; open if threshold reached
    /// * HalfOpen → immediately re-open the circuit
    pub fn record_failure(&mut self) {
        self.last_failure = Some(Utc::now());

        match self.state {
            CircuitState::Closed => {
                self.failure_count += 1;
                if self.failure_count >= self.config.failure_threshold {
                    self.state = CircuitState::Open;
                    self.opened_at = Some(Utc::now());
                }
            }
            CircuitState::HalfOpen => {
                self.state = CircuitState::Open;
                self.opened_at = Some(Utc::now());
                self.success_count = 0;
                self.half_open_calls = 0;
            }
            CircuitState::Open => {
                // Already open, just update last_failure
            }
        }
    }

    /// Current state of the circuit.
    pub fn state(&self) -> CircuitState {
        self.state
    }

    /// Whether the circuit is currently open (rejecting all).
    pub fn is_open(&self) -> bool {
        self.state == CircuitState::Open
    }

    /// Force-reset the circuit back to Closed with zeroed counters.
    pub fn reset(&mut self) {
        self.state = CircuitState::Closed;
        self.failure_count = 0;
        self.success_count = 0;
        self.last_failure = None;
        self.opened_at = None;
        self.half_open_calls = 0;
    }
}

// ===========================================================================
// Combined Request Guard
// ===========================================================================

/// Combines a rate limiter and a set of named circuit breakers into a
/// single guard that callers check before dispatching work.
pub struct RequestGuard {
    pub rate_limiter: RateLimiter,
    pub circuit_breakers: HashMap<String, CircuitBreaker>,
}

impl RequestGuard {
    /// Create a new guard with the given rate-limit configuration.
    pub fn new(rate_config: BucketConfig) -> Self {
        Self {
            rate_limiter: RateLimiter::new(rate_config),
            circuit_breakers: HashMap::new(),
        }
    }

    /// Check rate limit (always) and optionally a named circuit breaker.
    ///
    /// Returns `Ok(())` only when **both** checks pass.
    pub fn check(&mut self, key: &str, circuit_name: Option<&str>) -> Result<(), String> {
        // Rate limit first
        self.rate_limiter.check(key).map_err(|e| e.to_string())?;

        // Circuit breaker second
        if let Some(name) = circuit_name {
            if let Some(cb) = self.circuit_breakers.get_mut(name) {
                cb.allow_request().map_err(|e| e.to_string())?;
            }
        }

        Ok(())
    }

    /// Register a new named circuit breaker.
    pub fn register_circuit(&mut self, name: &str, config: CircuitBreakerConfig) {
        self.circuit_breakers
            .insert(name.to_string(), CircuitBreaker::new(name, config));
    }

    /// Record a success on the named circuit breaker.
    pub fn record_success(&mut self, circuit_name: &str) {
        if let Some(cb) = self.circuit_breakers.get_mut(circuit_name) {
            cb.record_success();
        }
    }

    /// Record a failure on the named circuit breaker.
    pub fn record_failure(&mut self, circuit_name: &str) {
        if let Some(cb) = self.circuit_breakers.get_mut(circuit_name) {
            cb.record_failure();
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Token Bucket: basic consumption
    // -----------------------------------------------------------------------

    #[test]
    fn check_single_token_ok() {
        let mut rl = RateLimiter::new(BucketConfig::default());
        assert!(rl.check("user-1").is_ok());
    }

    #[test]
    fn check_single_token_decrements() {
        let config = BucketConfig {
            max_tokens: 5,
            refill_rate: 1,
            refill_interval_ms: 1000,
        };
        let mut rl = RateLimiter::new(config);
        for _ in 0..5 {
            assert!(rl.check("k").is_ok());
        }
        // 6th should fail
        assert!(rl.check("k").is_err());
    }

    // -----------------------------------------------------------------------
    // Token Bucket: exhaustion
    // -----------------------------------------------------------------------

    #[test]
    fn exhaust_bucket_returns_error() {
        let config = BucketConfig {
            max_tokens: 2,
            refill_rate: 1,
            refill_interval_ms: 100_000, // effectively no refill in tests
        };
        let mut rl = RateLimiter::new(config);
        assert!(rl.check("k").is_ok());
        assert!(rl.check("k").is_ok());
        let err = rl.check("k").unwrap_err();
        assert_eq!(err.key(), "k");
    }

    #[test]
    fn error_contains_retry_after() {
        let config = BucketConfig {
            max_tokens: 1,
            refill_rate: 1,
            refill_interval_ms: 500,
        };
        let mut rl = RateLimiter::new(config);
        rl.check("k").unwrap();
        let err = rl.check("k").unwrap_err();
        assert!(err.retry_after_ms() > 0);
    }

    // -----------------------------------------------------------------------
    // Token Bucket: available count
    // -----------------------------------------------------------------------

    #[test]
    fn available_returns_full_capacity() {
        let config = BucketConfig {
            max_tokens: 50,
            refill_rate: 10,
            refill_interval_ms: 100,
        };
        let mut rl = RateLimiter::new(config);
        assert_eq!(rl.available("new-key"), 50);
    }

    #[test]
    fn available_decreases_after_consumption() {
        let config = BucketConfig {
            max_tokens: 10,
            refill_rate: 1,
            refill_interval_ms: 100_000,
        };
        let mut rl = RateLimiter::new(config);
        rl.check("k").unwrap();
        rl.check("k").unwrap();
        assert_eq!(rl.available("k"), 8);
    }

    #[test]
    fn available_at_zero_when_exhausted() {
        let config = BucketConfig {
            max_tokens: 3,
            refill_rate: 1,
            refill_interval_ms: 100_000,
        };
        let mut rl = RateLimiter::new(config);
        for _ in 0..3 {
            rl.check("k").unwrap();
        }
        assert_eq!(rl.available("k"), 0);
    }

    // -----------------------------------------------------------------------
    // Token Bucket: multi-token
    // -----------------------------------------------------------------------

    #[test]
    fn check_n_ok() {
        let config = BucketConfig {
            max_tokens: 10,
            refill_rate: 1,
            refill_interval_ms: 100_000,
        };
        let mut rl = RateLimiter::new(config);
        assert!(rl.check_n("k", 5).is_ok());
        assert_eq!(rl.available("k"), 5);
    }

    #[test]
    fn check_n_exhausts() {
        let config = BucketConfig {
            max_tokens: 5,
            refill_rate: 1,
            refill_interval_ms: 100_000,
        };
        let mut rl = RateLimiter::new(config);
        assert!(rl.check_n("k", 3).is_ok());
        assert!(rl.check_n("k", 3).is_err());
        assert_eq!(rl.available("k"), 2);
    }

    #[test]
    fn check_n_zero_tokens_always_ok() {
        let config = BucketConfig {
            max_tokens: 0,
            refill_rate: 1,
            refill_interval_ms: 100_000,
        };
        let mut rl = RateLimiter::new(config);
        assert!(rl.check_n("k", 0).is_ok());
    }

    #[test]
    fn check_n_greater_than_max_fails() {
        let config = BucketConfig {
            max_tokens: 3,
            refill_rate: 1,
            refill_interval_ms: 100_000,
        };
        let mut rl = RateLimiter::new(config);
        assert!(rl.check_n("k", 10).is_err());
    }

    // -----------------------------------------------------------------------
    // Token Bucket: reset
    // -----------------------------------------------------------------------

    #[test]
    fn reset_restores_capacity() {
        let config = BucketConfig {
            max_tokens: 5,
            refill_rate: 1,
            refill_interval_ms: 100_000,
        };
        let mut rl = RateLimiter::new(config);
        for _ in 0..5 {
            rl.check("k").unwrap();
        }
        assert!(rl.check("k").is_err());
        rl.reset("k");
        assert_eq!(rl.available("k"), 5);
    }

    // -----------------------------------------------------------------------
    // Token Bucket: multiple keys are independent
    // -----------------------------------------------------------------------

    #[test]
    fn independent_keys() {
        let config = BucketConfig {
            max_tokens: 2,
            refill_rate: 1,
            refill_interval_ms: 100_000,
        };
        let mut rl = RateLimiter::new(config);
        for _ in 0..2 {
            rl.check("a").unwrap();
        }
        assert!(rl.check("a").is_err());
        assert!(rl.check("b").is_ok()); // b is fresh
    }

    // -----------------------------------------------------------------------
    // Token Bucket: edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn zero_max_tokens_immediate_reject() {
        let config = BucketConfig {
            max_tokens: 0,
            refill_rate: 0,
            refill_interval_ms: 100,
        };
        let mut rl = RateLimiter::new(config);
        assert!(rl.check("k").is_err());
    }

    #[test]
    fn max_tokens_boundary() {
        let config = BucketConfig {
            max_tokens: 1,
            refill_rate: 1,
            refill_interval_ms: 100_000,
        };
        let mut rl = RateLimiter::new(config);
        assert!(rl.check("k").is_ok());
        assert!(rl.check("k").is_err());
    }

    #[test]
    fn tokens_capped_at_max_after_refill() {
        let config = BucketConfig {
            max_tokens: 5,
            refill_rate: 100, // very high refill
            refill_interval_ms: 1,
        };
        let mut rl = RateLimiter::new(config);
        rl.check("k").unwrap();
        rl.check("k").unwrap();
        // Even with huge refill rate, tokens shouldn't exceed max
        assert!(rl.available("k") <= 5);
    }

    #[test]
    fn bucket_count_increases() {
        let mut rl = RateLimiter::new(BucketConfig::default());
        assert_eq!(rl.bucket_count(), 0);
        rl.check("a").unwrap();
        assert_eq!(rl.bucket_count(), 1);
        rl.check("b").unwrap();
        assert_eq!(rl.bucket_count(), 2);
    }

    // -----------------------------------------------------------------------
    // Circuit Breaker: starts closed
    // -----------------------------------------------------------------------

    #[test]
    fn circuit_starts_closed() {
        let mut cb = CircuitBreaker::new("test", CircuitBreakerConfig::default());
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(!cb.is_open());
        assert!(cb.allow_request().is_ok());
    }

    // -----------------------------------------------------------------------
    // Circuit Breaker: closed → open on threshold
    // -----------------------------------------------------------------------

    #[test]
    fn closed_opens_on_failure_threshold() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout_secs: 60,
            half_open_max_calls: 2,
        };
        let mut cb = CircuitBreaker::new("svc", config);
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(cb.is_open());
    }

    #[test]
    fn open_rejects_requests() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            ..Default::default()
        };
        let mut cb = CircuitBreaker::new("svc", config);
        cb.record_failure();
        let err = cb.allow_request().unwrap_err();
        assert!(matches!(err, CircuitBreakerError::Open { .. }));
    }

    // -----------------------------------------------------------------------
    // Circuit Breaker: success resets failure count in closed
    // -----------------------------------------------------------------------

    #[test]
    fn success_resets_failure_count() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        };
        let mut cb = CircuitBreaker::new("svc", config);
        cb.record_failure();
        cb.record_failure();
        cb.record_success();
        assert_eq!(cb.failure_count, 0);
        // Two more failures needed to open
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
    }

    // -----------------------------------------------------------------------
    // Circuit Breaker: open → half-open on timeout
    // -----------------------------------------------------------------------

    #[test]
    fn open_transitions_to_half_open_on_timeout() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            timeout_secs: 0, // immediate timeout for testing
            ..Default::default()
        };
        let mut cb = CircuitBreaker::new("svc", config);
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);

        // Sleep a tiny bit to ensure timeout elapses
        std::thread::sleep(std::time::Duration::from_millis(10));

        // allow_request triggers check_timeout
        let result = cb.allow_request();
        assert!(result.is_ok() || cb.state() == CircuitState::HalfOpen);
    }

    #[test]
    fn half_open_allows_limited_calls() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            timeout_secs: 0,
            half_open_max_calls: 2,
            success_threshold: 2,
        };
        let mut cb = CircuitBreaker::new("svc", config);
        cb.record_failure();
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Should allow up to 2 calls
        assert!(cb.allow_request().is_ok());
        assert!(cb.allow_request().is_ok());
        assert!(cb.allow_request().is_err());
    }

    // -----------------------------------------------------------------------
    // Circuit Breaker: half-open → closed on success
    // -----------------------------------------------------------------------

    #[test]
    fn half_open_closes_on_success_threshold() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            timeout_secs: 0,
            half_open_max_calls: 3,
            success_threshold: 2,
        };
        let mut cb = CircuitBreaker::new("svc", config);
        cb.record_failure();
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Transition to half-open
        let _ = cb.allow_request();
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        cb.record_success();
        assert_eq!(cb.state(), CircuitState::HalfOpen);
        cb.record_success();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    // -----------------------------------------------------------------------
    // Circuit Breaker: half-open → open on failure
    // -----------------------------------------------------------------------

    #[test]
    fn half_open_reopens_on_failure() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            timeout_secs: 0,
            half_open_max_calls: 3,
            success_threshold: 2,
        };
        let mut cb = CircuitBreaker::new("svc", config);
        cb.record_failure();
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Transition to half-open
        let _ = cb.allow_request();
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
    }

    // -----------------------------------------------------------------------
    // Circuit Breaker: reset
    // -----------------------------------------------------------------------

    #[test]
    fn reset_clears_state() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            ..Default::default()
        };
        let mut cb = CircuitBreaker::new("svc", config);
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        cb.reset();
        assert_eq!(cb.state(), CircuitState::Closed);
        assert_eq!(cb.failure_count, 0);
        assert!(cb.opened_at.is_none());
    }

    // -----------------------------------------------------------------------
    // Circuit Breaker: last_failure set
    // -----------------------------------------------------------------------

    #[test]
    fn last_failure_recorded() {
        let mut cb = CircuitBreaker::new("svc", CircuitBreakerConfig::default());
        assert!(cb.last_failure.is_none());
        cb.record_failure();
        assert!(cb.last_failure.is_some());
    }

    // -----------------------------------------------------------------------
    // Circuit Breaker: opened_at set
    // -----------------------------------------------------------------------

    #[test]
    fn opened_at_set_when_opened() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            ..Default::default()
        };
        let mut cb = CircuitBreaker::new("svc", config);
        assert!(cb.opened_at.is_none());
        cb.record_failure();
        assert!(cb.opened_at.is_none());
        cb.record_failure();
        assert!(cb.opened_at.is_some());
    }

    // -----------------------------------------------------------------------
    // Circuit Breaker: error messages
    // -----------------------------------------------------------------------

    #[test]
    fn open_error_message_contains_name() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            ..Default::default()
        };
        let mut cb = CircuitBreaker::new("email-svc", config);
        cb.record_failure();
        let err = cb.allow_request().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("email-svc"));
    }

    #[test]
    fn half_open_exhausted_message_contains_name() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            timeout_secs: 0,
            half_open_max_calls: 1,
            success_threshold: 1,
        };
        let mut cb = CircuitBreaker::new("db-svc", config);
        cb.record_failure();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let _ = cb.allow_request(); // consume the 1 allowed call
        let err = cb.allow_request().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("db-svc"));
    }

    // -----------------------------------------------------------------------
    // CircuitState: serde round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn circuit_state_serde_roundtrip() {
        let states = [
            CircuitState::Closed,
            CircuitState::Open,
            CircuitState::HalfOpen,
        ];
        for s in &states {
            let json = serde_json::to_value(s).unwrap();
            let back: CircuitState = serde_json::from_value(json).unwrap();
            assert_eq!(*s, back);
        }
    }

    // -----------------------------------------------------------------------
    // Combined RequestGuard: rate limit hit first
    // -----------------------------------------------------------------------

    #[test]
    fn guard_rate_limit_hit() {
        let config = BucketConfig {
            max_tokens: 1,
            refill_rate: 1,
            refill_interval_ms: 100_000,
        };
        let mut guard = RequestGuard::new(config);
        guard.register_circuit("svc", CircuitBreakerConfig::default());
        assert!(guard.check("k", Some("svc")).is_ok());
        // Second call should fail on rate limit
        assert!(guard.check("k", Some("svc")).is_err());
    }

    // -----------------------------------------------------------------------
    // Combined RequestGuard: circuit breaker hit
    // -----------------------------------------------------------------------

    #[test]
    fn guard_circuit_breaker_hit() {
        let config = BucketConfig {
            max_tokens: 100,
            refill_rate: 10,
            refill_interval_ms: 100,
        };
        let mut guard = RequestGuard::new(config);
        let cb_config = CircuitBreakerConfig {
            failure_threshold: 1,
            ..Default::default()
        };
        guard.register_circuit("svc", cb_config);
        guard.record_failure("svc");

        // Rate limit passes, circuit breaker blocks
        let result = guard.check("k", Some("svc"));
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("open"), "expected 'open' in error, got: {}", msg);
    }

    // -----------------------------------------------------------------------
    // Combined RequestGuard: both pass
    // -----------------------------------------------------------------------

    #[test]
    fn guard_both_pass() {
        let config = BucketConfig {
            max_tokens: 100,
            refill_rate: 10,
            refill_interval_ms: 100,
        };
        let mut guard = RequestGuard::new(config);
        guard.register_circuit("svc", CircuitBreakerConfig::default());
        assert!(guard.check("k", Some("svc")).is_ok());
    }

    #[test]
    fn guard_no_circuit_always_passes_rate_check() {
        let config = BucketConfig {
            max_tokens: 100,
            refill_rate: 10,
            refill_interval_ms: 100,
        };
        let mut guard = RequestGuard::new(config);
        assert!(guard.check("k", None).is_ok());
    }

    // -----------------------------------------------------------------------
    // Combined RequestGuard: record success / failure
    // -----------------------------------------------------------------------

    #[test]
    fn guard_record_success_resets_circuit() {
        let config = BucketConfig {
            max_tokens: 100,
            refill_rate: 10,
            refill_interval_ms: 100,
        };
        let mut guard = RequestGuard::new(config);
        let cb_config = CircuitBreakerConfig {
            failure_threshold: 1,
            ..Default::default()
        };
        guard.register_circuit("svc", cb_config);
        guard.record_failure("svc");
        // Reset the circuit manually
        guard.circuit_breakers.get_mut("svc").unwrap().reset();
        guard.record_success("svc");
        assert_eq!(guard.circuit_breakers["svc"].failure_count, 0);
    }

    #[test]
    fn guard_record_failure_on_unknown_circuit_noop() {
        let mut guard = RequestGuard::new(BucketConfig::default());
        // Should not panic
        guard.record_failure("nonexistent");
        guard.record_success("nonexistent");
    }
}

// Helper methods for tests
impl RateLimitError {
    pub fn key(&self) -> &str {
        match self {
            RateLimitError::Exceeded { key, .. } => key,
        }
    }
    pub fn retry_after_ms(&self) -> u64 {
        match self {
            RateLimitError::Exceeded { retry_after_ms, .. } => *retry_after_ms,
        }
    }
}
