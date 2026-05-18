//! # Approval Service
//!
//! Human-in-the-loop approval chain that tracks pending and decided requests,
//! provides statistics, and supports timeout-based alerting.

use crate::state_machine::ApprovalStateMachine;
use crate::types::*;
use chrono::Utc;
use thiserror::Error;

// ── Error ────────────────────────────────────────────────────────────

/// Errors produced by the [`ApprovalService`].
#[derive(Debug, Error, PartialEq)]
pub enum ApprovalError {
    #[error("approval not found: {0}")]
    NotFound(String),
    #[error("approval already decided: {0}")]
    AlreadyDecided(String),
}

// ── Stats ────────────────────────────────────────────────────────────

/// Aggregate statistics across all approvals.
#[derive(Debug, Clone, PartialEq)]
pub struct ApprovalStats {
    pub total_requested: usize,
    pub pending_count: usize,
    pub approved_count: usize,
    pub rejected_count: usize,
    pub avg_decision_time_secs: Option<f64>,
}

// ── Service ──────────────────────────────────────────────────────────

/// In-memory human-in-the-loop approval chain.
///
/// Approvals start in the **pending** list. When decided they are moved to
/// the **decided** list. All lookups and statistics operate across both
/// lists.
pub struct ApprovalService {
    pending: Vec<HumanApproval>,
    decided: Vec<HumanApproval>,
}

impl ApprovalService {
    /// Create a new empty approval service.
    pub fn new() -> Self {
        ApprovalService {
            pending: Vec::new(),
            decided: Vec::new(),
        }
    }

    /// Open a new approval request.
    ///
    /// Returns a reference to the newly created [`HumanApproval`].
    pub fn request_approval(
        &mut self,
        workflow_id: &str,
        title: &str,
        reason: &str,
        proposed_action: serde_json::Value,
    ) -> &HumanApproval {
        let id = format!("apr_{}", uuid::Uuid::new_v4().simple());
        let approval = HumanApproval::new(&id, workflow_id, title, reason, proposed_action);
        self.pending.push(approval);
        self.pending.last().unwrap()
    }

    /// Approve an open approval by `approval_id`.
    pub fn approve(
        &mut self,
        approval_id: &str,
        decided_by: &str,
        note: Option<&str>,
    ) -> Result<&HumanApproval, ApprovalError> {
        self.decide(approval_id, ApprovalDecision::Approved, decided_by, note)
    }

    /// Reject an open approval by `approval_id`.
    pub fn reject(
        &mut self,
        approval_id: &str,
        decided_by: &str,
        note: &str,
    ) -> Result<&HumanApproval, ApprovalError> {
        self.decide(approval_id, ApprovalDecision::Rejected, decided_by, Some(note))
    }

    /// Internal decision helper — moves the approval from pending to decided.
    fn decide(
        &mut self,
        approval_id: &str,
        decision: ApprovalDecision,
        decided_by: &str,
        note: Option<&str>,
    ) -> Result<&HumanApproval, ApprovalError> {
        let idx = self
            .pending
            .iter()
            .position(|a| a.id == approval_id)
            .ok_or_else(|| ApprovalError::NotFound(approval_id.to_string()))?;

        // Validate it is still open
        if !matches!(self.pending[idx].status, ApprovalStatus::Open) {
            return Err(ApprovalError::AlreadyDecided(approval_id.to_string()));
        }

        // Apply state machine transition
        ApprovalStateMachine::decide(&mut self.pending[idx], decision, decided_by, note)
            .map_err(|_| ApprovalError::AlreadyDecided(approval_id.to_string()))?;

        // Move from pending to decided
        let approval = self.pending.remove(idx);
        self.decided.push(approval);
        Ok(self.decided.last().unwrap())
    }

    /// All approvals that are still waiting for a human decision.
    pub fn get_pending(&self) -> Vec<&HumanApproval> {
        self.pending.iter().collect()
    }

    /// All approvals (pending + decided) associated with a given workflow.
    pub fn get_by_workflow(&self, workflow_id: &str) -> Vec<&HumanApproval> {
        self.pending
            .iter()
            .chain(self.decided.iter())
            .filter(|a| a.workflow_id == workflow_id)
            .collect()
    }

    /// Find an approval by its unique id (searches both pending and decided).
    pub fn get_by_id(&self, id: &str) -> Option<&HumanApproval> {
        self.pending
            .iter()
            .find(|a| a.id == id)
            .or_else(|| self.decided.iter().find(|a| a.id == id))
    }

    /// Aggregate statistics across all approvals.
    pub fn stats(&self) -> ApprovalStats {
        let total_requested = self.pending.len() + self.decided.len();
        let pending_count = self.pending.len();
        let approved_count = self
            .decided
            .iter()
            .filter(|a| a.status == ApprovalStatus::Approved)
            .count();
        let rejected_count = self
            .decided
            .iter()
            .filter(|a| a.status == ApprovalStatus::Rejected)
            .count();

        let avg_decision_time_secs = if self.decided.is_empty() {
            None
        } else {
            let total: i64 = self
                .decided
                .iter()
                .filter_map(|a| {
                    a.decided_at
                        .map(|d| (d - a.requested_at).num_seconds())
                })
                .sum();
            let count = self
                .decided
                .iter()
                .filter(|a| a.decided_at.is_some())
                .count();
            if count > 0 {
                Some(total as f64 / count as f64)
            } else {
                None
            }
        };

        ApprovalStats {
            total_requested,
            pending_count,
            approved_count,
            rejected_count,
            avg_decision_time_secs,
        }
    }

    /// Check whether an approval with the given id is still pending.
    pub fn is_pending(&self, approval_id: &str) -> bool {
        self.pending.iter().any(|a| a.id == approval_id)
    }

    /// Return pending approvals that have been open longer than `timeout_secs`.
    pub fn timeout_check(&self, timeout_secs: i64) -> Vec<&HumanApproval> {
        let now = Utc::now();
        self.pending
            .iter()
            .filter(|a| {
                let elapsed = (now - a.requested_at).num_seconds();
                elapsed > timeout_secs
            })
            .collect()
    }

    /// Number of pending approvals.
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Number of decided approvals.
    pub fn decided_len(&self) -> usize {
        self.decided.len()
    }
}

impl Default for ApprovalService {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    // ── Helpers ───────────────────────────────────────────────────

    fn new_service() -> ApprovalService {
        ApprovalService::new()
    }

    fn request_one(svc: &mut ApprovalService, wf_id: &str) -> String {
        let a = svc.request_approval(wf_id, "Title", "Reason", serde_json::json!({}));
        a.id.clone()
    }

    // ── Construction ──────────────────────────────────────────────

    #[test]
    fn new_service_is_empty() {
        let svc = new_service();
        assert!(svc.get_pending().is_empty());
        assert_eq!(svc.stats().total_requested, 0);
    }

    #[test]
    fn default_is_empty() {
        let svc = ApprovalService::default();
        assert_eq!(svc.pending_len(), 0);
    }

    // ── Request ───────────────────────────────────────────────────

    #[test]
    fn request_approval_creates_open() {
        let mut svc = new_service();
        let a = svc.request_approval(
            "wf-1",
            "Deploy",
            "Need prod access",
            serde_json::json!({"action": "deploy"}),
        );
        assert_eq!(a.status, ApprovalStatus::Open);
        assert_eq!(a.workflow_id, "wf-1");
        assert_eq!(a.title, "Deploy");
        assert_eq!(a.reason, "Need prod access");
    }

    #[test]
    fn request_approval_increments_stats() {
        let mut svc = new_service();
        svc.request_approval("wf-1", "A", "r", serde_json::json!({}));
        svc.request_approval("wf-1", "B", "r", serde_json::json!({}));
        let stats = svc.stats();
        assert_eq!(stats.total_requested, 2);
        assert_eq!(stats.pending_count, 2);
    }

    #[test]
    fn request_approval_generates_unique_ids() {
        let mut svc = new_service();
        let id1 = request_one(&mut svc, "wf-1");
        let id2 = request_one(&mut svc, "wf-1");
        assert_ne!(id1, id2);
    }

    // ── Approve ───────────────────────────────────────────────────

    #[test]
    fn approve_success() {
        let mut svc = new_service();
        let id = request_one(&mut svc, "wf-1");
        let result = svc.approve(&id, "alice", Some("LGTM"));
        assert!(result.is_ok());
        let a = result.unwrap();
        assert_eq!(a.status, ApprovalStatus::Approved);
        assert_eq!(a.decided_by.as_deref(), Some("alice"));
        assert_eq!(a.decision_note.as_deref(), Some("LGTM"));
    }

    #[test]
    fn approve_moves_to_decided() {
        let mut svc = new_service();
        let id = request_one(&mut svc, "wf-1");
        svc.approve(&id, "alice", None).unwrap();
        assert!(svc.get_pending().is_empty());
        assert_eq!(svc.decided_len(), 1);
        assert!(!svc.is_pending(&id));
    }

    #[test]
    fn approve_not_found() {
        let mut svc = new_service();
        let err = svc.approve("nonexistent", "alice", None).unwrap_err();
        assert_eq!(err, ApprovalError::NotFound("nonexistent".into()));
    }

    #[test]
    fn approve_already_decided() {
        let mut svc = new_service();
        let id = request_one(&mut svc, "wf-1");
        svc.approve(&id, "alice", None).unwrap();
        let err = svc.approve(&id, "bob", None).unwrap_err();
        assert_eq!(err, ApprovalError::NotFound(id.clone()));
    }

    // ── Reject ────────────────────────────────────────────────────

    #[test]
    fn reject_success() {
        let mut svc = new_service();
        let id = request_one(&mut svc, "wf-1");
        let result = svc.reject(&id, "bob", "Not safe");
        assert!(result.is_ok());
        let a = result.unwrap();
        assert_eq!(a.status, ApprovalStatus::Rejected);
        assert_eq!(a.decided_by.as_deref(), Some("bob"));
        assert_eq!(a.decision_note.as_deref(), Some("Not safe"));
    }

    #[test]
    fn reject_moves_to_decided() {
        let mut svc = new_service();
        let id = request_one(&mut svc, "wf-1");
        svc.reject(&id, "bob", "no").unwrap();
        assert!(svc.get_pending().is_empty());
        assert_eq!(svc.decided_len(), 1);
    }

    #[test]
    fn reject_not_found() {
        let mut svc = new_service();
        let err = svc.reject("ghost", "bob", "no").unwrap_err();
        assert_eq!(err, ApprovalError::NotFound("ghost".into()));
    }

    // ── get_pending ───────────────────────────────────────────────

    #[test]
    fn get_pending_returns_only_open() {
        let mut svc = new_service();
        let id1 = request_one(&mut svc, "wf-1");
        let _id2 = request_one(&mut svc, "wf-1");
        svc.approve(&id1, "alice", None).unwrap();
        assert_eq!(svc.get_pending().len(), 1);
    }

    #[test]
    fn get_pending_empty_after_all_decided() {
        let mut svc = new_service();
        let id1 = request_one(&mut svc, "wf-1");
        let id2 = request_one(&mut svc, "wf-1");
        svc.approve(&id1, "alice", None).unwrap();
        svc.reject(&id2, "bob", "no").unwrap();
        assert!(svc.get_pending().is_empty());
    }

    // ── get_by_workflow ───────────────────────────────────────────

    #[test]
    fn get_by_workflow_filters_correctly() {
        let mut svc = new_service();
        request_one(&mut svc, "wf-1");
        request_one(&mut svc, "wf-1");
        request_one(&mut svc, "wf-2");
        assert_eq!(svc.get_by_workflow("wf-1").len(), 2);
        assert_eq!(svc.get_by_workflow("wf-2").len(), 1);
        assert_eq!(svc.get_by_workflow("wf-99").len(), 0);
    }

    #[test]
    fn get_by_workflow_includes_decided() {
        let mut svc = new_service();
        let id = request_one(&mut svc, "wf-1");
        svc.approve(&id, "alice", None).unwrap();
        assert_eq!(svc.get_by_workflow("wf-1").len(), 1);
    }

    // ── get_by_id ─────────────────────────────────────────────────

    #[test]
    fn get_by_id_pending() {
        let mut svc = new_service();
        let id = request_one(&mut svc, "wf-1");
        let found = svc.get_by_id(&id);
        assert!(found.is_some());
        assert_eq!(found.unwrap().status, ApprovalStatus::Open);
    }

    #[test]
    fn get_by_id_decided() {
        let mut svc = new_service();
        let id = request_one(&mut svc, "wf-1");
        svc.reject(&id, "bob", "no").unwrap();
        let found = svc.get_by_id(&id);
        assert!(found.is_some());
        assert_eq!(found.unwrap().status, ApprovalStatus::Rejected);
    }

    #[test]
    fn get_by_id_missing() {
        let svc = new_service();
        assert!(svc.get_by_id("nope").is_none());
    }

    // ── stats ─────────────────────────────────────────────────────

    #[test]
    fn stats_all_pending() {
        let mut svc = new_service();
        request_one(&mut svc, "wf-1");
        request_one(&mut svc, "wf-2");
        let s = svc.stats();
        assert_eq!(s.total_requested, 2);
        assert_eq!(s.pending_count, 2);
        assert_eq!(s.approved_count, 0);
        assert_eq!(s.rejected_count, 0);
        assert!(s.avg_decision_time_secs.is_none());
    }

    #[test]
    fn stats_mixed() {
        let mut svc = new_service();
        let id1 = request_one(&mut svc, "wf-1");
        let id2 = request_one(&mut svc, "wf-1");
        let _id3 = request_one(&mut svc, "wf-1"); // stays pending
        svc.approve(&id1, "alice", None).unwrap();
        svc.reject(&id2, "bob", "no").unwrap();
        let s = svc.stats();
        assert_eq!(s.total_requested, 3);
        assert_eq!(s.pending_count, 1);
        assert_eq!(s.approved_count, 1);
        assert_eq!(s.rejected_count, 1);
        assert!(s.avg_decision_time_secs.is_some());
    }

    #[test]
    fn stats_avg_decision_time() {
        let mut svc = new_service();
        let id1 = request_one(&mut svc, "wf-1");
        let id2 = request_one(&mut svc, "wf-1");
        svc.approve(&id1, "alice", None).unwrap();
        svc.reject(&id2, "bob", "no").unwrap();
        let s = svc.stats();
        // Both decisions were near-instant so avg should be very small
        assert!(s.avg_decision_time_secs.unwrap() < 2.0);
    }

    // ── is_pending ────────────────────────────────────────────────

    #[test]
    fn is_pending_true() {
        let mut svc = new_service();
        let id = request_one(&mut svc, "wf-1");
        assert!(svc.is_pending(&id));
    }

    #[test]
    fn is_pending_false_after_decide() {
        let mut svc = new_service();
        let id = request_one(&mut svc, "wf-1");
        svc.approve(&id, "alice", None).unwrap();
        assert!(!svc.is_pending(&id));
    }

    #[test]
    fn is_pending_false_nonexistent() {
        let svc = new_service();
        assert!(!svc.is_pending("nope"));
    }

    // ── timeout_check ─────────────────────────────────────────────

    #[test]
    fn timeout_check_no_timeouts() {
        let mut svc = new_service();
        request_one(&mut svc, "wf-1");
        let timed_out = svc.timeout_check(3600); // 1 hour
        assert!(timed_out.is_empty());
    }

    #[test]
    fn timeout_check_with_stale_approval() {
        let mut svc = new_service();
        let a = svc.request_approval("wf-1", "Old", "Reason", serde_json::json!({}));
        // Backdate the requested_at to simulate staleness
        let id = a.id.clone();
        if let Some(pos) = svc.pending.iter().position(|x| x.id == id) {
            svc.pending[pos].requested_at = Utc::now() - Duration::seconds(100);
        }
        let timed_out = svc.timeout_check(60);
        assert_eq!(timed_out.len(), 1);
        assert_eq!(timed_out[0].title, "Old");
    }

    #[test]
    fn timeout_check_decided_not_returned() {
        let mut svc = new_service();
        let a = svc.request_approval("wf-1", "Done", "Reason", serde_json::json!({}));
        let id = a.id.clone();
        if let Some(pos) = svc.pending.iter().position(|x| x.id == id) {
            svc.pending[pos].requested_at = Utc::now() - Duration::seconds(100);
        }
        svc.approve(&id, "alice", None).unwrap();
        let timed_out = svc.timeout_check(60);
        assert!(timed_out.is_empty());
    }

    #[test]
    fn timeout_check_mixed() {
        let mut svc = new_service();
        // Fresh one
        svc.request_approval("wf-1", "Fresh", "Reason", serde_json::json!({}));
        // Stale one
        let a = svc.request_approval("wf-1", "Stale", "Reason", serde_json::json!({}));
        let id = a.id.clone();
        if let Some(pos) = svc.pending.iter().position(|x| x.id == id) {
            svc.pending[pos].requested_at = Utc::now() - Duration::seconds(200);
        }
        let timed_out = svc.timeout_check(60);
        assert_eq!(timed_out.len(), 1);
        assert_eq!(timed_out[0].title, "Stale");
    }
}
