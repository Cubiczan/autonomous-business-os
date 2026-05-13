from typing import Any

from sqlalchemy.orm import Session

from app.models import AuditAction, AuditLog


class AuditService:
    def __init__(self, session: Session):
        self.session = session

    def record(
        self,
        action: AuditAction,
        message: str,
        *,
        workflow_id: str | None = None,
        actor: str = "system",
        metadata: dict[str, Any] | None = None,
    ) -> AuditLog:
        entry = AuditLog(
            workflow_id=workflow_id,
            action=action,
            actor=actor,
            message=message,
            metadata_json=metadata or {},
        )
        self.session.add(entry)
        self.session.commit()
        return entry
