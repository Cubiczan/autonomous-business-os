//! # State Machine
//!
//! Enforced workflow, task, and approval state transitions with full validation.
//! Every transition checks preconditions, updates timestamps, and returns
//! descriptive errors on invalid operations.

use crate::types::*;
use chrono::Utc;
use thiserror::Error;

// ── Errors ────────────────────────────────────────────────────────────

/// Errors produced by invalid state transitions.
#[derive(Debug, Error, PartialEq)]
pub enum TransitionError {
    #[error("invalid transition from {from:?} to {to:?}")]
    InvalidTransition {
        from: WorkflowStatus,
        to: WorkflowStatus,
    },
    #[error("workflow has reached max attempts ({max_attempts})")]
    MaxAttemptsExceeded { max_attempts: u32 },
    #[error("task invalid transition from {from:?} to {to:?}")]
    InvalidTaskTransition {
        from: TaskStatus,
        to: TaskStatus,
    },
    #[error("approval already decided")]
    AlreadyDecided,
}

// ── Workflow State Machine ───────────────────────────────────────────

/// Manages valid state transitions for [`Workflow`].
///
/// # Valid transitions
///
/// ```text
/// Pending          → Running              (increments attempts)
/// Running          → Completed | Failed | WaitingForHuman
/// WaitingForHuman  → Running | Completed | Failed
/// Failed           → Pending              (retry, only if attempts < max_attempts)
/// Any (not Completed, not Cancelled) → Cancelled
/// ```
pub struct WorkflowStateMachine;

impl WorkflowStateMachine {
    // ── Queries ───────────────────────────────────────────────────

    /// Returns `true` if transitioning `from → to` is valid for a workflow.
    pub fn can_transition(from: WorkflowStatus, to: WorkflowStatus) -> bool {
        match from {
            WorkflowStatus::Pending => matches!(to, WorkflowStatus::Running),
            WorkflowStatus::Running => matches!(
                to,
                WorkflowStatus::Completed
                    | WorkflowStatus::Failed
                    | WorkflowStatus::WaitingForHuman
            ),
            WorkflowStatus::WaitingForHuman => matches!(
                to,
                WorkflowStatus::Running
                    | WorkflowStatus::Completed
                    | WorkflowStatus::Failed
            ),
            WorkflowStatus::Failed => matches!(to, WorkflowStatus::Pending),
            WorkflowStatus::Completed => false,
            WorkflowStatus::Cancelled => false,
        }
    }

    // ── Generic transition ────────────────────────────────────────

    /// Move `workflow` to `to` if the transition is valid.
    ///
    /// This is the low-level primitive used by the convenience methods below.
    pub fn transition(
        workflow: &mut Workflow,
        to: WorkflowStatus,
    ) -> Result<(), TransitionError> {
        let from = workflow.status;
        if !Self::can_transition(from, to) {
            return Err(TransitionError::InvalidTransition { from, to });
        }
        workflow.status = to;
        workflow.updated_at = Utc::now();
        Ok(())
    }

    // ── Convenience methods ───────────────────────────────────────

    /// `Pending → Running`. Increments the attempt counter.
    pub fn mark_running(workflow: &mut Workflow) -> Result<(), TransitionError> {
        Self::transition(workflow, WorkflowStatus::Running)?;
        workflow.attempts += 1;
        Ok(())
    }

    /// `Running | WaitingForHuman → Completed`.
    pub fn mark_completed(
        workflow: &mut Workflow,
        result: serde_json::Value,
    ) -> Result<(), TransitionError> {
        Self::transition(workflow, WorkflowStatus::Completed)?;
        workflow.result = Some(result);
        workflow.completed_at = Some(Utc::now());
        Ok(())
    }

    /// `Running | WaitingForHuman → Failed`.
    pub fn mark_failed(workflow: &mut Workflow, error: String) -> Result<(), TransitionError> {
        Self::transition(workflow, WorkflowStatus::Failed)?;
        workflow.error = Some(error);
        Ok(())
    }

    /// `Running → WaitingForHuman`.
    pub fn mark_waiting_for_human(workflow: &mut Workflow) -> Result<(), TransitionError> {
        Self::transition(workflow, WorkflowStatus::WaitingForHuman)
    }

    /// `Any (not Completed, not Cancelled) → Cancelled`.
    pub fn mark_cancelled(workflow: &mut Workflow) -> Result<(), TransitionError> {
        let from = workflow.status;
        if matches!(from, WorkflowStatus::Completed | WorkflowStatus::Cancelled) {
            return Err(TransitionError::InvalidTransition {
                from,
                to: WorkflowStatus::Cancelled,
            });
        }
        workflow.status = WorkflowStatus::Cancelled;
        workflow.updated_at = Utc::now();
        Ok(())
    }

    /// `Failed → Pending` (retry). Checks that `attempts < max_attempts`.
    pub fn retry(workflow: &mut Workflow) -> Result<(), TransitionError> {
        if workflow.attempts >= workflow.max_attempts {
            return Err(TransitionError::MaxAttemptsExceeded {
                max_attempts: workflow.max_attempts,
            });
        }
        Self::transition(workflow, WorkflowStatus::Pending)
    }
}

// ── Task State Machine ───────────────────────────────────────────────

/// Manages valid state transitions for [`AgentTask`].
///
/// # Valid transitions
///
/// ```text
/// Queued    → Running
/// Running   → Completed | Failed | Escalated
/// Escalated → Running        (after human review)
/// ```
pub struct TaskStateMachine;

impl TaskStateMachine {
    /// Returns `true` if transitioning `from → to` is valid for a task.
    pub fn can_transition(from: TaskStatus, to: TaskStatus) -> bool {
        match from {
            TaskStatus::Queued => matches!(to, TaskStatus::Running),
            TaskStatus::Running => matches!(
                to,
                TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Escalated
            ),
            TaskStatus::Escalated => matches!(to, TaskStatus::Running),
            TaskStatus::Completed => false,
            TaskStatus::Failed => false,
        }
    }

    /// Move `task` to `to` if the transition is valid.
    pub fn transition(task: &mut AgentTask, to: TaskStatus) -> Result<(), TransitionError> {
        let from = task.status;
        if !Self::can_transition(from, to) {
            return Err(TransitionError::InvalidTaskTransition { from, to });
        }
        task.status = to;
        task.updated_at = Utc::now();
        let now = Utc::now();
        if matches!(to, TaskStatus::Running) && task.started_at.is_none() {
            task.started_at = Some(now);
        }
        if matches!(to, TaskStatus::Completed | TaskStatus::Failed) {
            task.completed_at = Some(now);
        }
        Ok(())
    }
}

// ── Approval State Machine ───────────────────────────────────────────

/// Manages valid state transitions for [`HumanApproval`].
///
/// # Valid transitions
///
/// ```text
/// Open → Approved | Rejected   (irreversible)
/// ```
pub struct ApprovalStateMachine;

impl ApprovalStateMachine {
    /// Returns `true` if transitioning `from → to` is valid for an approval.
    pub fn can_transition(from: ApprovalStatus, to: ApprovalStatus) -> bool {
        match (from, to) {
            (ApprovalStatus::Open, ApprovalStatus::Approved) => true,
            (ApprovalStatus::Open, ApprovalStatus::Rejected) => true,
            _ => false,
        }
    }

    /// Record a human decision on an Open approval.
    ///
    /// Once decided, no further transitions are allowed.
    pub fn decide(
        approval: &mut HumanApproval,
        decision: ApprovalDecision,
        decided_by: &str,
        note: Option<&str>,
    ) -> Result<(), TransitionError> {
        if !matches!(approval.status, ApprovalStatus::Open) {
            return Err(TransitionError::AlreadyDecided);
        }

        let target = match decision {
            ApprovalDecision::Approved => ApprovalStatus::Approved,
            ApprovalDecision::Rejected => ApprovalStatus::Rejected,
        };

        let now = Utc::now();
        approval.status = target;
        approval.decision = Some(decision);
        approval.decided_by = Some(decided_by.to_string());
        approval.decision_note = note.map(String::from);
        approval.note = note.map(String::from);
        approval.decided_at = Some(now);
        approval.updated_at = now;
        Ok(())
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helpers ───────────────────────────────────────────────────

    fn fresh_workflow() -> Workflow {
        Workflow::new("wf-1", 3)
    }

    fn running_workflow() -> Workflow {
        let mut wf = fresh_workflow();
        WorkflowStateMachine::mark_running(&mut wf).unwrap();
        wf
    }

    fn failed_workflow(max: u32) -> Workflow {
        let mut wf = Workflow::new("wf-fail", max);
        WorkflowStateMachine::mark_running(&mut wf).unwrap();
        WorkflowStateMachine::mark_failed(&mut wf, "boom".into()).unwrap();
        wf
    }

    fn waiting_workflow() -> Workflow {
        let mut wf = running_workflow();
        WorkflowStateMachine::mark_waiting_for_human(&mut wf).unwrap();
        wf
    }

    fn fresh_task() -> AgentTask {
        AgentTask::new("t-1", "wf-1")
    }

    fn running_task() -> AgentTask {
        let mut t = fresh_task();
        TaskStateMachine::transition(&mut t, TaskStatus::Running).unwrap();
        t
    }

    fn fresh_approval() -> HumanApproval {
        HumanApproval::new(
            "apr-1",
            "wf-1",
            "Deploy to prod",
            "Release v2",
            serde_json::json!({"action": "deploy"}),
        )
    }

    // ── Workflow: can_transition ─────────────────────────────────

    #[test]
    fn workflow_can_pending_to_running() {
        assert!(WorkflowStateMachine::can_transition(
            WorkflowStatus::Pending,
            WorkflowStatus::Running
        ));
    }

    #[test]
    fn workflow_can_running_to_completed() {
        assert!(WorkflowStateMachine::can_transition(
            WorkflowStatus::Running,
            WorkflowStatus::Completed
        ));
    }

    #[test]
    fn workflow_can_running_to_failed() {
        assert!(WorkflowStateMachine::can_transition(
            WorkflowStatus::Running,
            WorkflowStatus::Failed
        ));
    }

    #[test]
    fn workflow_can_running_to_waiting() {
        assert!(WorkflowStateMachine::can_transition(
            WorkflowStatus::Running,
            WorkflowStatus::WaitingForHuman
        ));
    }

    #[test]
    fn workflow_can_waiting_to_running() {
        assert!(WorkflowStateMachine::can_transition(
            WorkflowStatus::WaitingForHuman,
            WorkflowStatus::Running
        ));
    }

    #[test]
    fn workflow_can_waiting_to_completed() {
        assert!(WorkflowStateMachine::can_transition(
            WorkflowStatus::WaitingForHuman,
            WorkflowStatus::Completed
        ));
    }

    #[test]
    fn workflow_can_waiting_to_failed() {
        assert!(WorkflowStateMachine::can_transition(
            WorkflowStatus::WaitingForHuman,
            WorkflowStatus::Failed
        ));
    }

    #[test]
    fn workflow_can_failed_to_pending() {
        assert!(WorkflowStateMachine::can_transition(
            WorkflowStatus::Failed,
            WorkflowStatus::Pending
        ));
    }

    #[test]
    fn workflow_cannot_completed_anywhere() {
        assert!(!WorkflowStateMachine::can_transition(
            WorkflowStatus::Completed,
            WorkflowStatus::Running
        ));
        assert!(!WorkflowStateMachine::can_transition(
            WorkflowStatus::Completed,
            WorkflowStatus::Pending
        ));
        assert!(!WorkflowStateMachine::can_transition(
            WorkflowStatus::Completed,
            WorkflowStatus::Cancelled
        ));
    }

    #[test]
    fn workflow_cannot_cancelled_anywhere() {
        assert!(!WorkflowStateMachine::can_transition(
            WorkflowStatus::Cancelled,
            WorkflowStatus::Running
        ));
        assert!(!WorkflowStateMachine::can_transition(
            WorkflowStatus::Cancelled,
            WorkflowStatus::Pending
        ));
    }

    #[test]
    fn workflow_cannot_pending_to_completed() {
        assert!(!WorkflowStateMachine::can_transition(
            WorkflowStatus::Pending,
            WorkflowStatus::Completed
        ));
    }

    #[test]
    fn workflow_cannot_running_to_pending() {
        assert!(!WorkflowStateMachine::can_transition(
            WorkflowStatus::Running,
            WorkflowStatus::Pending
        ));
    }

    // ── Workflow: mark_running ───────────────────────────────────

    #[test]
    fn workflow_mark_running_success() {
        let mut wf = fresh_workflow();
        WorkflowStateMachine::mark_running(&mut wf).unwrap();
        assert_eq!(wf.status, WorkflowStatus::Running);
        assert_eq!(wf.attempts, 1);
    }

    #[test]
    fn workflow_mark_running_increments_attempts() {
        let mut wf = failed_workflow(3);
        WorkflowStateMachine::retry(&mut wf).unwrap(); // back to Pending
        WorkflowStateMachine::mark_running(&mut wf).unwrap();
        assert_eq!(wf.attempts, 2);
    }

    #[test]
    fn workflow_mark_running_from_completed_fails() {
        let mut wf = running_workflow();
        WorkflowStateMachine::mark_completed(&mut wf, serde_json::json!("ok")).unwrap();
        let err = WorkflowStateMachine::mark_running(&mut wf).unwrap_err();
        assert_eq!(
            err,
            TransitionError::InvalidTransition {
                from: WorkflowStatus::Completed,
                to: WorkflowStatus::Running,
            }
        );
    }

    // ── Workflow: mark_completed ─────────────────────────────────

    #[test]
    fn workflow_mark_completed_from_running() {
        let mut wf = running_workflow();
        let result = serde_json::json!({"output": 42});
        WorkflowStateMachine::mark_completed(&mut wf, result.clone()).unwrap();
        assert_eq!(wf.status, WorkflowStatus::Completed);
        assert_eq!(wf.result, Some(result));
        assert!(wf.completed_at.is_some());
    }

    #[test]
    fn workflow_mark_completed_from_waiting() {
        let mut wf = waiting_workflow();
        WorkflowStateMachine::mark_completed(&mut wf, serde_json::json!(true)).unwrap();
        assert_eq!(wf.status, WorkflowStatus::Completed);
    }

    #[test]
    fn workflow_mark_completed_from_pending_fails() {
        let mut wf = fresh_workflow();
        let err = WorkflowStateMachine::mark_completed(&mut wf, serde_json::json!(null)).unwrap_err();
        assert!(matches!(err, TransitionError::InvalidTransition { .. }));
    }

    // ── Workflow: mark_failed ────────────────────────────────────

    #[test]
    fn workflow_mark_failed_from_running() {
        let mut wf = running_workflow();
        WorkflowStateMachine::mark_failed(&mut wf, "timeout".into()).unwrap();
        assert_eq!(wf.status, WorkflowStatus::Failed);
        assert_eq!(wf.error.as_deref(), Some("timeout"));
    }

    #[test]
    fn workflow_mark_failed_from_waiting() {
        let mut wf = waiting_workflow();
        WorkflowStateMachine::mark_failed(&mut wf, "human said no".into()).unwrap();
        assert_eq!(wf.status, WorkflowStatus::Failed);
    }

    // ── Workflow: mark_waiting_for_human ─────────────────────────

    #[test]
    fn workflow_mark_waiting_success() {
        let mut wf = running_workflow();
        WorkflowStateMachine::mark_waiting_for_human(&mut wf).unwrap();
        assert_eq!(wf.status, WorkflowStatus::WaitingForHuman);
    }

    #[test]
    fn workflow_mark_waiting_from_pending_fails() {
        let mut wf = fresh_workflow();
        let err = WorkflowStateMachine::mark_waiting_for_human(&mut wf).unwrap_err();
        assert!(matches!(err, TransitionError::InvalidTransition { .. }));
    }

    // ── Workflow: mark_cancelled ─────────────────────────────────

    #[test]
    fn workflow_cancel_from_pending() {
        let mut wf = fresh_workflow();
        WorkflowStateMachine::mark_cancelled(&mut wf).unwrap();
        assert_eq!(wf.status, WorkflowStatus::Cancelled);
    }

    #[test]
    fn workflow_cancel_from_running() {
        let mut wf = running_workflow();
        WorkflowStateMachine::mark_cancelled(&mut wf).unwrap();
        assert_eq!(wf.status, WorkflowStatus::Cancelled);
    }

    #[test]
    fn workflow_cancel_from_waiting() {
        let mut wf = waiting_workflow();
        WorkflowStateMachine::mark_cancelled(&mut wf).unwrap();
        assert_eq!(wf.status, WorkflowStatus::Cancelled);
    }

    #[test]
    fn workflow_cancel_from_failed() {
        let mut wf = failed_workflow(3);
        WorkflowStateMachine::mark_cancelled(&mut wf).unwrap();
        assert_eq!(wf.status, WorkflowStatus::Cancelled);
    }

    #[test]
    fn workflow_cancel_from_completed_fails() {
        let mut wf = running_workflow();
        WorkflowStateMachine::mark_completed(&mut wf, serde_json::json!(null)).unwrap();
        let err = WorkflowStateMachine::mark_cancelled(&mut wf).unwrap_err();
        assert!(matches!(err, TransitionError::InvalidTransition { .. }));
    }

    #[test]
    fn workflow_cancel_from_cancelled_fails() {
        let mut wf = fresh_workflow();
        WorkflowStateMachine::mark_cancelled(&mut wf).unwrap();
        let err = WorkflowStateMachine::mark_cancelled(&mut wf).unwrap_err();
        assert!(matches!(err, TransitionError::InvalidTransition { .. }));
    }

    // ── Workflow: retry ──────────────────────────────────────────

    #[test]
    fn workflow_retry_success() {
        let mut wf = failed_workflow(3);
        WorkflowStateMachine::retry(&mut wf).unwrap();
        assert_eq!(wf.status, WorkflowStatus::Pending);
    }

    #[test]
    fn workflow_retry_exhausted() {
        let mut wf = failed_workflow(1); // only 1 attempt allowed, already used
        let err = WorkflowStateMachine::retry(&mut wf).unwrap_err();
        assert_eq!(
            err,
            TransitionError::MaxAttemptsExceeded { max_attempts: 1 }
        );
    }

    #[test]
    fn workflow_retry_multiple() {
        // max_attempts = 3, so 2 retries are allowed (3 total runs)
        let mut wf = failed_workflow(3);
        WorkflowStateMachine::retry(&mut wf).unwrap(); // attempt 2
        WorkflowStateMachine::mark_running(&mut wf).unwrap();
        WorkflowStateMachine::mark_failed(&mut wf, "again".into()).unwrap();
        WorkflowStateMachine::retry(&mut wf).unwrap(); // attempt 3
        WorkflowStateMachine::mark_running(&mut wf).unwrap();
        WorkflowStateMachine::mark_failed(&mut wf, "final".into()).unwrap();
        let err = WorkflowStateMachine::retry(&mut wf).unwrap_err();
        assert_eq!(
            err,
            TransitionError::MaxAttemptsExceeded { max_attempts: 3 }
        );
    }

    #[test]
    fn workflow_retry_from_running_fails() {
        let mut wf = running_workflow();
        let err = WorkflowStateMachine::retry(&mut wf).unwrap_err();
        assert!(matches!(err, TransitionError::InvalidTransition { .. }));
    }

    #[test]
    fn workflow_retry_from_pending_fails() {
        let mut wf = fresh_workflow();
        let err = WorkflowStateMachine::retry(&mut wf).unwrap_err();
        assert!(matches!(err, TransitionError::InvalidTransition { .. }));
    }

    // ── Workflow: edge cases ─────────────────────────────────────

    #[test]
    fn workflow_double_complete_fails() {
        let mut wf = running_workflow();
        WorkflowStateMachine::mark_completed(&mut wf, serde_json::json!(1)).unwrap();
        let err = WorkflowStateMachine::mark_completed(&mut wf, serde_json::json!(2)).unwrap_err();
        assert!(matches!(err, TransitionError::InvalidTransition { .. }));
    }

    #[test]
    fn workflow_double_fail_direct_fails() {
        let mut wf = running_workflow();
        WorkflowStateMachine::mark_failed(&mut wf, "first".into()).unwrap();
        let err = WorkflowStateMachine::mark_failed(&mut wf, "second".into()).unwrap_err();
        assert!(matches!(err, TransitionError::InvalidTransition { .. }));
    }

    #[test]
    fn workflow_transition_updates_timestamp() {
        let mut wf = fresh_workflow();
        let before = wf.updated_at;
        // Small sleep to ensure timestamp differs
        std::thread::sleep(std::time::Duration::from_millis(5));
        WorkflowStateMachine::mark_running(&mut wf).unwrap();
        assert!(wf.updated_at > before);
    }

    #[test]
    fn workflow_completed_at_set_on_complete() {
        let mut wf = running_workflow();
        assert!(wf.completed_at.is_none());
        WorkflowStateMachine::mark_completed(&mut wf, serde_json::json!(null)).unwrap();
        assert!(wf.completed_at.is_some());
    }

    #[test]
    fn workflow_completed_at_not_set_on_cancel() {
        let mut wf = running_workflow();
        WorkflowStateMachine::mark_cancelled(&mut wf).unwrap();
        assert!(wf.completed_at.is_none());
    }

    // ── Task: can_transition ─────────────────────────────────────

    #[test]
    fn task_can_queued_to_running() {
        assert!(TaskStateMachine::can_transition(
            TaskStatus::Queued,
            TaskStatus::Running
        ));
    }

    #[test]
    fn task_can_running_to_completed() {
        assert!(TaskStateMachine::can_transition(
            TaskStatus::Running,
            TaskStatus::Completed
        ));
    }

    #[test]
    fn task_can_running_to_failed() {
        assert!(TaskStateMachine::can_transition(
            TaskStatus::Running,
            TaskStatus::Failed
        ));
    }

    #[test]
    fn task_can_running_to_escalated() {
        assert!(TaskStateMachine::can_transition(
            TaskStatus::Running,
            TaskStatus::Escalated
        ));
    }

    #[test]
    fn task_can_escalated_to_running() {
        assert!(TaskStateMachine::can_transition(
            TaskStatus::Escalated,
            TaskStatus::Running
        ));
    }

    #[test]
    fn task_cannot_completed_anywhere() {
        assert!(!TaskStateMachine::can_transition(
            TaskStatus::Completed,
            TaskStatus::Running
        ));
        assert!(!TaskStateMachine::can_transition(
            TaskStatus::Completed,
            TaskStatus::Queued
        ));
    }

    #[test]
    fn task_cannot_failed_anywhere() {
        assert!(!TaskStateMachine::can_transition(
            TaskStatus::Failed,
            TaskStatus::Running
        ));
    }

    // ── Task: transition ─────────────────────────────────────────

    #[test]
    fn task_transition_queued_to_running() {
        let mut t = fresh_task();
        TaskStateMachine::transition(&mut t, TaskStatus::Running).unwrap();
        assert_eq!(t.status, TaskStatus::Running);
        assert!(t.started_at.is_some());
    }

    #[test]
    fn task_transition_running_to_completed() {
        let mut t = running_task();
        TaskStateMachine::transition(&mut t, TaskStatus::Completed).unwrap();
        assert_eq!(t.status, TaskStatus::Completed);
        assert!(t.completed_at.is_some());
    }

    #[test]
    fn task_transition_running_to_failed() {
        let mut t = running_task();
        TaskStateMachine::transition(&mut t, TaskStatus::Failed).unwrap();
        assert_eq!(t.status, TaskStatus::Failed);
        assert!(t.completed_at.is_some());
    }

    #[test]
    fn task_transition_running_to_escalated() {
        let mut t = running_task();
        TaskStateMachine::transition(&mut t, TaskStatus::Escalated).unwrap();
        assert_eq!(t.status, TaskStatus::Escalated);
        assert!(t.completed_at.is_none()); // not a terminal state
    }

    #[test]
    fn task_transition_escalated_to_running() {
        let mut t = running_task();
        TaskStateMachine::transition(&mut t, TaskStatus::Escalated).unwrap();
        TaskStateMachine::transition(&mut t, TaskStatus::Running).unwrap();
        assert_eq!(t.status, TaskStatus::Running);
    }

    #[test]
    fn task_transition_invalid_fails() {
        let mut t = fresh_task();
        let err = TaskStateMachine::transition(&mut t, TaskStatus::Completed).unwrap_err();
        assert_eq!(
            err,
            TransitionError::InvalidTaskTransition {
                from: TaskStatus::Queued,
                to: TaskStatus::Completed,
            }
        );
    }

    #[test]
    fn task_double_complete_fails() {
        let mut t = running_task();
        TaskStateMachine::transition(&mut t, TaskStatus::Completed).unwrap();
        let err = TaskStateMachine::transition(&mut t, TaskStatus::Running).unwrap_err();
        assert!(matches!(err, TransitionError::InvalidTaskTransition { .. }));
    }

    #[test]
    fn task_escalate_then_complete() {
        let mut t = running_task();
        TaskStateMachine::transition(&mut t, TaskStatus::Escalated).unwrap();
        TaskStateMachine::transition(&mut t, TaskStatus::Running).unwrap();
        TaskStateMachine::transition(&mut t, TaskStatus::Completed).unwrap();
        assert_eq!(t.status, TaskStatus::Completed);
        assert!(t.completed_at.is_some());
    }

    // ── Approval: can_transition ─────────────────────────────────

    #[test]
    fn approval_can_open_to_approved() {
        assert!(ApprovalStateMachine::can_transition(
            ApprovalStatus::Open,
            ApprovalStatus::Approved
        ));
    }

    #[test]
    fn approval_can_open_to_rejected() {
        assert!(ApprovalStateMachine::can_transition(
            ApprovalStatus::Open,
            ApprovalStatus::Rejected
        ));
    }

    #[test]
    fn approval_cannot_approved_to_rejected() {
        assert!(!ApprovalStateMachine::can_transition(
            ApprovalStatus::Approved,
            ApprovalStatus::Rejected
        ));
    }

    #[test]
    fn approval_cannot_rejected_to_approved() {
        assert!(!ApprovalStateMachine::can_transition(
            ApprovalStatus::Rejected,
            ApprovalStatus::Approved
        ));
    }

    // ── Approval: decide ─────────────────────────────────────────

    #[test]
    fn approval_decide_approve() {
        let mut a = fresh_approval();
        ApprovalStateMachine::decide(
            &mut a,
            ApprovalDecision::Approved,
            "alice",
            Some("looks good"),
        )
        .unwrap();
        assert_eq!(a.status, ApprovalStatus::Approved);
        assert_eq!(a.decision, Some(ApprovalDecision::Approved));
        assert_eq!(a.decided_by.as_deref(), Some("alice"));
        assert_eq!(a.decision_note.as_deref(), Some("looks good"));
        assert_eq!(a.note.as_deref(), Some("looks good"));
        assert!(a.decided_at.is_some());
    }

    #[test]
    fn approval_decide_reject() {
        let mut a = fresh_approval();
        ApprovalStateMachine::decide(&mut a, ApprovalDecision::Rejected, "bob", Some("nope"))
            .unwrap();
        assert_eq!(a.status, ApprovalStatus::Rejected);
        assert_eq!(a.decision, Some(ApprovalDecision::Rejected));
        assert_eq!(a.decided_by.as_deref(), Some("bob"));
    }

    #[test]
    fn approval_decide_no_note() {
        let mut a = fresh_approval();
        ApprovalStateMachine::decide(&mut a, ApprovalDecision::Approved, "carol", None).unwrap();
        assert!(a.decision_note.is_none());
        assert!(a.note.is_none());
    }

    #[test]
    fn approval_already_decided_approve_again() {
        let mut a = fresh_approval();
        ApprovalStateMachine::decide(&mut a, ApprovalDecision::Approved, "alice", None).unwrap();
        let err = ApprovalStateMachine::decide(
            &mut a,
            ApprovalDecision::Rejected,
            "bob",
            None,
        )
        .unwrap_err();
        assert_eq!(err, TransitionError::AlreadyDecided);
    }

    #[test]
    fn approval_already_decided_reject_again() {
        let mut a = fresh_approval();
        ApprovalStateMachine::decide(&mut a, ApprovalDecision::Rejected, "alice", None).unwrap();
        let err = ApprovalStateMachine::decide(
            &mut a,
            ApprovalDecision::Approved,
            "bob",
            None,
        )
        .unwrap_err();
        assert_eq!(err, TransitionError::AlreadyDecided);
    }

    #[test]
    fn approval_decide_updates_timestamp() {
        let mut a = fresh_approval();
        let before = a.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(5));
        ApprovalStateMachine::decide(&mut a, ApprovalDecision::Approved, "alice", None).unwrap();
        assert!(a.updated_at > before);
    }

    #[test]
    fn approval_decide_sets_decided_at() {
        let mut a = fresh_approval();
        assert!(a.decided_at.is_none());
        ApprovalStateMachine::decide(&mut a, ApprovalDecision::Approved, "alice", None).unwrap();
        assert!(a.decided_at.is_some());
    }
}
