from fastapi import APIRouter, Depends, Header, HTTPException, Request
from sqlalchemy.orm import Session

from app.agents.orchestrator import MasterOrchestrator
from app.config import get_settings
from app.db import get_session
from app.security import verify_shared_secret, verify_slack_signature
from app.services.workflows import WorkflowService

router = APIRouter(prefix="/webhooks", tags=["webhooks"])


@router.post("/leads")
async def lead_webhook(request: Request, session: Session = Depends(get_session)) -> dict:
    payload = await request.json()
    workflow = WorkflowService(session).create(
        "lead_qualification",
        f"Webhook lead {payload.get('email', 'unknown')}",
        payload,
        source=payload.get("source", "webhook"),
    )
    result = MasterOrchestrator(session).run_workflow(workflow)
    return {"workflow_id": workflow.id, "status": workflow.status.value, "result": result}


@router.post("/docusign")
async def docusign_webhook(
    request: Request,
    x_docusign_signature_1: str | None = Header(default=None),
    session: Session = Depends(get_session),
) -> dict:
    settings = get_settings()
    if not verify_shared_secret(x_docusign_signature_1, settings.docusign_webhook_secret):
        raise HTTPException(status_code=401, detail="Invalid DocuSign signature")
    payload = await request.json()
    contract = {
        "client_name": payload.get("client_name") or payload.get("recipient_name") or "New client",
        "client_email": payload.get("client_email") or payload.get("recipient_email"),
        "contract_id": payload.get("envelopeId") or payload.get("contract_id"),
        "project_type": payload.get("project_type", "implementation"),
        "metadata": payload,
    }
    workflow = WorkflowService(session).create(
        "client_onboarding",
        f"Onboard {contract['client_name']}",
        contract,
        source="docusign",
    )
    result = MasterOrchestrator(session).run_workflow(workflow)
    return {"workflow_id": workflow.id, "status": workflow.status.value, "result": result}


@router.post("/stripe")
async def stripe_webhook(
    request: Request,
    stripe_signature: str | None = Header(default=None),
    session: Session = Depends(get_session),
) -> dict:
    settings = get_settings()
    if not verify_shared_secret(stripe_signature, settings.stripe_webhook_secret):
        raise HTTPException(status_code=401, detail="Invalid Stripe signature")
    payload = await request.json()
    workflow = WorkflowService(session).create(
        "finance_operations",
        f"Stripe event {payload.get('type', 'unknown')}",
        payload.get("data", {}).get("object", payload),
        source="stripe",
    )
    result = MasterOrchestrator(session).run_workflow(workflow)
    return {"workflow_id": workflow.id, "status": workflow.status.value, "result": result}


@router.post("/slack")
async def slack_webhook(request: Request, session: Session = Depends(get_session)) -> dict:
    body = await request.body()
    if not await verify_slack_signature(request, body):
        raise HTTPException(status_code=401, detail="Invalid Slack signature")
    payload = await request.json()
    if payload.get("type") == "url_verification":
        return {"challenge": payload.get("challenge")}
    event = payload.get("event", {})
    workflow = WorkflowService(session).create(
        "knowledge_communication",
        "Slack knowledge query",
        {
            "question": event.get("text", ""),
            "channel_id": event.get("channel"),
            "requester": event.get("user"),
            "metadata": payload,
        },
        source="slack",
    )
    result = MasterOrchestrator(session).run_workflow(workflow)
    return {"workflow_id": workflow.id, "status": workflow.status.value, "result": result}


@router.post("/calendar")
async def calendar_webhook(request: Request, session: Session = Depends(get_session)) -> dict:
    payload = await request.json()
    workflow = WorkflowService(session).create(
        "delivery_monitoring",
        f"Calendar delivery signal {payload.get('project_id', 'unknown')}",
        payload,
        source="calendar",
    )
    return {"workflow_id": workflow.id, "status": workflow.status.value}


@router.post("/uipath")
async def uipath_webhook(
    request: Request,
    x_uipath_signature: str | None = Header(default=None),
    session: Session = Depends(get_session),
) -> dict:
    settings = get_settings()
    if not verify_shared_secret(x_uipath_signature, settings.uipath_webhook_secret):
        raise HTTPException(status_code=401, detail="Invalid UiPath signature")

    payload = await request.json()
    workflow_kind = payload.get("workflow_kind") or "finance_operations"
    workflow = WorkflowService(session).create(
        workflow_kind,
        payload.get("title") or f"UiPath handoff {payload.get('subject', 'unknown')}",
        payload,
        source="uipath",
    )
    result = MasterOrchestrator(session).run_workflow(workflow)
    return {"workflow_id": workflow.id, "status": workflow.status.value, "result": result}
