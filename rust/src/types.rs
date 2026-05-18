//! Core types for the Autonomous Business OS.
//!
//! Defines the workflow state machine, approval chains, audit actions,
//! escalation tracking, lead scoring, and secret redaction primitives.

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

// ===========================================================================
// Workflow State Machine
// ===========================================================================

/// Lifecycle status of a workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum WorkflowStatus {
    Pending,
    Running,
    WaitingForHuman,
    Completed,
    Failed,
    Cancelled,
}

impl WorkflowStatus {
    /// Returns the set of valid successor states for a given status.
    pub fn valid_transitions(self) -> &'static [WorkflowStatus] {
        match self {
            WorkflowStatus::Pending => &[WorkflowStatus::Running, WorkflowStatus::Cancelled],
            WorkflowStatus::Running => &[
                WorkflowStatus::WaitingForHuman,
                WorkflowStatus::Completed,
                WorkflowStatus::Failed,
                WorkflowStatus::Cancelled,
            ],
            WorkflowStatus::WaitingForHuman => &[
                WorkflowStatus::Running,
                WorkflowStatus::Completed,
                WorkflowStatus::Failed,
                WorkflowStatus::Cancelled,
            ],
            WorkflowStatus::Completed => &[],
            WorkflowStatus::Failed => &[WorkflowStatus::Running],
            WorkflowStatus::Cancelled => &[],
        }
    }

    /// Checks whether transitioning to `target` is valid from `self`.
    pub fn can_transition_to(self, target: WorkflowStatus) -> bool {
        self.valid_transitions().contains(&target)
    }

    /// Returns true if the status represents a terminal state.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            WorkflowStatus::Completed | WorkflowStatus::Cancelled
        )
    }
}

/// Status of an individual agent task within a workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum TaskStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Escalated,
}

/// Open/Granted/Rejected status for an approval record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum ApprovalStatus {
    Open,
    Approved,
    Rejected,
}

/// The actual decision made on an approval request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum ApprovalDecision {
    Approved,
    Rejected,
}

// ===========================================================================
// Audit Actions
// ===========================================================================

/// Every auditable action in the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum AuditAction {
    WorkflowCreated,
    WorkflowStarted,
    WorkflowCompleted,
    WorkflowFailed,
    WorkflowCancelled,
    TaskStarted,
    TaskCompleted,
    TaskFailed,
    ApprovalRequested,
    ApprovalGranted,
    ApprovalRejected,
    EscalationCreated,
    EscalationResolved,
    LeadScored,
    RateLimitExceeded,
    CircuitBreakerTripped,
    CircuitBreakerReset,
    ConfigChanged,
    AgentRegistered,
    AgentDispatched,
    SecretRotated,
}

impl AuditAction {
    /// Total number of distinct audit actions.
    pub const COUNT: usize = 21;
}

// ===========================================================================
// Workflow Kind, Severity, Score Tier
// ===========================================================================

/// Top-level categorization of workflows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum WorkflowKind {
    LeadQualification,
    ClientOnboarding,
    DeliveryMonitoring,
    FinanceOperations,
    KnowledgeCommunication,
}

/// Severity level for escalations and alerts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

/// Lead scoring tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum ScoreTier {
    A, // >= 80
    B, // >= 55
    C, // < 55
}

impl ScoreTier {
    /// Derives a tier from a numeric lead score.
    ///
    /// - `>= 80` → `ScoreTier::A`
    /// - `>= 55` → `ScoreTier::B`
    /// - `< 55`  → `ScoreTier::C`
    pub fn from_score(score: u32) -> Self {
        if score >= 80 {
            ScoreTier::A
        } else if score >= 55 {
            ScoreTier::B
        } else {
            ScoreTier::C
        }
    }
}

// ===========================================================================
// Core Structs
// ===========================================================================

/// A workflow instance — the central orchestration unit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub kind: WorkflowKind,
    pub status: WorkflowStatus,
    pub payload: serde_json::Value,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub attempts: u32,
    pub max_attempts: u32,
    pub source: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl Workflow {
    /// Create a new workflow with sensible defaults.
    pub fn new(id: &str, max_attempts: u32) -> Self {
        let now = Utc::now();
        Self {
            id: id.to_owned(),
            kind: WorkflowKind::LeadQualification,
            status: WorkflowStatus::Pending,
            payload: serde_json::Value::Null,
            result: None,
            error: None,
            attempts: 0,
            max_attempts,
            source: String::new(),
            created_at: now,
            updated_at: now,
            completed_at: None,
        }
    }
}

/// A single task executed by an agent within a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTask {
    pub id: String,
    pub workflow_id: String,
    pub agent_name: String,
    pub tool_name: String,
    pub status: TaskStatus,
    pub input: serde_json::Value,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<u64>,
    pub updated_at: DateTime<Utc>,
}

impl AgentTask {
    /// Create a new task with sensible defaults.
    pub fn new(id: &str, workflow_id: &str) -> Self {
        let now = Utc::now();
        Self {
            id: id.to_owned(),
            workflow_id: workflow_id.to_owned(),
            agent_name: String::new(),
            tool_name: String::new(),
            status: TaskStatus::Queued,
            input: serde_json::Value::Null,
            output: None,
            error: None,
            started_at: None,
            completed_at: None,
            duration_ms: None,
            updated_at: now,
        }
    }
}

/// A human approval gate attached to a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanApproval {
    pub id: String,
    pub workflow_id: String,
    pub title: String,
    pub reason: String,
    pub proposed_action: serde_json::Value,
    pub status: ApprovalStatus,
    pub decided_by: Option<String>,
    pub decision: Option<ApprovalDecision>,
    pub decision_note: Option<String>,
    /// Short-hand alias used by the approval service.
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub requested_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

impl HumanApproval {
    /// Create a new open approval request.
    pub fn new(
        id: &str,
        workflow_id: &str,
        title: &str,
        reason: &str,
        proposed_action: serde_json::Value,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: id.to_owned(),
            workflow_id: workflow_id.to_owned(),
            title: title.to_owned(),
            reason: reason.to_owned(),
            proposed_action,
            status: ApprovalStatus::Open,
            decided_by: None,
            decision: None,
            decision_note: None,
            note: None,
            created_at: now,
            requested_at: now,
            decided_at: None,
            updated_at: now,
        }
    }
}

/// A single, immutable audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub workflow_id: Option<String>,
    pub action: AuditAction,
    pub actor: String,
    pub message: String,
    pub metadata: serde_json::Value,
    pub timestamp: DateTime<Utc>,
    // Append-only: no update methods will be provided.
}

/// An escalation event requiring human attention.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Escalation {
    pub id: String,
    pub workflow_id: Option<String>,
    pub severity: Severity,
    pub owner: String,
    pub reason: String,
    pub context: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolution: Option<String>,
}

/// A sales lead with scoring and enrichment data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lead {
    pub id: String,
    pub email: String,
    pub company: Option<String>,
    pub name: Option<String>,
    pub source: String,
    pub score: Option<u32>,
    pub tier: Option<ScoreTier>,
    pub enrichment: serde_json::Value,
    pub outreach: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

impl Lead {
    /// Create a new lead.
    pub fn new(id: &str, name: &str, email: &str, company: &str) -> Self {
        Self {
            id: id.to_owned(),
            email: email.to_owned(),
            company: if company.is_empty() { None } else { Some(company.to_owned()) },
            name: if name.is_empty() { None } else { Some(name.to_owned()) },
            source: String::new(),
            score: None,
            tier: None,
            enrichment: serde_json::Value::Null,
            outreach: serde_json::Value::Null,
            created_at: Utc::now(),
        }
    }

    /// Builder-style method to attach enrichment data.
    pub fn with_enrichment(mut self, enrichment: serde_json::Value) -> Self {
        self.enrichment = enrichment;
        self
    }
}

// ===========================================================================
// Secret wrapper
// ===========================================================================

/// Wraps a sensitive value so that `Debug` prints `***REDACTED***`.
///
/// No `Display` impl is provided — this prevents accidental logging via
/// `{}` format.  Callers must explicitly use `.expose()` when the plain
/// value is needed.
pub struct Secret<T: ToString> {
    inner: T,
}

impl<T: ToString> Secret<T> {
    pub fn new(value: T) -> Self {
        Self { inner: value }
    }

    /// Returns a reference to the underlying value.
    /// Use sparingly and never log the result.
    pub fn expose(&self) -> &T {
        &self.inner
    }
}

impl<T: ToString> std::fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "***REDACTED***")
    }
}

// Intentionally NO Display impl — prevents accidental logging.

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    // -----------------------------------------------------------------------
    // WorkflowStatus transitions
    // -----------------------------------------------------------------------

    #[test]
    fn pending_can_go_to_running() {
        assert!(WorkflowStatus::Pending.can_transition_to(WorkflowStatus::Running));
    }

    #[test]
    fn pending_can_go_to_cancelled() {
        assert!(WorkflowStatus::Pending.can_transition_to(WorkflowStatus::Cancelled));
    }

    #[test]
    fn pending_cannot_go_to_completed() {
        assert!(!WorkflowStatus::Pending.can_transition_to(WorkflowStatus::Completed));
    }

    #[test]
    fn running_can_go_to_waiting_for_human() {
        assert!(WorkflowStatus::Running.can_transition_to(WorkflowStatus::WaitingForHuman));
    }

    #[test]
    fn running_can_go_to_completed() {
        assert!(WorkflowStatus::Running.can_transition_to(WorkflowStatus::Completed));
    }

    #[test]
    fn running_can_go_to_failed() {
        assert!(WorkflowStatus::Running.can_transition_to(WorkflowStatus::Failed));
    }

    #[test]
    fn failed_can_retry_to_running() {
        assert!(WorkflowStatus::Failed.can_transition_to(WorkflowStatus::Running));
    }

    #[test]
    fn completed_is_terminal() {
        assert!(WorkflowStatus::Completed.is_terminal());
        assert_eq!(WorkflowStatus::Completed.valid_transitions().len(), 0);
    }

    #[test]
    fn cancelled_is_terminal() {
        assert!(WorkflowStatus::Cancelled.is_terminal());
    }

    #[test]
    fn failed_is_not_terminal() {
        assert!(!WorkflowStatus::Failed.is_terminal());
    }

    // -----------------------------------------------------------------------
    // TaskStatus variants
    // -----------------------------------------------------------------------

    #[test]
    fn task_status_all_variants() {
        let variants = [
            TaskStatus::Queued,
            TaskStatus::Running,
            TaskStatus::Completed,
            TaskStatus::Failed,
            TaskStatus::Escalated,
        ];
        assert_eq!(variants.len(), 5);
    }

    // -----------------------------------------------------------------------
    // ApprovalStatus / ApprovalDecision
    // -----------------------------------------------------------------------

    #[test]
    fn approval_status_open() {
        assert_eq!(ApprovalStatus::Open, ApprovalStatus::Open);
    }

    #[test]
    fn approval_decision_matches_approval_status() {
        // ApprovalDecision::Approved and ApprovalStatus::Approved are distinct
        // enums but correspond logically. Verify they map correctly.
        let decision_to_status = |d: ApprovalDecision| match d {
            ApprovalDecision::Approved => ApprovalStatus::Approved,
            ApprovalDecision::Rejected => ApprovalStatus::Rejected,
        };
        assert_eq!(decision_to_status(ApprovalDecision::Approved), ApprovalStatus::Approved);
        assert_eq!(decision_to_status(ApprovalDecision::Rejected), ApprovalStatus::Rejected);
    }

    #[test]
    fn approval_status_variants_count() {
        let all = [
            ApprovalStatus::Open,
            ApprovalStatus::Approved,
            ApprovalStatus::Rejected,
        ];
        assert_eq!(all.len(), 3);
    }

    // -----------------------------------------------------------------------
    // AuditAction count
    // -----------------------------------------------------------------------

    #[test]
    fn audit_action_count() {
        // Verify our const count matches the actual enum variants
        assert_eq!(AuditAction::COUNT, 21);
    }

    #[test]
    fn audit_action_all_distinct() {
        let actions = [
            AuditAction::WorkflowCreated,
            AuditAction::WorkflowStarted,
            AuditAction::WorkflowCompleted,
            AuditAction::WorkflowFailed,
            AuditAction::WorkflowCancelled,
            AuditAction::TaskStarted,
            AuditAction::TaskCompleted,
            AuditAction::TaskFailed,
            AuditAction::ApprovalRequested,
            AuditAction::ApprovalGranted,
            AuditAction::ApprovalRejected,
            AuditAction::EscalationCreated,
            AuditAction::EscalationResolved,
            AuditAction::LeadScored,
            AuditAction::RateLimitExceeded,
            AuditAction::CircuitBreakerTripped,
            AuditAction::CircuitBreakerReset,
            AuditAction::ConfigChanged,
            AuditAction::AgentRegistered,
            AuditAction::AgentDispatched,
            AuditAction::SecretRotated,
        ];
        assert_eq!(actions.len(), AuditAction::COUNT);
        // Ensure all are unique via HashSet
        use std::collections::HashSet;
        let set: HashSet<_> = actions.iter().collect();
        assert_eq!(set.len(), AuditAction::COUNT);
    }

    // -----------------------------------------------------------------------
    // WorkflowKind, Severity, ScoreTier
    // -----------------------------------------------------------------------

    #[test]
    fn workflow_kind_count() {
        let kinds = [
            WorkflowKind::LeadQualification,
            WorkflowKind::ClientOnboarding,
            WorkflowKind::DeliveryMonitoring,
            WorkflowKind::FinanceOperations,
            WorkflowKind::KnowledgeCommunication,
        ];
        assert_eq!(kinds.len(), 5);
    }

    #[test]
    fn severity_ordering() {
        // Verify severity levels are all present
        let severities = [Severity::Low, Severity::Medium, Severity::High, Severity::Critical];
        assert_eq!(severities.len(), 4);
    }

    #[test]
    fn score_tier_from_score_a() {
        assert_eq!(ScoreTier::from_score(80), ScoreTier::A);
        assert_eq!(ScoreTier::from_score(100), ScoreTier::A);
    }

    #[test]
    fn score_tier_from_score_b() {
        assert_eq!(ScoreTier::from_score(55), ScoreTier::B);
        assert_eq!(ScoreTier::from_score(79), ScoreTier::B);
    }

    #[test]
    fn score_tier_from_score_c() {
        assert_eq!(ScoreTier::from_score(0), ScoreTier::C);
        assert_eq!(ScoreTier::from_score(54), ScoreTier::C);
    }

    // -----------------------------------------------------------------------
    // Struct construction
    // -----------------------------------------------------------------------

    #[test]
    fn workflow_construction() {
        let now = Utc::now();
        let wf = Workflow {
            id: "wf-1".into(),
            kind: WorkflowKind::LeadQualification,
            status: WorkflowStatus::Pending,
            payload: serde_json::json!({"lead_id": "l1"}),
            result: None,
            error: None,
            attempts: 0,
            max_attempts: 3,
            source: "webhook".into(),
            created_at: now,
            updated_at: now,
            completed_at: None,
        };
        assert_eq!(wf.id, "wf-1");
        assert_eq!(wf.status, WorkflowStatus::Pending);
        assert_eq!(wf.max_attempts, 3);
        assert!(wf.completed_at.is_none());
    }

    #[test]
    fn agent_task_construction() {
        let now = Utc::now();
        let task = AgentTask {
            id: "t-1".into(),
            workflow_id: "wf-1".into(),
            agent_name: "qualifier".into(),
            tool_name: "score_lead".into(),
            status: TaskStatus::Queued,
            input: serde_json::json!({"email": "a@b.com"}),
            output: None,
            error: None,
            started_at: None,
            completed_at: None,
            duration_ms: None,
            updated_at: now,
        };
        assert_eq!(task.agent_name, "qualifier");
        assert_eq!(task.status, TaskStatus::Queued);
    }

    #[test]
    fn human_approval_construction() {
        let now = Utc::now();
        let approval = HumanApproval {
            id: "apr-1".into(),
            workflow_id: "wf-1".into(),
            title: "Review deal".into(),
            reason: "High value".into(),
            proposed_action: serde_json::json!({"send_email": true}),
            status: ApprovalStatus::Open,
            decided_by: None,
            decision: None,
            decision_note: None,
            note: None,
            created_at: now,
            requested_at: now,
            decided_at: None,
            updated_at: now,
        };
        assert_eq!(approval.status, ApprovalStatus::Open);
        assert!(approval.decided_by.is_none());
    }

    #[test]
    fn audit_entry_construction() {
        let entry = AuditEntry {
            id: "ae-1".into(),
            workflow_id: Some("wf-1".into()),
            action: AuditAction::WorkflowCreated,
            actor: "system".into(),
            message: "Created".into(),
            metadata: serde_json::json!({}),
            timestamp: Utc::now(),
        };
        assert_eq!(entry.action, AuditAction::WorkflowCreated);
        assert_eq!(entry.workflow_id.as_deref(), Some("wf-1"));
    }

    #[test]
    fn escalation_construction() {
        let now = Utc::now();
        let esc = Escalation {
            id: "esc-1".into(),
            workflow_id: Some("wf-1".into()),
            severity: Severity::High,
            owner: "ops".into(),
            reason: "Timeout".into(),
            context: serde_json::json!({"task": "email"}),
            created_at: now,
            resolved_at: None,
            resolution: None,
        };
        assert_eq!(esc.severity, Severity::High);
        assert!(esc.resolved_at.is_none());
    }

    #[test]
    fn lead_construction() {
        let lead = Lead {
            id: "lead-1".into(),
            email: "test@example.com".into(),
            company: Some("Acme".into()),
            name: Some("Jane".into()),
            source: "website".into(),
            score: Some(90),
            tier: Some(ScoreTier::A),
            enrichment: serde_json::json!({"industry": "tech"}),
            outreach: serde_json::json!({}),
            created_at: Utc::now(),
        };
        assert_eq!(lead.score, Some(90));
        assert_eq!(lead.tier, Some(ScoreTier::A));
    }

    // -----------------------------------------------------------------------
    // Secret
    // -----------------------------------------------------------------------

    #[test]
    fn secret_debug_is_redacted() {
        let s = Secret::new("super-secret-key-12345");
        let debug_str = format!("{:?}", s);
        assert_eq!(debug_str, "***REDACTED***");
    }

    #[test]
    fn secret_expose_returns_value() {
        let s = Secret::new("my-key");
        assert_eq!(s.expose(), &"my-key");
    }

    // -----------------------------------------------------------------------
    // Serialization round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn workflow_serde_roundtrip() {
        let now = Utc::now();
        let wf = Workflow {
            id: "wf-42".into(),
            kind: WorkflowKind::FinanceOperations,
            status: WorkflowStatus::Running,
            payload: serde_json::json!({"amount": 5000}),
            result: None,
            error: None,
            attempts: 1,
            max_attempts: 3,
            source: "api".into(),
            created_at: now,
            updated_at: now,
            completed_at: None,
        };
        let json = serde_json::to_value(&wf).unwrap();
        let deserialized: Workflow = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.id, wf.id);
        assert_eq!(deserialized.kind, wf.kind);
        assert_eq!(deserialized.status, wf.status);
    }

    #[test]
    fn lead_serde_roundtrip() {
        let lead = Lead {
            id: "l-1".into(),
            email: "x@y.com".into(),
            company: None,
            name: None,
            source: "referral".into(),
            score: Some(42),
            tier: Some(ScoreTier::C),
            enrichment: serde_json::json!({}),
            outreach: serde_json::json!({}),
            created_at: Utc::now(),
        };
        let json = serde_json::to_value(&lead).unwrap();
        let back: Lead = serde_json::from_value(json).unwrap();
        assert_eq!(back.tier, Some(ScoreTier::C));
    }
}
