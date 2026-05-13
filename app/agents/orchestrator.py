from typing import Any

from sqlalchemy.orm import Session

from app.agents.client_onboarding import ClientOnboardingAgent
from app.agents.delivery_monitoring import DeliveryMonitoringAgent
from app.agents.finance_operations import FinanceOperationsAgent
from app.agents.knowledge_communication import KnowledgeCommunicationAgent
from app.agents.lead_qualification import LeadQualificationAgent
from app.models import AuditAction, Workflow, WorkflowStatus, utcnow
from app.services.audit import AuditService
from app.services.escalation import EscalationService
from app.services.workflows import WorkflowService


class MasterOrchestrator:
    def __init__(self, session: Session):
        self.session = session
        self.workflow_service = WorkflowService(session)
        self.audit = AuditService(session)
        self.escalations = EscalationService(session)
        self.agents = {
            "lead_qualification": LeadQualificationAgent(session),
            "client_onboarding": ClientOnboardingAgent(session),
            "delivery_monitoring": DeliveryMonitoringAgent(session),
            "finance_operations": FinanceOperationsAgent(session),
            "knowledge_communication": KnowledgeCommunicationAgent(session),
        }

    def run_workflow(self, workflow: Workflow) -> dict[str, Any]:
        agent = self.agents.get(workflow.kind)
        if not agent:
            raise ValueError(f"Unsupported workflow kind: {workflow.kind}")

        self.workflow_service.mark_running(workflow)
        try:
            result = agent.run(workflow)
            if workflow.status != WorkflowStatus.waiting_for_human:
                self.workflow_service.mark_completed(workflow, result)
            else:
                workflow.result = result
                workflow.updated_at = utcnow()
                self.session.commit()
            return result
        except Exception as exc:
            if workflow.attempts < workflow.max_attempts:
                workflow.status = WorkflowStatus.pending
                workflow.result = {"last_error": str(exc), "retrying": True}
                workflow.updated_at = utcnow()
                self.session.commit()
                self.audit.record(
                    AuditAction.workflow_failed,
                    f"Workflow will retry: {exc}",
                    workflow_id=workflow.id,
                    metadata={"attempts": workflow.attempts, "max_attempts": workflow.max_attempts},
                )
            else:
                self.workflow_service.mark_failed(workflow, str(exc))
                self.escalations.create(
                    f"Workflow failed after retries: {workflow.title}",
                    workflow_id=workflow.id,
                    severity="high",
                    owner="ops",
                    context={"error": str(exc), "kind": workflow.kind},
                )
            raise
