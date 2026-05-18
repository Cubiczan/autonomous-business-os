//! # Business OS Pipeline
//!
//! Full orchestration pipeline that ties together all subsystems:
//! audit, approvals, scoring, security, orchestrator, and rate limiting.

use crate::approval::{ApprovalError, ApprovalService};
use crate::audit::AuditService;
use crate::orchestrator::{Agent, Orchestrator, OrchestratorConfig, OrchestratorError};
use crate::rate_limit::{BucketConfig, RequestGuard};
use crate::scoring::{LeadScoringService, ScoringConfig};
use crate::security::{SecurityError, SecurityService};
use crate::types::*;

// ===========================================================================
// Configuration
// ===========================================================================

/// Top-level configuration for the Business OS.
#[derive(Debug, Clone)]
pub struct BusinessOSConfig {
    /// Admin API key for authentication.
    pub admin_api_key: String,
    /// Replay window in seconds for request timestamp validation.
    pub replay_window_secs: i64,
    /// Max tokens per rate-limit bucket.
    pub rate_limit_max_tokens: u32,
    /// Tokens added per refill interval.
    pub rate_limit_refill_rate: u32,
    /// Failures before a circuit breaker opens.
    pub circuit_breaker_failure_threshold: u32,
    /// Seconds a circuit breaker stays open.
    pub circuit_breaker_timeout_secs: i64,
    /// Maximum workflow retry attempts.
    pub max_workflow_retries: u32,
}

impl Default for BusinessOSConfig {
    fn default() -> Self {
        Self {
            admin_api_key: "changeme".into(),
            replay_window_secs: 300,
            rate_limit_max_tokens: 100,
            rate_limit_refill_rate: 10,
            circuit_breaker_failure_threshold: 5,
            circuit_breaker_timeout_secs: 60,
            max_workflow_retries: 3,
        }
    }
}

// ===========================================================================
// System Health & Stats
// ===========================================================================

/// Snapshot of overall system health.
#[derive(Debug, Clone)]
pub struct SystemHealth {
    pub audit_entries: usize,
    pub pending_approvals: usize,
    pub registered_agents: usize,
    pub rate_limit_buckets: usize,
    pub circuit_breakers: usize,
}

/// Aggregate system statistics.
#[derive(Debug, Clone)]
pub struct SystemStats {
    pub total_workflows: usize,
    pub completed_workflows: usize,
    pub failed_workflows: usize,
    pub pending_workflows: usize,
    pub total_approvals: usize,
    pub approved_count: usize,
    pub rejected_count: usize,
}

// ===========================================================================
// BusinessOS
// ===========================================================================

/// The top-level façade that exposes a unified API over all subsystems.
///
/// Consumers interact with this struct rather than individual services.
pub struct BusinessOS {
    pub audit: AuditService,
    pub approvals: ApprovalService,
    pub scoring: LeadScoringService,
    pub security: SecurityService,
    pub orchestrator: Orchestrator,
    pub guard: RequestGuard,
    pub config: BusinessOSConfig,
}

impl BusinessOS {
    /// Create a new Business OS instance from the given configuration.
    pub fn new(config: BusinessOSConfig) -> Self {
        let orchestrator_config = OrchestratorConfig {
            max_retries: config.max_workflow_retries,
            ..Default::default()
        };

        let audit = AuditService::new();
        let orchestrator = Orchestrator::with_audit(
            orchestrator_config,
            AuditService::new(),
        );

        Self {
            audit,
            approvals: ApprovalService::new(),
            scoring: LeadScoringService::new(ScoringConfig::default()),
            security: SecurityService::new(
                config.admin_api_key.clone(),
                config.replay_window_secs,
            ),
            orchestrator,
            guard: RequestGuard::new(BucketConfig {
                max_tokens: config.rate_limit_max_tokens,
                refill_rate: config.rate_limit_refill_rate,
                refill_interval_ms: 100,
            }),
            config,
        }
    }

    /// Builder: register an agent for a workflow kind.
    pub fn with_agent(mut self, kind: WorkflowKind, agent: Box<dyn Agent>) -> Self {
        self.orchestrator = self.orchestrator.with_agent(kind, agent);
        self
    }

    /// Register a named circuit breaker on the request guard.
    pub fn register_circuit(
        &mut self,
        name: &str,
        failure_threshold: u32,
        timeout_secs: i64,
    ) {
        use crate::rate_limit::CircuitBreakerConfig;
        self.guard.register_circuit(
            name,
            CircuitBreakerConfig {
                failure_threshold,
                timeout_secs,
                ..Default::default()
            },
        );
    }

    // -------------------------------------------------------------------
    // High-level operations
    // -------------------------------------------------------------------

    /// Submit and immediately run a workflow.
    ///
    /// Combines `submit_workflow` + `run_workflow` in one call.
    pub fn submit_and_run(
        &mut self,
        kind: WorkflowKind,
        payload: serde_json::Value,
        source: &str,
    ) -> Result<Workflow, OrchestratorError> {
        let mut wf = self.orchestrator.submit_workflow(kind, payload, source);
        self.orchestrator.run_workflow(&mut wf)?;
        Ok(wf)
    }

    /// Score a lead and return `(score, tier)`.
    pub fn score_lead(&self, lead: &Lead) -> (u32, ScoreTier) {
        self.scoring.score(lead)
    }

    /// Request a human approval gate.
    ///
    /// Returns a reference to the newly created [`HumanApproval`].
    pub fn request_approval(
        &mut self,
        workflow_id: &str,
        title: &str,
        reason: &str,
        action: serde_json::Value,
    ) -> &HumanApproval {
        let approval = self.approvals.request_approval(workflow_id, title, reason, action);
        self.audit
            .record_approval_requested(workflow_id, title);
        approval
    }

    /// Approve an open approval.
    pub fn approve(
        &mut self,
        approval_id: &str,
        decided_by: &str,
    ) -> Result<&HumanApproval, ApprovalError> {
        let result = self.approvals.approve(approval_id, decided_by, None)?;
        self.audit
            .record_approval_granted(
                &result.workflow_id,
                decided_by,
            );
        Ok(result)
    }

    /// Reject an open approval with a note.
    pub fn reject(
        &mut self,
        approval_id: &str,
        decided_by: &str,
        note: &str,
    ) -> Result<&HumanApproval, ApprovalError> {
        let result = self.approvals.reject(approval_id, decided_by, note)?;
        self.audit
            .record_approval_rejected(
                &result.workflow_id,
                decided_by,
                note,
            );
        Ok(result)
    }

    /// Verify a webhook request signature.
    ///
    /// Combines replay-window check and HMAC verification.
    pub fn verify_request(
        &self,
        key: &str,
        timestamp: &str,
        body: &str,
        signature: &str,
    ) -> Result<bool, SecurityError> {
        // Replay check first
        if !self.security.is_within_replay_window(timestamp)? {
            return Err(SecurityError::ExpiredRequest);
        }
        // HMAC verification
        self.security
            .verify_webhook_signature(key, timestamp, body, signature)
    }

    /// Check rate limit for a given key.
    pub fn check_rate_limit(&mut self, key: &str) -> Result<(), String> {
        self.guard.check(key, None)
    }

    /// Return a health snapshot of the system.
    pub fn health_check(&self) -> SystemHealth {
        SystemHealth {
            audit_entries: self.audit.len(),
            pending_approvals: self.approvals.pending_len(),
            registered_agents: self.orchestrator.registry().len(),
            rate_limit_buckets: self.guard.rate_limiter.bucket_count(),
            circuit_breakers: self.guard.circuit_breakers.len(),
        }
    }

    /// Return audit trail entries for a specific workflow.
    pub fn audit_trail(&self, workflow_id: &str) -> Vec<&AuditEntry> {
        let mut entries = self.audit.entries_for_workflow(workflow_id);
        entries.extend(self.orchestrator.audit().entries_for_workflow(workflow_id));
        entries
    }

    /// Return aggregate system statistics.
    pub fn stats(&self) -> SystemStats {
        let approval_stats = self.approvals.stats();
        SystemStats {
            total_workflows: 0,
            completed_workflows: 0,
            failed_workflows: 0,
            pending_workflows: 0,
            total_approvals: approval_stats.total_requested,
            approved_count: approval_stats.approved_count,
            rejected_count: approval_stats.rejected_count,
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::AgentContext;

    // -------------------------------------------------------------------
    // Mock agent
    // -------------------------------------------------------------------

    struct EchoAgent;

    impl Agent for EchoAgent {
        fn name(&self) -> &str {
            "echo"
        }
        fn run(
            &self,
            workflow: &Workflow,
            _ctx: &mut AgentContext,
        ) -> Result<serde_json::Value, String> {
            Ok(workflow.payload.clone())
        }
    }

    /// Helper: create a BusinessOS with an echo agent registered.
    fn make_os() -> BusinessOS {
        BusinessOS::new(BusinessOSConfig::default())
            .with_agent(WorkflowKind::LeadQualification, Box::new(EchoAgent))
    }

    // -------------------------------------------------------------------
    // Construction
    // -------------------------------------------------------------------

    #[test]
    fn new_os_is_healthy() {
        let os = BusinessOS::new(BusinessOSConfig::default());
        let h = os.health_check();
        assert_eq!(h.audit_entries, 0);
        assert_eq!(h.pending_approvals, 0);
        assert_eq!(h.registered_agents, 0);
    }

    #[test]
    fn default_config_values() {
        let cfg = BusinessOSConfig::default();
        assert_eq!(cfg.replay_window_secs, 300);
        assert_eq!(cfg.rate_limit_max_tokens, 100);
        assert_eq!(cfg.max_workflow_retries, 3);
    }

    // -------------------------------------------------------------------
    // Submit and run
    // -------------------------------------------------------------------

    #[test]
    fn submit_and_run_success() {
        let mut os = make_os();
        let payload = serde_json::json!({"email": "a@b.com"});
        let wf = os
            .submit_and_run(WorkflowKind::LeadQualification, payload.clone(), "test")
            .unwrap();
        assert_eq!(wf.status, WorkflowStatus::Completed);
        assert_eq!(wf.result, Some(payload));
    }

    #[test]
    fn submit_and_run_no_agent() {
        let mut os = BusinessOS::new(BusinessOSConfig::default());
        let err = os.submit_and_run(
            WorkflowKind::FinanceOperations,
            serde_json::json!({}),
            "test",
        );
        assert!(err.is_err());
        assert!(matches!(err.unwrap_err(), OrchestratorError::NoAgent(_)));
    }

    // -------------------------------------------------------------------
    // Lead scoring
    // -------------------------------------------------------------------

    #[test]
    fn score_lead_basic() {
        let os = make_os();
        let lead = Lead {
            id: "l-1".into(),
            email: "jane@example.com".into(),
            company: None,
            name: None,
            source: "web".into(),
            score: None,
            tier: None,
            enrichment: serde_json::json!({}),
            outreach: serde_json::json!({}),
            created_at: chrono::Utc::now(),
        };
        let (score, tier) = os.score_lead(&lead);
        assert!(score > 0);
        // With just an email (10 pts) and no other signals, should be Tier C
        assert_eq!(tier, ScoreTier::C);
    }

    // -------------------------------------------------------------------
    // Approvals
    // -------------------------------------------------------------------

    #[test]
    fn request_approval_returns_open() {
        let mut os = make_os();
        let approval = os.request_approval(
            "wf-1",
            "Review deal",
            "High value",
            serde_json::json!({"action": "send"}),
        );
        assert_eq!(approval.status, ApprovalStatus::Open);
    }

    #[test]
    fn approve_workflow_records_audit() {
        let mut os = make_os();
        let approval = os.request_approval("wf-1", "Title", "Reason", serde_json::json!({}));
        let id = approval.id.clone();
        let before = os.audit.len();
        os.approve(&id, "alice").unwrap();
        assert!(os.audit.len() > before);
    }

    #[test]
    fn reject_workflow_records_audit() {
        let mut os = make_os();
        let approval = os.request_approval("wf-1", "Title", "Reason", serde_json::json!({}));
        let id = approval.id.clone();
        os.reject(&id, "bob", "Not approved").unwrap();
        let decided = os.approvals.get_by_id(&id).unwrap();
        assert_eq!(decided.status, ApprovalStatus::Rejected);
    }

    #[test]
    fn approve_nonexistent_errors() {
        let mut os = make_os();
        let err = os.approve("nonexistent", "alice");
        assert!(err.is_err());
    }

    // -------------------------------------------------------------------
    // Security
    // -------------------------------------------------------------------

    #[test]
    fn verify_request_valid_signature() {
        let os = BusinessOS::new(BusinessOSConfig::default());
        let ts = chrono::Utc::now().timestamp().to_string();
        let body = r#"{"test": true}"#;
        let secret = "webhook-secret";
        let sig = format!(
            "v0={}",
            SecurityService::compute_hmac_hex(secret, &format!("{}{}", ts, body))
        );
        let result = os.verify_request(secret, &ts, body, &sig).unwrap();
        assert!(result);
    }

    #[test]
    fn verify_request_invalid_signature() {
        let os = BusinessOS::new(BusinessOSConfig::default());
        let ts = chrono::Utc::now().timestamp().to_string();
        let result = os.verify_request("secret", &ts, "body", "v0=badsig").unwrap();
        assert!(!result);
    }

    #[test]
    fn verify_request_expired_timestamp() {
        let os = BusinessOS::new(BusinessOSConfig::default());
        let old_ts = (chrono::Utc::now().timestamp() - 600).to_string();
        let err = os.verify_request("secret", &old_ts, "body", "v0=sig");
        assert!(err.is_err());
        assert!(matches!(err.unwrap_err(), SecurityError::ExpiredRequest));
    }

    // -------------------------------------------------------------------
    // Rate limiting
    // -------------------------------------------------------------------

    #[test]
    fn rate_limit_allows_initial() {
        let mut os = BusinessOS::new(BusinessOSConfig::default());
        assert!(os.check_rate_limit("user-1").is_ok());
    }

    #[test]
    fn rate_limit_blocks_after_exhaustion() {
        let config = BusinessOSConfig {
            rate_limit_max_tokens: 1,
            rate_limit_refill_rate: 1,
            ..Default::default()
        };
        let mut os = BusinessOS::new(config);
        assert!(os.check_rate_limit("user-1").is_ok());
        assert!(os.check_rate_limit("user-1").is_err());
    }

    // -------------------------------------------------------------------
    // Health check
    // -------------------------------------------------------------------

    #[test]
    fn health_check_after_operations() {
        let mut os = make_os();
        os.submit_and_run(
            WorkflowKind::LeadQualification,
            serde_json::json!({}),
            "test",
        )
        .unwrap();
        os.request_approval("wf-1", "Title", "Reason", serde_json::json!({}));

        let health = os.health_check();
        assert!(health.audit_entries > 0);
        assert_eq!(health.pending_approvals, 1);
        assert_eq!(health.registered_agents, 1);
    }

    // -------------------------------------------------------------------
    // Audit trail
    // -------------------------------------------------------------------

    #[test]
    fn audit_trail_returns_entries() {
        let mut os = make_os();
        let wf = os
            .submit_and_run(
                WorkflowKind::LeadQualification,
                serde_json::json!({}),
                "test",
            )
            .unwrap();
        let trail = os.audit_trail(&wf.id);
        assert!(!trail.is_empty());
    }

    #[test]
    fn audit_trail_empty_for_unknown() {
        let os = make_os();
        let trail = os.audit_trail("nonexistent");
        assert!(trail.is_empty());
    }

    // -------------------------------------------------------------------
    // Stats
    // -------------------------------------------------------------------

    #[test]
    fn stats_empty() {
        let os = make_os();
        let s = os.stats();
        assert_eq!(s.total_approvals, 0);
    }

    #[test]
    fn stats_with_approvals() {
        let mut os = make_os();
        let approval = os.request_approval("wf-1", "A", "R", serde_json::json!({}));
        let id = approval.id.clone();
        os.approve(&id, "alice").unwrap();
        let s = os.stats();
        assert_eq!(s.total_approvals, 1);
        assert_eq!(s.approved_count, 1);
    }

    // -------------------------------------------------------------------
    // Circuit breaker registration
    // -------------------------------------------------------------------

    #[test]
    fn register_circuit_increases_count() {
        let mut os = make_os();
        os.register_circuit("email-svc", 5, 60);
        let health = os.health_check();
        assert_eq!(health.circuit_breakers, 1);
    }
}
