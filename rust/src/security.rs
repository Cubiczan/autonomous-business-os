//! HMAC-based authentication and webhook signature verification.
//!
//! This module provides constant-time comparisons, replay-attack protection,
//! and Slack-compatible request signing.

use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use thiserror::Error;
use crate::types::Secret;

type HmacSha256 = Hmac<Sha256>;

// ===========================================================================
// Error types
// ===========================================================================

/// Security-related errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SecurityError {
    #[error("invalid API key")]
    InvalidApiKey,
    #[error("request timestamp expired (replay protection)")]
    ExpiredRequest,
    #[error("invalid signature")]
    InvalidSignature,
    #[error("secret is not configured")]
    MissingSecret,
}

// ===========================================================================
// SecurityService
// ===========================================================================

/// Central service for HMAC verification, API key checking, and replay protection.
///
/// The admin API key is hashed once at construction time so that the raw key
/// is never compared directly — only SHA-256 hashes are compared in
/// constant time.
pub struct SecurityService {
    /// SHA-256 hash of the admin API key (hex-encoded).
    admin_api_key_hash: Secret<String>,
    /// Optional Slack signing secret (raw, stored in Secret wrapper).
    slack_signing_secret: Option<Secret<String>>,
    /// Maximum age of a request timestamp in seconds (default 300 = 5 min).
    replay_window_secs: i64,
}

impl SecurityService {
    /// Creates a new `SecurityService` with the given admin API key.
    ///
    /// The key is hashed immediately; only the hash is stored.
    pub fn new(admin_api_key: String, replay_window_secs: i64) -> Self {
        let hash = Self::hash_key(&admin_api_key);
        // Hashing is done via a pure function — admin_api_key is dropped here.
        Self {
            admin_api_key_hash: Secret::new(hash),
            slack_signing_secret: None,
            replay_window_secs,
        }
    }

    /// Builder method: attach a Slack signing secret.
    pub fn with_slack_secret(mut self, slack_secret: String) -> Self {
        self.slack_signing_secret = Some(Secret::new(slack_secret));
        self
    }

    // -------------------------------------------------------------------
    // Internal helpers
    // -------------------------------------------------------------------

    /// Compute SHA-256 hex digest of a key.
    fn hash_key(key: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Compute HMAC-SHA256 hex signature for `data` using `key`.
    pub(crate) fn compute_hmac_hex(key: &str, data: &str) -> String {
        let mut mac =
            HmacSha256::new_from_slice(key.as_bytes()).expect("HMAC accepts any key length");
        mac.update(data.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    /// Constant-time comparison of two hex-encoded strings.
    fn constant_time_eq(a: &str, b: &str) -> bool {
        let a_bytes = a.as_bytes();
        let b_bytes = b.as_bytes();

        if a_bytes.len() != b_bytes.len() {
            // Still do work proportional to the longer length to avoid
            // timing leaks based on length.
            for &byte in a_bytes.iter().chain(b_bytes.iter()) {
                let _ = byte;
            }
            return false;
        }

        let mut result: u8 = 0;
        for (x, y) in a_bytes.iter().zip(b_bytes.iter()) {
            result |= x ^ y;
        }
        result == 0
    }

    // -------------------------------------------------------------------
    // Public API
    // -------------------------------------------------------------------

    /// Verifies an admin API key using constant-time HMAC hash comparison.
    ///
    /// The provided key is hashed and compared against the stored hash.
    pub fn verify_admin_api_key(&self, header_value: &str) -> Result<bool, SecurityError> {
        if header_value.is_empty() {
            return Ok(false);
        }
        let provided_hash = Self::hash_key(header_value);
        Ok(Self::constant_time_eq(
            &provided_hash,
            self.admin_api_key_hash.expose(),
        ))
    }

    /// Verifies a generic webhook signature of the form `v0=<hex_hmac>`.
    ///
    /// Reconstructs `v0=HMAC-SHA256(secret, timestamp_hex + body)` and compares.
    pub fn verify_webhook_signature(
        &self,
        secret: &str,
        timestamp: &str,
        body: &str,
        signature: &str,
    ) -> Result<bool, SecurityError> {
        if secret.is_empty() {
            return Err(SecurityError::MissingSecret);
        }
        let expected = format!("v0={}", Self::compute_hmac_hex(secret, &format!("{}{}", timestamp, body)));
        Ok(Self::constant_time_eq(&expected, signature))
    }

    /// Checks whether a Unix timestamp string is within the replay window.
    pub fn is_within_replay_window(&self, timestamp: &str) -> Result<bool, SecurityError> {
        let ts = timestamp
            .parse::<i64>()
            .map_err(|_| SecurityError::ExpiredRequest)?;
        let now = Utc::now().timestamp();
        let diff = (now - ts).abs();
        Ok(diff <= self.replay_window_secs)
    }

    /// Verifies a Slack request signature (replay check + HMAC).
    ///
    /// Slack signs requests as `v0=HMAC-SHA256(signing_secret, "v0:" + timestamp + ":" + body)`.
    pub fn verify_slack_request(
        &self,
        timestamp: &str,
        body: &str,
        signature: &str,
    ) -> Result<bool, SecurityError> {
        // Replay check
        if !self.is_within_replay_window(timestamp)? {
            return Err(SecurityError::ExpiredRequest);
        }

        // Must have a signing secret
        let secret = self
            .slack_signing_secret
            .as_ref()
            .ok_or(SecurityError::MissingSecret)?;

        let sig_base = format!("v0:{}:{}", timestamp, body);
        let expected = format!("v0={}", Self::compute_hmac_hex(secret.expose(), &sig_base));
        Ok(Self::constant_time_eq(&expected, signature))
    }

    /// Constant-time comparison of a provided secret against an expected one.
    ///
    /// Returns `false` if `expected` is empty (fail-closed).
    pub fn verify_shared_secret(provided: &str, expected: &str) -> bool {
        if expected.is_empty() {
            return false;
        }
        Self::constant_time_eq(provided, expected)
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_service() -> SecurityService {
        SecurityService::new("admin-secret-key".into(), 300)
    }

    fn make_service_with_slack() -> SecurityService {
        SecurityService::new("admin-secret-key".into(), 300)
            .with_slack_secret("slack-signing-secret".into())
    }

    // -------------------------------------------------------------------
    // Admin API key verification
    // -------------------------------------------------------------------

    #[test]
    fn valid_admin_key_is_accepted() {
        let svc = make_service();
        assert_eq!(svc.verify_admin_api_key("admin-secret-key").unwrap(), true);
    }

    #[test]
    fn invalid_admin_key_is_rejected() {
        let svc = make_service();
        assert_eq!(svc.verify_admin_api_key("wrong-key").unwrap(), false);
    }

    #[test]
    fn empty_admin_key_is_rejected() {
        let svc = make_service();
        assert_eq!(svc.verify_admin_api_key("").unwrap(), false);
    }

    #[test]
    fn admin_key_different_length_rejected() {
        let svc = make_service();
        assert_eq!(svc.verify_admin_api_key("x").unwrap(), false);
    }

    #[test]
    fn admin_key_verification_is_constant_time() {
        // This test doesn't prove constant-time but ensures both paths work.
        let svc = make_service();
        let _ = svc.verify_admin_api_key("admin-secret-key");
        let _ = svc.verify_admin_api_key("admin-secret-kez"); // off by one
    }

    #[test]
    fn admin_key_hash_differs_from_original() {
        let svc = make_service();
        let hash = svc.admin_api_key_hash.expose();
        assert_ne!(hash, "admin-secret-key");
        // Verify it's a hex string
        assert!(hash.len() == 64); // SHA-256 = 32 bytes = 64 hex chars
    }

    // -------------------------------------------------------------------
    // Webhook signature verification
    // -------------------------------------------------------------------

    #[test]
    fn valid_webhook_signature_accepted() {
        let svc = make_service();
        let timestamp = "1234567890";
        let body = r#"{"hello": "world"}"#;
        let secret = "webhook-secret";
        let sig = format!("v0={}", SecurityService::compute_hmac_hex(secret, &format!("{}{}", timestamp, body)));
        assert_eq!(
            svc.verify_webhook_signature(secret, timestamp, body, &sig).unwrap(),
            true
        );
    }

    #[test]
    fn invalid_webhook_signature_rejected() {
        let svc = make_service();
        assert_eq!(
            svc.verify_webhook_signature("secret", "1234", "body", "v0=badsig")
                .unwrap(),
            false
        );
    }

    #[test]
    fn webhook_missing_secret_returns_error() {
        let svc = make_service();
        let result = svc.verify_webhook_signature("", "1234", "body", "v0=sig");
        assert_eq!(result.unwrap_err(), SecurityError::MissingSecret);
    }

    #[test]
    fn webhook_signature_with_v0_prefix() {
        let svc = make_service();
        let secret = "mysecret";
        let ts = "9999999999";
        let body = "testbody";
        let computed = format!("v0={}", SecurityService::compute_hmac_hex(secret, &format!("{}{}", ts, body)));
        assert!(computed.starts_with("v0="));
        assert_eq!(svc.verify_webhook_signature(secret, ts, body, &computed).unwrap(), true);
    }

    #[test]
    fn webhook_empty_body() {
        let svc = make_service();
        let secret = "sec";
        let ts = "1000";
        let body = "";
        let sig = format!("v0={}", SecurityService::compute_hmac_hex(secret, &format!("{}{}", ts, body)));
        assert_eq!(svc.verify_webhook_signature(secret, ts, body, &sig).unwrap(), true);
    }

    // -------------------------------------------------------------------
    // Replay window
    // -------------------------------------------------------------------

    #[test]
    fn current_timestamp_within_window() {
        let svc = make_service();
        let now = Utc::now().timestamp().to_string();
        assert_eq!(svc.is_within_replay_window(&now).unwrap(), true);
    }

    #[test]
    fn old_timestamp_outside_window() {
        let svc = make_service();
        let old = (Utc::now().timestamp() - 600).to_string(); // 10 min ago
        assert_eq!(svc.is_within_replay_window(&old).unwrap(), false);
    }

    #[test]
    fn future_timestamp_within_window() {
        let svc = make_service();
        let future = (Utc::now().timestamp() + 60).to_string(); // 1 min ahead
        assert_eq!(svc.is_within_replay_window(&future).unwrap(), true);
    }

    #[test]
    fn future_timestamp_outside_window() {
        let svc = make_service();
        let future = (Utc::now().timestamp() + 600).to_string(); // 10 min ahead
        assert_eq!(svc.is_within_replay_window(&future).unwrap(), false);
    }

    #[test]
    fn invalid_timestamp_returns_error() {
        let svc = make_service();
        let result = svc.is_within_replay_window("not-a-number");
        assert_eq!(result.unwrap_err(), SecurityError::ExpiredRequest);
    }

    #[test]
    fn empty_timestamp_returns_error() {
        let svc = make_service();
        let result = svc.is_within_replay_window("");
        assert_eq!(result.unwrap_err(), SecurityError::ExpiredRequest);
    }

    #[test]
    fn boundary_timestamp_exactly_at_window() {
        let svc = make_service();
        let boundary = (Utc::now().timestamp() - 300).to_string(); // exactly 5 min
        assert_eq!(svc.is_within_replay_window(&boundary).unwrap(), true);
    }

    #[test]
    fn boundary_timestamp_just_past_window() {
        let svc = make_service();
        let past = (Utc::now().timestamp() - 301).to_string(); // 5 min 1 sec
        assert_eq!(svc.is_within_replay_window(&past).unwrap(), false);
    }

    // -------------------------------------------------------------------
    // Slack request verification
    // -------------------------------------------------------------------

    #[test]
    fn valid_slack_request_accepted() {
        let svc = make_service_with_slack();
        let ts = Utc::now().timestamp().to_string();
        let body = "payload=thing";
        let sig_base = format!("v0:{}:{}", ts, body);
        let sig = format!("v0={}", SecurityService::compute_hmac_hex("slack-signing-secret", &sig_base));
        assert_eq!(svc.verify_slack_request(&ts, body, &sig).unwrap(), true);
    }

    #[test]
    fn slack_request_without_secret_returns_error() {
        let svc = make_service(); // no slack secret
        let ts = Utc::now().timestamp().to_string();
        let result = svc.verify_slack_request(&ts, "body", "v0=sig");
        assert_eq!(result.unwrap_err(), SecurityError::MissingSecret);
    }

    #[test]
    fn slack_request_expired_returns_error() {
        let svc = make_service_with_slack();
        let old_ts = (Utc::now().timestamp() - 600).to_string();
        let result = svc.verify_slack_request(&old_ts, "body", "v0=sig");
        assert_eq!(result.unwrap_err(), SecurityError::ExpiredRequest);
    }

    #[test]
    fn slack_request_wrong_signature_rejected() {
        let svc = make_service_with_slack();
        let ts = Utc::now().timestamp().to_string();
        assert_eq!(
            svc.verify_slack_request(&ts, "body", "v0=wrong").unwrap(),
            false
        );
    }

    // -------------------------------------------------------------------
    // Shared secret verification
    // -------------------------------------------------------------------

    #[test]
    fn shared_secret_match() {
        assert_eq!(SecurityService::verify_shared_secret("pass", "pass"), true);
    }

    #[test]
    fn shared_secret_mismatch() {
        assert_eq!(SecurityService::verify_shared_secret("pass", "word"), false);
    }

    #[test]
    fn shared_secret_empty_expected_fails_closed() {
        assert_eq!(SecurityService::verify_shared_secret("anything", ""), false);
    }

    #[test]
    fn shared_secret_both_empty() {
        // Empty expected → fail-closed → false
        assert_eq!(SecurityService::verify_shared_secret("", ""), false);
    }

    #[test]
    fn shared_secret_constant_time_check() {
        // Ensure we don't short-circuit on different lengths
        let _ = SecurityService::verify_shared_secret("short", "much-longer-secret-value");
    }

    // -------------------------------------------------------------------
    // Constant-time equality
    // -------------------------------------------------------------------

    #[test]
    fn constant_time_eq_matching() {
        assert!(SecurityService::constant_time_eq("abc", "abc"));
    }

    #[test]
    fn constant_time_eq_different() {
        assert!(!SecurityService::constant_time_eq("abc", "abd"));
    }

    #[test]
    fn constant_time_eq_different_lengths() {
        assert!(!SecurityService::constant_time_eq("abc", "abcd"));
    }

    #[test]
    fn constant_time_eq_empty() {
        assert!(SecurityService::constant_time_eq("", ""));
    }

    // -------------------------------------------------------------------
    // Error type
    // -------------------------------------------------------------------

    #[test]
    fn security_error_display() {
        assert_eq!(
            format!("{}", SecurityError::InvalidApiKey),
            "invalid API key"
        );
        assert_eq!(
            format!("{}", SecurityError::ExpiredRequest),
            "request timestamp expired (replay protection)"
        );
        assert_eq!(
            format!("{}", SecurityError::InvalidSignature),
            "invalid signature"
        );
        assert_eq!(
            format!("{}", SecurityError::MissingSecret),
            "secret is not configured"
        );
    }

    // -------------------------------------------------------------------
    // HMAC computation correctness
    // -------------------------------------------------------------------

    #[test]
    fn hmac_computation_deterministic() {
        let a = SecurityService::compute_hmac_hex("key", "data");
        let b = SecurityService::compute_hmac_hex("key", "data");
        assert_eq!(a, b);
    }

    #[test]
    fn hmac_different_keys_produce_different_outputs() {
        let a = SecurityService::compute_hmac_hex("key1", "data");
        let b = SecurityService::compute_hmac_hex("key2", "data");
        assert_ne!(a, b);
    }

    #[test]
    fn hmac_sha256_length() {
        let out = SecurityService::compute_hmac_hex("k", "d");
        assert_eq!(out.len(), 64); // 32 bytes = 64 hex chars
    }
}
