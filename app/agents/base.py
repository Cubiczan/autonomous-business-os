from collections.abc import Callable
from dataclasses import asdict, is_dataclass
from typing import Any

from sqlalchemy.orm import Session

from app.models import AgentTask, AuditAction, TaskStatus, Workflow, utcnow
from app.services.audit import AuditService


class BaseAgent:
    name = "base_agent"

    def __init__(self, session: Session):
        self.session = session
        self.audit = AuditService(session)

    def run(self, workflow: Workflow) -> dict[str, Any]:
        raise NotImplementedError

    def execute_tool(
        self,
        workflow: Workflow,
        tool_name: str,
        payload: dict[str, Any],
        func: Callable[[], Any],
    ) -> dict[str, Any]:
        task = AgentTask(
            workflow_id=workflow.id,
            agent_name=self.name,
            tool_name=tool_name,
            status=TaskStatus.running,
            input=payload,
            started_at=utcnow(),
        )
        self.session.add(task)
        self.session.commit()
        self.audit.record(
            AuditAction.task_started,
            f"{self.name}.{tool_name} started",
            workflow_id=workflow.id,
            metadata={"task_id": task.id},
        )

        try:
            raw_result = func()
            result = self._normalize_result(raw_result)
            task.status = TaskStatus.completed
            task.output = result
            task.completed_at = utcnow()
            self.session.commit()
            self.audit.record(
                AuditAction.task_completed,
                f"{self.name}.{tool_name} completed",
                workflow_id=workflow.id,
                metadata={"task_id": task.id, "simulated": result.get("simulated", False)},
            )
            return result
        except Exception as exc:
            task.status = TaskStatus.failed
            task.error = str(exc)
            task.completed_at = utcnow()
            self.session.commit()
            self.audit.record(
                AuditAction.task_failed,
                f"{self.name}.{tool_name} failed: {exc}",
                workflow_id=workflow.id,
                metadata={"task_id": task.id},
            )
            raise

    def _normalize_result(self, result: Any) -> dict[str, Any]:
        if is_dataclass(result):
            return asdict(result)
        if isinstance(result, dict):
            return result
        return {"value": result}
