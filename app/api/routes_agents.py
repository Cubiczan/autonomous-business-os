from fastapi import APIRouter, Depends, HTTPException
from sqlalchemy import desc, select
from sqlalchemy.orm import Session

from app.agents.orchestrator import MasterOrchestrator
from app.db import get_session
from app.models import ApprovalStatus, HumanApproval, Workflow
from app.schemas import (
    ApprovalDecisionRequest,
    ContractSignedRequest,
    DeliveryStatusRequest,
    InvoiceRequest,
    KnowledgeQueryRequest,
    LeadIngestRequest,
    WorkflowCreateRequest,
)
from app.security import require_admin_api_key
from app.services.approval import ApprovalService
from app.services.workflows import WorkflowService

router = APIRouter(prefix="/agents", tags=["agents"], dependencies=[Depends(require_admin_api_key)])


@router.post("/workflows")
def create_workflow(
    request: WorkflowCreateRequest,
    run_immediately: bool = True,
    session: Session = Depends(get_session),
) -> dict:
    workflow = WorkflowService(session).create(
        request.kind,
        request.title,
        request.payload,
        source=request.source,
    )
    result = None
    if run_immediately:
        result = MasterOrchestrator(session).run_workflow(workflow)
    return {"workflow_id": workflow.id, "status": workflow.status.value, "result": result}


@router.get("/workflows")
def list_workflows(session: Session = Depends(get_session)) -> list[dict]:
    workflows = session.scalars(select(Workflow).order_by(desc(Workflow.created_at)).limit(100)).all()
    return [
        {
            "id": workflow.id,
            "kind": workflow.kind,
            "status": workflow.status.value,
            "title": workflow.title,
            "created_at": workflow.created_at.isoformat(),
            "result": workflow.result,
        }
        for workflow in workflows
    ]


@router.post("/lead-qualification")
def qualify_lead(request: LeadIngestRequest, session: Session = Depends(get_session)) -> dict:
    workflow = WorkflowService(session).create(
        "lead_qualification",
        f"Qualify lead {request.email}",
        request.model_dump(),
        source=request.source,
    )
    result = MasterOrchestrator(session).run_workflow(workflow)
    return {"workflow_id": workflow.id, "status": workflow.status.value, "result": result}


@router.post("/client-onboarding")
def onboard_client(request: ContractSignedRequest, session: Session = Depends(get_session)) -> dict:
    workflow = WorkflowService(session).create(
        "client_onboarding",
        f"Onboard {request.client_name}",
        request.model_dump(),
        source="api",
    )
    result = MasterOrchestrator(session).run_workflow(workflow)
    return {"workflow_id": workflow.id, "status": workflow.status.value, "result": result}


@router.post("/delivery-monitoring")
def monitor_delivery(request: DeliveryStatusRequest, session: Session = Depends(get_session)) -> dict:
    workflow = WorkflowService(session).create(
        "delivery_monitoring",
        f"Monitor {request.client_name}",
        request.model_dump(),
        source="api",
    )
    result = MasterOrchestrator(session).run_workflow(workflow)
    return {"workflow_id": workflow.id, "status": workflow.status.value, "result": result}


@router.post("/finance-operations")
def run_finance(request: InvoiceRequest, session: Session = Depends(get_session)) -> dict:
    workflow = WorkflowService(session).create(
        "finance_operations",
        f"Invoice {request.customer_id}",
        request.model_dump(),
        source="api",
    )
    result = MasterOrchestrator(session).run_workflow(workflow)
    return {"workflow_id": workflow.id, "status": workflow.status.value, "result": result}


@router.post("/knowledge")
def query_knowledge(request: KnowledgeQueryRequest, session: Session = Depends(get_session)) -> dict:
    workflow = WorkflowService(session).create(
        "knowledge_communication",
        "Knowledge query",
        request.model_dump(),
        source="api",
    )
    result = MasterOrchestrator(session).run_workflow(workflow)
    return {"workflow_id": workflow.id, "status": workflow.status.value, "result": result}


@router.post("/approvals/{approval_id}/decision")
def decide_approval(
    approval_id: str,
    request: ApprovalDecisionRequest,
    session: Session = Depends(get_session),
) -> dict:
    approval = session.get(HumanApproval, approval_id)
    if not approval:
        raise HTTPException(status_code=404, detail="Approval not found")
    status = ApprovalStatus.approved if request.status == "approved" else ApprovalStatus.rejected
    updated = ApprovalService(session).decide(
        approval,
        status,
        request.decided_by,
        request.decision_note,
    )
    return {"approval_id": updated.id, "status": updated.status.value}
