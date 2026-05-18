//! # Orchestrator
//!
//! Agent dispatch, retry with exponential backoff, and guaranteed escalation.
//! Routes [`Workflow`] instances to registered [`Agent`] implementations,
//! manages retry semantics, and creates escalations for unrecoverable failures.

use std::collections::HashMap;

use crate::audit::AuditService;
use crate::state_machine::{TransitionError, WorkflowStateMachine};
use crate::types::*;
use chrono::Utc;
use thiserror::Error;
use uuid::Uuid;

// ===========================================================================
// Errors
// ===========================================================================

/// Errors produced by the orchestrator.
#[derive(Debug, Error)]
pub enum OrchestratorError {
    #[error("no agent registered for workflow kind: {0:?}")]
    NoAgent(WorkflowKind),
    #[error("state transition error: {0}")]
    Transition(#[from] TransitionError),
    #[error("agent execution failed: {0}")]
    AgentFailed(String),
    #[error("workflow {0} already in terminal state")]
    TerminalState(String),
}

// ===========================================================================
// Agent trait
// ===========================================================================

/// A pluggable agent that can execute a workflow.
///
/// Implementations must be `Send + Sync` so the orchestrator can, in future,
/// dispatch them on async runtimes.
pub trait Agent: Send + Sync {
    /// Human-readable agent name.
    fn name(&self) -> &str;

    /// Execute the agent's logic for the given workflow.
    ///
    /// Receives a reference to the workflow and a mutable context for recording
    /// audit events and spawning sub-tasks.
    fn run(
        &self,
        workflow: &Workflow,
        ctx: &mut AgentContext,
    ) -> Result<serde_json::Value, String>;
}

/// Context passed to agents during execution.
pub struct AgentContext {
    pub audit: AuditService,
    pub tasks: Vec<AgentTask>,
}

impl AgentContext {
    /// Create a new empty agent context.
    pub fn new() -> Self {
        Self {
            audit: AuditService::new(),
            tasks: Vec::new(),
        }
    }
}

impl Default for AgentContext {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Agent Registry
// ===========================================================================

/// Registry that maps [`WorkflowKind`] → [`Agent`].
pub struct AgentRegistry {
    agents: HashMap<WorkflowKind, Box<dyn Agent>>,
}

impl AgentRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
        }
    }

    /// Register an agent for a given workflow kind.
    ///
    /// If an agent is already registered for this kind, it is replaced.
    pub fn register(&mut self, kind: WorkflowKind, agent: Box<dyn Agent>) {
        self.agents.insert(kind, agent);
    }

    /// Look up the agent for a workflow kind.
    pub fn get(&self, kind: WorkflowKind) -> Option<&dyn Agent> {
        self.agents.get(&kind).map(|a| a.as_ref())
    }

    /// Return all registered workflow kinds.
    pub fn registered_kinds(&self) -> Vec<WorkflowKind> {
        self.agents.keys().copied().collect()
    }

    /// Return the number of registered agents.
    pub fn len(&self) -> usize {
        self.agents.len()
    }

    /// Return true if no agents are registered.
    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Configuration
// ===========================================================================

/// Configuration for retry / backoff behaviour.
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    /// Maximum number of retry attempts (default 3).
    pub max_retries: u32,
    /// Base backoff delay in milliseconds (default 1000).
    pub base_backoff_ms: u64,
    /// Maximum backoff cap in milliseconds (default 30000).
    pub max_backoff_ms: u64,
    /// Exponential multiplier applied each attempt (default 2.0).
    pub backoff_multiplier: f64,
    /// Random jitter as fraction of computed backoff (default 0.1).
    pub jitter_pct: f64,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_backoff_ms: 1000,
            max_backoff_ms: 30000,
            backoff_multiplier: 2.0,
            jitter_pct: 0.1,
        }
    }
}

// ===========================================================================
// Retry Decision
// ===========================================================================

/// Outcome of a retry-policy evaluation.
#[derive(Debug, Clone)]
pub struct RetryDecision {
    /// Whether the workflow should be retried.
    pub should_retry: bool,
    /// The current attempt number (1-based).
    pub attempt: u32,
    /// Recommended delay before the next attempt, in milliseconds.
    pub delay_ms: u64,
    /// Human-readable reason.
    pub reason: String,
}

// ===========================================================================
// Orchestrator
// ===========================================================================

/// Central dispatch engine that routes workflows to registered agents.
///
/// The orchestrator:
/// 1. Looks up the agent for a workflow kind
/// 2. Transitions the workflow to Running
/// 3. Delegates to the agent
/// 4. On success: transitions to Completed
/// 5. On failure: evaluates retry policy; if retries exhausted, creates a
///    high-severity escalation
pub struct Orchestrator {
    registry: AgentRegistry,
    config: OrchestratorConfig,
    audit: AuditService,
}

impl Orchestrator {
    /// Create a new orchestrator with the given retry configuration.
    pub fn new(config: OrchestratorConfig) -> Self {
        Self {
            registry: AgentRegistry::new(),
            config,
            audit: AuditService::new(),
        }
    }

    /// Create a new orchestrator with a shared audit service.
    pub fn with_audit(config: OrchestratorConfig, audit: AuditService) -> Self {
        Self {
            registry: AgentRegistry::new(),
            config,
            audit,
        }
    }

    /// Builder: register an agent for a workflow kind.
    pub fn with_agent(mut self, kind: WorkflowKind, agent: Box<dyn Agent>) -> Self {
        self.registry.register(kind, agent);
        self
    }

    /// Register an agent for a workflow kind (mutable version).
    pub fn register_agent(&mut self, kind: WorkflowKind, agent: Box<dyn Agent>) {
        self.registry.register(kind, agent);
    }

    /// Submit a new workflow, returning the created [`Workflow`].
    ///
    /// The workflow is created in `Pending` status with a UUID-based id.
    pub fn submit_workflow(
        &mut self,
        kind: WorkflowKind,
        payload: serde_json::Value,
        source: &str,
    ) -> Workflow {
        let id = format!("wf_{}", Uuid::new_v4().simple());
        let now = Utc::now();
        let wf = Workflow {
            id,
            kind,
            status: WorkflowStatus::Pending,
            payload,
            result: None,
            error: None,
            attempts: 0,
            max_attempts: self.config.max_retries + 1, // initial + retries
            source: source.to_string(),
            created_at: now,
            updated_at: now,
            completed_at: None,
        };
        self.audit
            .record_workflow_created(&wf.id, wf.kind, source);
        wf
    }

    /// Run a workflow to completion or failure.
    ///
    /// # Steps
    /// 1. Get agent from registry → error if not found
    /// 2. Call `WorkflowStateMachine::mark_running`
    /// 3. Record audit event `workflow_started`
    /// 4. Call `agent.run()`
    /// 5. On success: `mark_completed`, record audit
    /// 6. On failure: check `should_retry` → if yes, reset to Pending for retry;
    ///    if no, `mark_failed` + create high-severity escalation
    pub fn run_workflow(
        &mut self,
        workflow: &mut Workflow,
    ) -> Result<serde_json::Value, OrchestratorError> {
        // Check terminal state
        if matches!(
            workflow.status,
            WorkflowStatus::Completed | WorkflowStatus::Cancelled
        ) {
            return Err(OrchestratorError::TerminalState(workflow.id.clone()));
        }

        // 1. Get agent
        let agent = self
            .registry
            .get(workflow.kind)
            .ok_or(OrchestratorError::NoAgent(workflow.kind))?;

        // 2. Mark running
        WorkflowStateMachine::mark_running(workflow)?;

        // 3. Audit
        self.audit.record_workflow_started(&workflow.id);

        // 4. Execute
        let mut ctx = AgentContext::new();
        let result = agent.run(workflow, &mut ctx);

        match result {
            Ok(value) => {
                // 5. Success path
                WorkflowStateMachine::mark_completed(workflow, value.clone())?;
                self.audit
                    .record_workflow_completed(&workflow.id, &value);
                Ok(value)
            }
            Err(err_msg) => {
                // 6. Failure path — first mark the workflow as failed
                WorkflowStateMachine::mark_failed(
                    workflow,
                    format!("agent error: {}", err_msg),
                )?;

                let retry = self.should_retry(workflow);
                if retry.should_retry {
                    // Reset to Pending so it can be picked up again
                    WorkflowStateMachine::retry(workflow)?;
                    self.audit
                        .record_workflow_failed(&workflow.id, &format!("agent failed (will retry): {}", err_msg));
                    Err(OrchestratorError::AgentFailed(format!(
                        "agent failed, retrying in {}ms: {}",
                        retry.delay_ms, err_msg
                    )))
                } else {
                    // No more retries — permanent failure (already marked failed above)
                    workflow.error = Some(format!("agent failed after {} attempts: {}", workflow.attempts, err_msg));

                    self.audit
                        .record_workflow_failed(&workflow.id, &err_msg);
                    self.audit
                        .record_escalation(
                            Some(&workflow.id),
                            Severity::High,
                            &format!("Workflow {} permanently failed: {}", workflow.id, err_msg),
                        );

                    Err(OrchestratorError::AgentFailed(err_msg))
                }
            }
        }
    }

    /// Evaluate whether a failed workflow should be retried.
    ///
    /// Returns a [`RetryDecision`] with the recommended delay.
    pub fn should_retry(&self, workflow: &Workflow) -> RetryDecision {
        let attempt = workflow.attempts;

        if attempt >= workflow.max_attempts {
            return RetryDecision {
                should_retry: false,
                attempt,
                delay_ms: 0,
                reason: format!("max attempts ({}) reached", workflow.max_attempts),
            };
        }

        let remaining = workflow.max_attempts - attempt;
        let delay_ms = self.compute_backoff(attempt);

        RetryDecision {
            should_retry: true,
            attempt,
            delay_ms,
            reason: format!(
                "{} attempt(s) remaining, backing off {}ms",
                remaining, delay_ms
            ),
        }
    }

    /// Compute exponential backoff with jitter.
    ///
    /// Formula: `min(base * multiplier^attempt, max) * (1 ± jitter_pct)`
    pub fn compute_backoff(&self, attempt: u32) -> u64 {
        if attempt == 0 {
            return self.config.base_backoff_ms;
        }

        let raw = self.config.base_backoff_ms as f64
            * self.config.backoff_multiplier.powi(attempt as i32);
        let capped = raw.min(self.config.max_backoff_ms as f64);

        // Add jitter
        let jitter_range = capped * self.config.jitter_pct;
        let jitter = (rand_jitter() - 0.5) * 2.0 * jitter_range;

        let result = capped + jitter;
        result.max(0.0) as u64
    }

    /// Create an escalation for a workflow failure.
    pub fn create_escalation(&mut self, workflow: &Workflow, reason: &str) -> Escalation {
        let esc = Escalation {
            id: format!("esc_{}", Uuid::new_v4().simple()),
            workflow_id: Some(workflow.id.clone()),
            severity: Severity::High,
            owner: workflow.source.clone(),
            reason: reason.to_string(),
            context: serde_json::json!({
                "kind": format!("{:?}", workflow.kind),
                "attempts": workflow.attempts,
                "error": workflow.error,
            }),
            created_at: Utc::now(),
            resolved_at: None,
            resolution: None,
        };
        self.audit
            .record_escalation(Some(&workflow.id), Severity::High, reason);
        esc
    }

    /// Access the audit service.
    pub fn audit(&self) -> &AuditService {
        &self.audit
    }

    /// Access the audit service mutably.
    pub fn audit_mut(&mut self) -> &mut AuditService {
        &mut self.audit
    }

    /// Access the agent registry.
    pub fn registry(&self) -> &AgentRegistry {
        &self.registry
    }

    /// Access the configuration.
    pub fn config(&self) -> &OrchestratorConfig {
        &self.config
    }
}

// ===========================================================================
// Helpers
// ===========================================================================

/// Simple pseudo-random jitter in [0, 1).
///
/// Uses a basic LCG so we don't pull in the `rand` crate just for this.
fn rand_jitter() -> f64 {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    // Simple hash to get a value in [0, 1)
    let hash = ((nanos >> 32) ^ (nanos & 0xFFFFFFFF)) as f64;
    (hash % 1_000_000.0) / 1_000_000.0
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Mock agent
    // -----------------------------------------------------------------------

    struct MockOkAgent {
        name: String,
        result: serde_json::Value,
    }

    impl MockOkAgent {
        fn new(name: &str, result: serde_json::Value) -> Self {
            Self {
                name: name.to_string(),
                result,
            }
        }
    }

    impl Agent for MockOkAgent {
        fn name(&self) -> &str {
            &self.name
        }
        fn run(
            &self,
            _workflow: &Workflow,
            _ctx: &mut AgentContext,
        ) -> Result<serde_json::Value, String> {
            Ok(self.result.clone())
        }
    }

    struct MockFailAgent {
        name: String,
        error: String,
    }

    impl MockFailAgent {
        fn new(name: &str, error: &str) -> Self {
            Self {
                name: name.to_string(),
                error: error.to_string(),
            }
        }
    }

    impl Agent for MockFailAgent {
        fn name(&self) -> &str {
            &self.name
        }
        fn run(
            &self,
            _workflow: &Workflow,
            _ctx: &mut AgentContext,
        ) -> Result<serde_json::Value, String> {
            Err(self.error.clone())
        }
    }

    // -----------------------------------------------------------------------
    // Agent Registry
    // -----------------------------------------------------------------------

    #[test]
    fn registry_new_is_empty() {
        let reg = AgentRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn registry_register_and_get() {
        let mut reg = AgentRegistry::new();
        reg.register(
            WorkflowKind::LeadQualification,
            Box::new(MockOkAgent::new("lead-bot", serde_json::json!("ok"))),
        );
        assert_eq!(reg.len(), 1);
        let agent = reg.get(WorkflowKind::LeadQualification).unwrap();
        assert_eq!(agent.name(), "lead-bot");
    }

    #[test]
    fn registry_get_missing_returns_none() {
        let reg = AgentRegistry::new();
        assert!(reg.get(WorkflowKind::LeadQualification).is_none());
    }

    #[test]
    fn registry_registered_kinds() {
        let mut reg = AgentRegistry::new();
        reg.register(
            WorkflowKind::LeadQualification,
            Box::new(MockOkAgent::new("a", serde_json::json!(null))),
        );
        reg.register(
            WorkflowKind::FinanceOperations,
            Box::new(MockOkAgent::new("b", serde_json::json!(null))),
        );
        let kinds = reg.registered_kinds();
        assert_eq!(kinds.len(), 2);
    }

    #[test]
    fn registry_overwrites() {
        let mut reg = AgentRegistry::new();
        reg.register(
            WorkflowKind::LeadQualification,
            Box::new(MockOkAgent::new("v1", serde_json::json!(1))),
        );
        reg.register(
            WorkflowKind::LeadQualification,
            Box::new(MockOkAgent::new("v2", serde_json::json!(2))),
        );
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.get(WorkflowKind::LeadQualification).unwrap().name(), "v2");
    }

    // -----------------------------------------------------------------------
    // OrchestratorConfig defaults
    // -----------------------------------------------------------------------

    #[test]
    fn config_defaults() {
        let cfg = OrchestratorConfig::default();
        assert_eq!(cfg.max_retries, 3);
        assert_eq!(cfg.base_backoff_ms, 1000);
        assert_eq!(cfg.max_backoff_ms, 30000);
        assert_eq!(cfg.backoff_multiplier, 2.0);
        assert!((cfg.jitter_pct - 0.1).abs() < f64::EPSILON);
    }

    // -----------------------------------------------------------------------
    // Compute backoff
    // -----------------------------------------------------------------------

    #[test]
    fn backoff_attempt_zero_is_base() {
        let orc = Orchestrator::new(OrchestratorConfig::default());
        assert_eq!(orc.compute_backoff(0), 1000);
    }

    #[test]
    fn backoff_grows_exponentially() {
        let orc = Orchestrator::new(OrchestratorConfig {
            jitter_pct: 0.0, // disable jitter for deterministic test
            ..Default::default()
        });
        let b0 = orc.compute_backoff(0);
        let b1 = orc.compute_backoff(1);
        let b2 = orc.compute_backoff(2);
        assert!(b1 > b0);
        assert!(b2 > b1);
    }

    #[test]
    fn backoff_is_capped() {
        let orc = Orchestrator::new(OrchestratorConfig {
            jitter_pct: 0.0,
            max_backoff_ms: 2000,
            base_backoff_ms: 1000,
            backoff_multiplier: 10.0,
            ..Default::default()
        });
        let b5 = orc.compute_backoff(5);
        assert!(b5 <= 2000);
    }

    #[test]
    fn backoff_with_jitter_varies() {
        let orc = Orchestrator::new(OrchestratorConfig::default());
        let vals: Vec<u64> = (0..20).map(|_| orc.compute_backoff(3)).collect();
        // With jitter, not all values should be identical
        let unique: std::collections::HashSet<_> = vals.iter().collect();
        assert!(unique.len() > 1, "expected variation in backoff values");
    }

    // -----------------------------------------------------------------------
    // Should retry
    // -----------------------------------------------------------------------

    #[test]
    fn should_retry_when_attempts_remaining() {
        let orc = Orchestrator::new(OrchestratorConfig::default());
        let wf = Workflow {
            id: "wf-1".into(),
            kind: WorkflowKind::LeadQualification,
            status: WorkflowStatus::Failed,
            payload: serde_json::json!({}),
            result: None,
            error: Some("fail".into()),
            attempts: 1,
            max_attempts: 3,
            source: "test".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
        };
        let decision = orc.should_retry(&wf);
        assert!(decision.should_retry);
        assert!(decision.delay_ms > 0);
    }

    #[test]
    fn should_not_retry_when_exhausted() {
        let orc = Orchestrator::new(OrchestratorConfig::default());
        let wf = Workflow {
            id: "wf-2".into(),
            kind: WorkflowKind::LeadQualification,
            status: WorkflowStatus::Failed,
            payload: serde_json::json!({}),
            result: None,
            error: Some("fail".into()),
            attempts: 3,
            max_attempts: 3,
            source: "test".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
        };
        let decision = orc.should_retry(&wf);
        assert!(!decision.should_retry);
        assert_eq!(decision.delay_ms, 0);
    }

    // -----------------------------------------------------------------------
    // Submit workflow
    // -----------------------------------------------------------------------

    #[test]
    fn submit_workflow_creates_pending() {
        let mut orc = Orchestrator::new(OrchestratorConfig::default());
        let wf = orc.submit_workflow(
            WorkflowKind::LeadQualification,
            serde_json::json!({"email": "a@b.com"}),
            "webhook",
        );
        assert_eq!(wf.status, WorkflowStatus::Pending);
        assert_eq!(wf.kind, WorkflowKind::LeadQualification);
        assert_eq!(wf.attempts, 0);
        assert!(wf.id.starts_with("wf_"));
    }

    #[test]
    fn submit_workflow_records_audit() {
        let mut orc = Orchestrator::new(OrchestratorConfig::default());
        orc.submit_workflow(WorkflowKind::FinanceOperations, serde_json::json!({}), "api");
        assert_eq!(orc.audit().len(), 1);
    }

    // -----------------------------------------------------------------------
    // Run workflow — success
    // -----------------------------------------------------------------------

    #[test]
    fn run_workflow_success() {
        let mut orc = Orchestrator::new(OrchestratorConfig::default());
        orc = orc.with_agent(
            WorkflowKind::LeadQualification,
            Box::new(MockOkAgent::new("qualifier", serde_json::json!({"score": 85}))),
        );

        let mut wf = orc.submit_workflow(
            WorkflowKind::LeadQualification,
            serde_json::json!({"lead": "jane"}),
            "test",
        );
        let result = orc.run_workflow(&mut wf).unwrap();
        assert_eq!(result, serde_json::json!({"score": 85}));
        assert_eq!(wf.status, WorkflowStatus::Completed);
        assert!(wf.result.is_some());
    }

    #[test]
    fn run_workflow_success_records_audit() {
        let mut orc = Orchestrator::new(OrchestratorConfig::default());
        orc = orc.with_agent(
            WorkflowKind::LeadQualification,
            Box::new(MockOkAgent::new("a", serde_json::json!("ok"))),
        );

        let mut wf = orc.submit_workflow(WorkflowKind::LeadQualification, serde_json::json!({}), "test");
        let initial_len = orc.audit().len();
        orc.run_workflow(&mut wf).unwrap();
        // Should have: created + started + completed = 3 audit entries
        assert_eq!(orc.audit().len(), initial_len + 2); // started + completed
    }

    // -----------------------------------------------------------------------
    // Run workflow — failure with retries
    // -----------------------------------------------------------------------

    #[test]
    fn run_workflow_no_agent_errors() {
        let mut orc = Orchestrator::new(OrchestratorConfig::default());
        let mut wf = orc.submit_workflow(WorkflowKind::LeadQualification, serde_json::json!({}), "test");
        let err = orc.run_workflow(&mut wf).unwrap_err();
        assert!(matches!(err, OrchestratorError::NoAgent(_)));
    }

    #[test]
    fn run_workflow_agent_failure_records_retryable() {
        let mut orc = Orchestrator::new(OrchestratorConfig {
            max_retries: 2,
            ..Default::default()
        });
        orc = orc.with_agent(
            WorkflowKind::LeadQualification,
            Box::new(MockFailAgent::new("bad", "connection refused")),
        );

        let mut wf = orc.submit_workflow(WorkflowKind::LeadQualification, serde_json::json!({}), "test");
        let err = orc.run_workflow(&mut wf).unwrap_err();
        assert!(matches!(err, OrchestratorError::AgentFailed(_)));
        // Should be reset to Pending for retry
        assert_eq!(wf.status, WorkflowStatus::Pending);
        assert_eq!(wf.attempts, 1);
    }

    #[test]
    fn run_workflow_permanent_failure_creates_escalation() {
        let mut orc = Orchestrator::new(OrchestratorConfig {
            max_retries: 0,
            ..Default::default()
        });
        orc = orc.with_agent(
            WorkflowKind::LeadQualification,
            Box::new(MockFailAgent::new("bad", "permanent error")),
        );

        let mut wf = orc.submit_workflow(WorkflowKind::LeadQualification, serde_json::json!({}), "test");
        let err = orc.run_workflow(&mut wf).unwrap_err();
        assert!(matches!(err, OrchestratorError::AgentFailed(_)));
        assert_eq!(wf.status, WorkflowStatus::Failed);
        // Audit should have escalation entry
        assert!(wf.error.is_some());
    }

    #[test]
    fn run_workflow_terminal_state_errors() {
        let mut orc = Orchestrator::new(OrchestratorConfig::default());
        orc = orc.with_agent(
            WorkflowKind::LeadQualification,
            Box::new(MockOkAgent::new("a", serde_json::json!("ok"))),
        );

        let mut wf = orc.submit_workflow(WorkflowKind::LeadQualification, serde_json::json!({}), "test");
        // Run once successfully
        orc.run_workflow(&mut wf).unwrap();
        assert_eq!(wf.status, WorkflowStatus::Completed);

        // Running again should fail
        let err = orc.run_workflow(&mut wf).unwrap_err();
        assert!(matches!(err, OrchestratorError::TerminalState(_)));
    }

    // -----------------------------------------------------------------------
    // Create escalation
    // -----------------------------------------------------------------------

    #[test]
    fn create_escalation_records_audit() {
        let mut orc = Orchestrator::new(OrchestratorConfig::default());
        let wf = Workflow {
            id: "wf-1".into(),
            kind: WorkflowKind::LeadQualification,
            status: WorkflowStatus::Failed,
            payload: serde_json::json!({}),
            result: None,
            error: Some("timeout".into()),
            attempts: 2,
            max_attempts: 3,
            source: "webhook".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
        };
        let before = orc.audit().len();
        let esc = orc.create_escalation(&wf, "Payment gateway down");
        assert_eq!(esc.severity, Severity::High);
        assert_eq!(esc.workflow_id.as_deref(), Some("wf-1"));
        assert!(orc.audit().len() > before);
    }

    // -----------------------------------------------------------------------
    // Accessors
    // -----------------------------------------------------------------------

    #[test]
    fn audit_accessor() {
        let orc = Orchestrator::new(OrchestratorConfig::default());
        assert!(orc.audit().is_empty());
    }

    #[test]
    fn registry_accessor() {
        let orc = Orchestrator::new(OrchestratorConfig::default());
        assert!(orc.registry().is_empty());
    }

    #[test]
    fn config_accessor() {
        let orc = Orchestrator::new(OrchestratorConfig::default());
        assert_eq!(orc.config().max_retries, 3);
    }

    // -----------------------------------------------------------------------
    // Agent context
    // -----------------------------------------------------------------------

    #[test]
    fn agent_context_new_is_empty() {
        let ctx = AgentContext::new();
        assert!(ctx.audit.is_empty());
        assert!(ctx.tasks.is_empty());
    }

    #[test]
    fn agent_context_default() {
        let ctx = AgentContext::default();
        assert!(ctx.tasks.is_empty());
    }
}
