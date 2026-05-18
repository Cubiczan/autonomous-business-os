//! Append-only audit trail for the Autonomous Business OS.
//!
//! The `AuditService` records every significant system event.  Entries are
//! immutable once written — there is no `update` or `delete` API.  In
//! production the backing store would be a database or an append-only log
//! file; this in-memory implementation is suitable for testing and single-
//! process deployments.

use chrono::Utc;
use serde_json;
use uuid::Uuid;

use crate::types::{
    AuditAction, AuditEntry, Severity, WorkflowKind,
};

// ===========================================================================
// Query
// ===========================================================================

/// Filter parameters for querying the audit trail.
///
/// All fields are optional — omit a field to skip that filter.
#[derive(Debug, Clone, Default)]
pub struct AuditQuery {
    pub workflow_id: Option<String>,
    pub action: Option<AuditAction>,
    pub actor: Option<String>,
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    pub until: Option<chrono::DateTime<chrono::Utc>>,
    pub limit: usize,
}

impl AuditQuery {
    /// Creates a new query with a default limit of 100.
    pub fn new() -> Self {
        Self {
            limit: 100,
            ..Default::default()
        }
    }

    /// Sets the workflow_id filter.
    pub fn workflow_id(mut self, id: impl Into<String>) -> Self {
        self.workflow_id = Some(id.into());
        self
    }

    /// Sets the action filter.
    pub fn action(mut self, action: AuditAction) -> Self {
        self.action = Some(action);
        self
    }

    /// Sets the actor filter.
    pub fn actor(mut self, actor: impl Into<String>) -> Self {
        self.actor = Some(actor.into());
        self
    }

    /// Sets the lower-bound timestamp filter.
    pub fn since(mut self, ts: chrono::DateTime<chrono::Utc>) -> Self {
        self.since = Some(ts);
        self
    }

    /// Sets the upper-bound timestamp filter.
    pub fn until(mut self, ts: chrono::DateTime<chrono::Utc>) -> Self {
        self.until = Some(ts);
        self
    }

    /// Sets the maximum number of results.
    pub fn limit(mut self, n: usize) -> Self {
        self.limit = n;
        self
    }
}

// ===========================================================================
// AuditService
// ===========================================================================

/// In-memory, append-only audit log.
///
/// # Immutability guarantee
///
/// No method exists to modify or delete an entry once it has been appended.
/// The `entries` field is exposed as `&[AuditEntry]` only through query
/// results; direct mutation is not possible outside this module.
pub struct AuditService {
    entries: Vec<AuditEntry>,
}

impl AuditService {
    /// Creates an empty audit service.
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    // -------------------------------------------------------------------
    // Generic record
    // -------------------------------------------------------------------

    /// Appends a new audit entry and returns a reference to it.
    ///
    /// This is the **only** way to add entries — there is no update or
    /// delete method.
    pub fn record(
        &mut self,
        action: AuditAction,
        actor: &str,
        message: &str,
        workflow_id: Option<&str>,
        metadata: serde_json::Value,
    ) -> &AuditEntry {
        let entry = AuditEntry {
            id: Uuid::new_v4().to_string(),
            workflow_id: workflow_id.map(|s| s.to_owned()),
            action,
            actor: actor.to_owned(),
            message: message.to_owned(),
            metadata,
            timestamp: Utc::now(),
        };
        self.entries.push(entry);
        self.entries.last().unwrap()
    }

    // -------------------------------------------------------------------
    // Workflow lifecycle helpers
    // -------------------------------------------------------------------

    /// Records that a workflow was created.
    pub fn record_workflow_created(
        &mut self,
        workflow_id: &str,
        kind: WorkflowKind,
        source: &str,
    ) -> &AuditEntry {
        self.record(
            AuditAction::WorkflowCreated,
            "system",
            &format!("Workflow {} created (kind={:?}, source={})", workflow_id, kind, source),
            Some(workflow_id),
            serde_json::json!({
                "kind": format!("{:?}", kind),
                "source": source,
            }),
        )
    }

    /// Records that a workflow was started.
    pub fn record_workflow_started(&mut self, workflow_id: &str) -> &AuditEntry {
        self.record(
            AuditAction::WorkflowStarted,
            "system",
            &format!("Workflow {} started", workflow_id),
            Some(workflow_id),
            serde_json::json!({}),
        )
    }

    /// Records that a workflow completed successfully.
    pub fn record_workflow_completed(
        &mut self,
        workflow_id: &str,
        result: &serde_json::Value,
    ) -> &AuditEntry {
        self.record(
            AuditAction::WorkflowCompleted,
            "system",
            &format!("Workflow {} completed", workflow_id),
            Some(workflow_id),
            serde_json::json!({ "result": result }),
        )
    }

    /// Records that a workflow failed.
    pub fn record_workflow_failed(&mut self, workflow_id: &str, error: &str) -> &AuditEntry {
        self.record(
            AuditAction::WorkflowFailed,
            "system",
            &format!("Workflow {} failed: {}", workflow_id, error),
            Some(workflow_id),
            serde_json::json!({ "error": error }),
        )
    }

    // -------------------------------------------------------------------
    // Task lifecycle helpers
    // -------------------------------------------------------------------

    /// Records that a task was started.
    pub fn record_task_started(
        &mut self,
        workflow_id: &str,
        task_id: &str,
        agent: &str,
        tool: &str,
    ) -> &AuditEntry {
        self.record(
            AuditAction::TaskStarted,
            agent,
            &format!("Task {} started (agent={}, tool={})", task_id, agent, tool),
            Some(workflow_id),
            serde_json::json!({
                "task_id": task_id,
                "agent": agent,
                "tool": tool,
            }),
        )
    }

    /// Records that a task completed.
    pub fn record_task_completed(&mut self, workflow_id: &str, task_id: &str) -> &AuditEntry {
        self.record(
            AuditAction::TaskCompleted,
            "system",
            &format!("Task {} completed", task_id),
            Some(workflow_id),
            serde_json::json!({ "task_id": task_id }),
        )
    }

    /// Records that a task failed.
    pub fn record_task_failed(
        &mut self,
        workflow_id: &str,
        task_id: &str,
        error: &str,
    ) -> &AuditEntry {
        self.record(
            AuditAction::TaskFailed,
            "system",
            &format!("Task {} failed: {}", task_id, error),
            Some(workflow_id),
            serde_json::json!({
                "task_id": task_id,
                "error": error,
            }),
        )
    }

    // -------------------------------------------------------------------
    // Approval helpers
    // -------------------------------------------------------------------

    /// Records that an approval was requested.
    pub fn record_approval_requested(&mut self, workflow_id: &str, title: &str) -> &AuditEntry {
        self.record(
            AuditAction::ApprovalRequested,
            "system",
            &format!("Approval requested: {}", title),
            Some(workflow_id),
            serde_json::json!({ "title": title }),
        )
    }

    /// Records that an approval was granted.
    pub fn record_approval_granted(&mut self, workflow_id: &str, decided_by: &str) -> &AuditEntry {
        self.record(
            AuditAction::ApprovalGranted,
            decided_by,
            &format!("Approval granted for workflow {} by {}", workflow_id, decided_by),
            Some(workflow_id),
            serde_json::json!({ "decided_by": decided_by }),
        )
    }

    /// Records that an approval was rejected.
    pub fn record_approval_rejected(
        &mut self,
        workflow_id: &str,
        decided_by: &str,
        note: &str,
    ) -> &AuditEntry {
        self.record(
            AuditAction::ApprovalRejected,
            decided_by,
            &format!(
                "Approval rejected for workflow {} by {}: {}",
                workflow_id, decided_by, note
            ),
            Some(workflow_id),
            serde_json::json!({
                "decided_by": decided_by,
                "note": note,
            }),
        )
    }

    // -------------------------------------------------------------------
    // Escalation helpers
    // -------------------------------------------------------------------

    /// Records that an escalation was created.
    pub fn record_escalation(
        &mut self,
        workflow_id: Option<&str>,
        severity: Severity,
        reason: &str,
    ) -> &AuditEntry {
        self.record(
            AuditAction::EscalationCreated,
            "system",
            &format!("Escalation created (severity={:?}): {}", severity, reason),
            workflow_id,
            serde_json::json!({
                "severity": format!("{:?}", severity),
                "reason": reason,
            }),
        )
    }

    /// Records that an escalation was resolved.
    pub fn record_escalation_resolved(
        &mut self,
        escalation_id: &str,
        resolution: &str,
    ) -> &AuditEntry {
        self.record(
            AuditAction::EscalationResolved,
            "system",
            &format!("Escalation {} resolved: {}", escalation_id, resolution),
            None,
            serde_json::json!({
                "escalation_id": escalation_id,
                "resolution": resolution,
            }),
        )
    }

    // -------------------------------------------------------------------
    // Query
    // -------------------------------------------------------------------

    /// Queries the audit trail, returning references to matching entries.
    ///
    /// Filters are combined with AND logic. Results are returned in
    /// chronological order (oldest first), up to `query.limit`.
    pub fn query(&self, query: &AuditQuery) -> Vec<&AuditEntry> {
        let mut results: Vec<&AuditEntry> = self
            .entries
            .iter()
            .filter(|e| {
                if let Some(ref wid) = query.workflow_id {
                    if e.workflow_id.as_ref() != Some(wid) {
                        return false;
                    }
                }
                if let Some(action) = query.action {
                    if e.action != action {
                        return false;
                    }
                }
                if let Some(ref actor) = query.actor {
                    if e.actor != *actor {
                        return false;
                    }
                }
                if let Some(since) = query.since {
                    if e.timestamp < since {
                        return false;
                    }
                }
                if let Some(until) = query.until {
                    if e.timestamp > until {
                        return false;
                    }
                }
                true
            })
            .collect();

        // Enforce limit
        results.truncate(query.limit);
        results
    }

    /// Convenience method: returns all entries for a specific workflow.
    pub fn entries_for_workflow(&self, workflow_id: &str) -> Vec<&AuditEntry> {
        let q = AuditQuery::new().workflow_id(workflow_id);
        self.query(&q)
    }

    /// Returns the number of entries in the audit trail.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if the audit trail is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Serializes all entries to a JSON array.
    pub fn export_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.entries).unwrap_or(serde_json::json!([]))
    }
}

impl Default for AuditService {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    // -------------------------------------------------------------------
    // Basic recording
    // -------------------------------------------------------------------

    #[test]
    fn new_service_is_empty() {
        let svc = AuditService::new();
        assert!(svc.is_empty());
        assert_eq!(svc.len(), 0);
    }

    #[test]
    fn record_generic_entry() {
        let mut svc = AuditService::new();
        let entry = svc.record(
            AuditAction::ConfigChanged,
            "admin",
            "Updated rate limit",
            None,
            serde_json::json!({"old": 100, "new": 200}),
        );
        assert_eq!(entry.action, AuditAction::ConfigChanged);
        assert_eq!(entry.actor, "admin");
        assert_eq!(svc.len(), 1);
    }

    #[test]
    fn record_generates_unique_ids() {
        let mut svc = AuditService::new();
        let id1 = svc.record(AuditAction::ConfigChanged, "a", "m", None, serde_json::json!({})).id.clone();
        let id2 = svc.record(AuditAction::ConfigChanged, "a", "m", None, serde_json::json!({})).id.clone();
        assert_ne!(id1, id2);
    }

    #[test]
    fn record_preserves_timestamp_order() {
        let mut svc = AuditService::new();
        let ts1 = svc.record(AuditAction::ConfigChanged, "a", "first", None, serde_json::json!({})).timestamp;
        let ts2 = svc.record(AuditAction::ConfigChanged, "a", "second", None, serde_json::json!({})).timestamp;
        assert!(ts2 >= ts1);
    }

    // -------------------------------------------------------------------
    // Workflow lifecycle
    // -------------------------------------------------------------------

    #[test]
    fn record_workflow_created() {
        let mut svc = AuditService::new();
        let entry = svc.record_workflow_created("wf-1", WorkflowKind::LeadQualification, "webhook");
        assert_eq!(entry.action, AuditAction::WorkflowCreated);
        assert_eq!(entry.workflow_id.as_deref(), Some("wf-1"));
        assert!(entry.metadata["source"].is_string());
    }

    #[test]
    fn record_workflow_started() {
        let mut svc = AuditService::new();
        let entry = svc.record_workflow_started("wf-1");
        assert_eq!(entry.action, AuditAction::WorkflowStarted);
    }

    #[test]
    fn record_workflow_completed() {
        let mut svc = AuditService::new();
        let result = serde_json::json!({"status": "ok"});
        let entry = svc.record_workflow_completed("wf-1", &result);
        assert_eq!(entry.action, AuditAction::WorkflowCompleted);
        assert_eq!(entry.metadata["result"], result);
    }

    #[test]
    fn record_workflow_failed() {
        let mut svc = AuditService::new();
        let entry = svc.record_workflow_failed("wf-1", "timeout");
        assert_eq!(entry.action, AuditAction::WorkflowFailed);
        assert_eq!(entry.metadata["error"], "timeout");
    }

    #[test]
    fn full_workflow_lifecycle_produces_four_entries() {
        let mut svc = AuditService::new();
        svc.record_workflow_created("wf-2", WorkflowKind::ClientOnboarding, "api");
        svc.record_workflow_started("wf-2");
        svc.record_workflow_completed("wf-2", &serde_json::json!({"done": true}));
        assert_eq!(svc.len(), 3);
    }

    // -------------------------------------------------------------------
    // Task lifecycle
    // -------------------------------------------------------------------

    #[test]
    fn record_task_started() {
        let mut svc = AuditService::new();
        let entry = svc.record_task_started("wf-1", "t-1", "emailer", "send_email");
        assert_eq!(entry.action, AuditAction::TaskStarted);
        assert_eq!(entry.actor, "emailer");
        assert_eq!(entry.metadata["tool"], "send_email");
    }

    #[test]
    fn record_task_completed() {
        let mut svc = AuditService::new();
        let entry = svc.record_task_completed("wf-1", "t-1");
        assert_eq!(entry.action, AuditAction::TaskCompleted);
    }

    #[test]
    fn record_task_failed() {
        let mut svc = AuditService::new();
        let entry = svc.record_task_failed("wf-1", "t-1", "SMTP error");
        assert_eq!(entry.action, AuditAction::TaskFailed);
        assert_eq!(entry.metadata["error"], "SMTP error");
    }

    // -------------------------------------------------------------------
    // Approval lifecycle
    // -------------------------------------------------------------------

    #[test]
    fn record_approval_requested() {
        let mut svc = AuditService::new();
        let entry = svc.record_approval_requested("wf-1", "Review contract");
        assert_eq!(entry.action, AuditAction::ApprovalRequested);
        assert_eq!(entry.metadata["title"], "Review contract");
    }

    #[test]
    fn record_approval_granted() {
        let mut svc = AuditService::new();
        let entry = svc.record_approval_granted("wf-1", "ceo@example.com");
        assert_eq!(entry.action, AuditAction::ApprovalGranted);
        assert_eq!(entry.actor, "ceo@example.com");
    }

    #[test]
    fn record_approval_rejected() {
        let mut svc = AuditService::new();
        let entry = svc.record_approval_rejected("wf-1", "cfo@example.com", "Budget exceeded");
        assert_eq!(entry.action, AuditAction::ApprovalRejected);
        assert_eq!(entry.metadata["note"], "Budget exceeded");
    }

    // -------------------------------------------------------------------
    // Escalations
    // -------------------------------------------------------------------

    #[test]
    fn record_escalation_with_workflow() {
        let mut svc = AuditService::new();
        let entry = svc.record_escalation(Some("wf-1"), Severity::High, "Payment gateway down");
        assert_eq!(entry.action, AuditAction::EscalationCreated);
        assert_eq!(entry.workflow_id.as_deref(), Some("wf-1"));
        assert_eq!(entry.metadata["severity"], "High");
    }

    #[test]
    fn record_escalation_without_workflow() {
        let mut svc = AuditService::new();
        let entry = svc.record_escalation(None, Severity::Critical, "Database unreachable");
        assert_eq!(entry.workflow_id, None);
    }

    #[test]
    fn record_escalation_resolved() {
        let mut svc = AuditService::new();
        let entry = svc.record_escalation_resolved("esc-1", "Restarted service");
        assert_eq!(entry.action, AuditAction::EscalationResolved);
        assert_eq!(entry.metadata["escalation_id"], "esc-1");
    }

    // -------------------------------------------------------------------
    // Query
    // -------------------------------------------------------------------

    #[test]
    fn query_by_workflow_id() {
        let mut svc = AuditService::new();
        svc.record_workflow_created("wf-a", WorkflowKind::LeadQualification, "web");
        svc.record_workflow_created("wf-b", WorkflowKind::FinanceOperations, "api");
        svc.record_workflow_started("wf-a");

        let results = svc.entries_for_workflow("wf-a");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn query_by_action() {
        let mut svc = AuditService::new();
        svc.record_workflow_created("wf-1", WorkflowKind::LeadQualification, "web");
        svc.record_workflow_started("wf-1");
        svc.record_workflow_completed("wf-1", &serde_json::json!({}));

        let q = AuditQuery::new().action(AuditAction::WorkflowCreated);
        let results = svc.query(&q);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn query_by_actor() {
        let mut svc = AuditService::new();
        svc.record_task_started("wf-1", "t-1", "agent-a", "tool");
        svc.record_task_started("wf-1", "t-2", "agent-b", "tool");

        let q = AuditQuery::new().actor("agent-a");
        let results = svc.query(&q);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].actor, "agent-a");
    }

    #[test]
    fn query_by_time_range() {
        let mut svc = AuditService::new();
        svc.record(AuditAction::ConfigChanged, "a", "old", None, serde_json::json!({}));
        // Entries are appended with Utc::now(), so "since" a bit in the past
        // should include them all.
        let since = Utc::now() - Duration::seconds(60);
        let until = Utc::now() + Duration::seconds(1);
        let q = AuditQuery::new().since(since).until(until);
        let results = svc.query(&q);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn query_with_limit() {
        let mut svc = AuditService::new();
        for i in 0..10 {
            svc.record(AuditAction::ConfigChanged, "a", &i.to_string(), None, serde_json::json!({}));
        }
        let q = AuditQuery::new().limit(3);
        let results = svc.query(&q);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn query_combined_filters() {
        let mut svc = AuditService::new();
        svc.record_workflow_created("wf-1", WorkflowKind::LeadQualification, "web");
        svc.record_workflow_started("wf-1");
        svc.record_workflow_created("wf-2", WorkflowKind::FinanceOperations, "api");

        let q = AuditQuery::new()
            .workflow_id("wf-1")
            .action(AuditAction::WorkflowCreated);
        let results = svc.query(&q);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].workflow_id.as_deref(), Some("wf-1"));
    }

    #[test]
    fn query_empty_returns_all() {
        let mut svc = AuditService::new();
        svc.record(AuditAction::ConfigChanged, "a", "m", None, serde_json::json!({}));
        svc.record(AuditAction::ConfigChanged, "b", "m", None, serde_json::json!({}));
        let q = AuditQuery::new();
        assert_eq!(svc.query(&q).len(), 2);
    }

    #[test]
    fn query_no_matches() {
        let mut svc = AuditService::new();
        svc.record(AuditAction::ConfigChanged, "a", "m", None, serde_json::json!({}));
        let q = AuditQuery::new().workflow_id("nonexistent");
        assert!(svc.query(&q).is_empty());
    }

    // -------------------------------------------------------------------
    // Export
    // -------------------------------------------------------------------

    #[test]
    fn export_json_returns_array() {
        let mut svc = AuditService::new();
        svc.record_workflow_created("wf-1", WorkflowKind::LeadQualification, "web");
        let json = svc.export_json();
        assert!(json.is_array());
        assert_eq!(json.as_array().unwrap().len(), 1);
    }

    #[test]
    fn export_json_empty() {
        let svc = AuditService::new();
        let json = svc.export_json();
        assert_eq!(json.as_array().unwrap().len(), 0);
    }

    // -------------------------------------------------------------------
    // Append-only guarantee (compile-time check)
    // -------------------------------------------------------------------

    #[test]
    fn entries_are_in_chronological_order() {
        let mut svc = AuditService::new();
        for i in 0..5 {
            svc.record(AuditAction::ConfigChanged, "a", &i.to_string(), None, serde_json::json!({}));
        }
        let entries: Vec<_> = svc.query(&AuditQuery::new());
        for window in entries.windows(2) {
            assert!(window[0].timestamp <= window[1].timestamp);
        }
    }

    #[test]
    fn default_constructed_service_is_empty() {
        let svc = AuditService::default();
        assert!(svc.is_empty());
    }
}
