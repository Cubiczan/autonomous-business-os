from typing import Any

from sqlalchemy.orm import Session

from app.models import ApprovalStatus, AuditAction, HumanApproval, utcnow
from app.services.audit import AuditService


class ApprovalService:
    def __init__(self, session: Session):
        self.session = session
        self.audit = AuditService(session)

    def request(
        self,
        workflow_id: str,
        title: str,
        reason: str,
        proposed_action: dict[str, Any],
    ) -> HumanApproval:
        approval = HumanApproval(
            workflow_id=workflow_id,
            title=title,
            reason=reason,
            proposed_action=proposed_action,
        )
        self.session.add(approval)
        self.session.commit()
        self.audit.record(
            AuditAction.approval_requested,
            f"Approval requested: {title}",
            workflow_id=workflow_id,
            metadata={"approval_id": approval.id},
        )
        return approval

    def decide(
        self,
        approval: HumanApproval,
        status: ApprovalStatus,
        decided_by: str,
        decision_note: str | None = None,
    ) -> HumanApproval:
        approval.status = status
        approval.decided_by = decided_by
        approval.decision_note = decision_note
        approval.decided_at = utcnow()
        self.session.commit()
        self.audit.record(
            AuditAction.approval_updated,
            f"Approval {status.value}: {approval.title}",
            workflow_id=approval.workflow_id,
            actor=decided_by,
            metadata={"approval_id": approval.id},
        )
        return approval
