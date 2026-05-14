from typing import Any

from sqlalchemy.orm import Session

from app.models import AuditAction, Escalation
from app.services.audit import AuditService


class EscalationService:
    def __init__(self, session: Session):
        self.session = session
        self.audit = AuditService(session)

    def create(
        self,
        reason: str,
        *,
        workflow_id: str | None = None,
        severity: str = "medium",
        owner: str = "ops",
        context: dict[str, Any] | None = None,
    ) -> Escalation:
        escalation = Escalation(
            workflow_id=workflow_id,
            severity=severity,
            owner=owner,
            reason=reason,
            context=context or {},
        )
        self.session.add(escalation)
        self.session.commit()
        self.audit.record(
            AuditAction.escalation_created,
            reason,
            workflow_id=workflow_id,
            metadata={"severity": severity, "owner": owner, "escalation_id": escalation.id},
        )
        return escalation
