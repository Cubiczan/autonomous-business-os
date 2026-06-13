from typing import Any

from sqlalchemy import desc, select
from sqlalchemy.orm import Session

from app.models import AuditAction, Workflow, WorkflowStatus, utcnow
from app.services.audit import AuditService


class WorkflowService:
    def __init__(self, session: Session):
        self.session = session
        self.audit = AuditService(session)

    def create(
        self,
        kind: str,
        title: str,
        payload: dict[str, Any],
        *,
        source: str = "api",
    ) -> Workflow:
        workflow = Workflow(kind=kind, title=title, payload=payload, source=source)
        self.session.add(workflow)
        self.session.commit()
        self.audit.record(
            AuditAction.workflow_created,
            f"Workflow created: {title}",
            workflow_id=workflow.id,
            metadata={"kind": kind, "source": source},
        )
        return workflow

    def mark_running(self, workflow: Workflow) -> None:
        # The scheduler may have already atomically claimed this workflow
        # (status -> running, attempts incremented) to win the dispatch race.
        # Avoid double-counting attempts / emitting a duplicate audit event in
        # that case while still marking running for direct callers.
        if workflow.status == WorkflowStatus.running:
            return
        workflow.status = WorkflowStatus.running
        workflow.attempts += 1
        workflow.updated_at = utcnow()
        self.session.commit()
        self.audit.record(
            AuditAction.workflow_started,
            f"Workflow started: {workflow.title}",
            workflow_id=workflow.id,
        )

    def mark_completed(self, workflow: Workflow, result: dict[str, Any]) -> None:
        workflow.status = WorkflowStatus.completed
        workflow.result = result
        workflow.updated_at = utcnow()
        self.session.commit()
        self.audit.record(
            AuditAction.workflow_completed,
            f"Workflow completed: {workflow.title}",
            workflow_id=workflow.id,
        )

    def mark_failed(self, workflow: Workflow, error: str) -> None:
        workflow.status = WorkflowStatus.failed
        workflow.result = {"error": error}
        workflow.updated_at = utcnow()
        self.session.commit()
        self.audit.record(
            AuditAction.workflow_failed,
            error,
            workflow_id=workflow.id,
        )

    def recent(self, limit: int = 20) -> list[Workflow]:
        return list(
            self.session.scalars(select(Workflow).order_by(desc(Workflow.created_at)).limit(limit))
        )
