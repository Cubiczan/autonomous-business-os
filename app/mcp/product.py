"""In-process and optional HTTP wrappers around existing FastAPI product code."""

from __future__ import annotations

import os
from datetime import datetime
from enum import Enum
from typing import Any

import httpx
from sqlalchemy import desc, select
from sqlalchemy.orm import Session

from app.agents.orchestrator import MasterOrchestrator
from app.db import SessionLocal, init_db
from app.models import ApprovalStatus, HumanApproval
from app.schemas import InvoiceRequest, LeadIngestRequest
from app.services.approval import ApprovalService
from app.services.guardrails import GuardrailService
from app.services.workflows import WorkflowService


def ensure_product_ready() -> None:
    init_db()


def _jsonable(value: Any) -> Any:
    if isinstance(value, Enum):
        return value.value
    if isinstance(value, datetime):
        return value.isoformat()
    if isinstance(value, dict):
        return {key: _jsonable(item) for key, item in value.items()}
    if isinstance(value, list):
        return [_jsonable(item) for item in value]
    return value


def _serialize_approval(approval: HumanApproval) -> dict[str, Any]:
    return {
        "id": approval.id,
        "workflow_id": approval.workflow_id,
        "title": approval.title,
        "reason": approval.reason,
        "status": approval.status.value,
        "proposed_action": _jsonable(approval.proposed_action),
        "decided_by": approval.decided_by,
        "decision_note": approval.decision_note,
        "created_at": approval.created_at.isoformat() if approval.created_at else None,
        "decided_at": approval.decided_at.isoformat() if approval.decided_at else None,
    }


def product_mode() -> str:
    mode = os.environ.get("ABOS_MCP_MODE", "inprocess").strip().lower()
    return "http" if mode == "http" else "inprocess"


def _http_base() -> tuple[str, str]:
    base = os.environ.get("ABOS_BASE_URL", "http://localhost:8000").rstrip("/")
    admin_key = os.environ.get("ADMIN_API_KEY", "")
    if not admin_key:
        raise RuntimeError("ADMIN_API_KEY is required when ABOS_MCP_MODE=http")
    return base, admin_key


def _http_request(method: str, path: str, *, json_body: dict[str, Any] | None = None) -> Any:
    base, admin_key = _http_base()
    with httpx.Client(timeout=30.0) as client:
        response = client.request(
            method,
            f"{base}{path}",
            json=json_body,
            headers={"x-admin-api-key": admin_key},
        )
        response.raise_for_status()
        return response.json()


def list_approvals(
    status: str | None = None,
    limit: int = 50,
    *,
    session: Session | None = None,
) -> dict[str, Any]:
    if product_mode() == "http" and session is None:
        mission = _http_request("GET", "/agents/mission")
        items = mission.get("approval_inbox", [])
        if status and status != "open":
            return {
                "mode": "http",
                "approvals": [],
                "note": "HTTP mode lists open approvals from /agents/mission only.",
            }
        return {"mode": "http", "approvals": items[:limit]}

    def _list(active: Session) -> dict[str, Any]:
        statement = select(HumanApproval).order_by(desc(HumanApproval.created_at)).limit(limit)
        if status:
            statement = statement.where(HumanApproval.status == ApprovalStatus(status))
        approvals = active.scalars(statement).all()
        return {"mode": "inprocess", "approvals": [_serialize_approval(item) for item in approvals]}

    if session is not None:
        return _list(session)
    ensure_product_ready()
    with SessionLocal() as owned:
        return _list(owned)


def decide_approval(
    approval_id: str,
    status: str,
    decided_by: str,
    decision_note: str | None = None,
    *,
    session: Session | None = None,
) -> dict[str, Any]:
    if status not in {"approved", "rejected"}:
        raise ValueError("status must be 'approved' or 'rejected'")
    if product_mode() == "http" and session is None:
        return {
            "mode": "http",
            **_http_request(
                "POST",
                f"/agents/approvals/{approval_id}/decision",
                json_body={
                    "status": status,
                    "decided_by": decided_by,
                    "decision_note": decision_note,
                },
            ),
        }

    def _decide(active: Session) -> dict[str, Any]:
        approval = active.get(HumanApproval, approval_id)
        if not approval:
            raise ValueError(f"Approval not found: {approval_id}")
        decision = ApprovalStatus.approved if status == "approved" else ApprovalStatus.rejected
        updated = ApprovalService(active).decide(approval, decision, decided_by, decision_note)
        GuardrailService(active).sync_external_action_decision(updated)
        return {"mode": "inprocess", "approval": _serialize_approval(updated)}

    if session is not None:
        return _decide(session)
    ensure_product_ready()
    with SessionLocal() as owned:
        return _decide(owned)


def _run_workflow(session: Session, kind: str, title: str, payload: dict[str, Any], source: str) -> dict[str, Any]:
    workflow = WorkflowService(session).create(kind, title, payload, source=source)
    result = MasterOrchestrator(session).run_workflow(workflow)
    return {
        "mode": "inprocess",
        "workflow_id": workflow.id,
        "status": workflow.status.value,
        "result": _jsonable(result),
    }


def finance_operations(
    customer_id: str,
    amount_cents: int,
    description: str,
    *,
    customer_email: str | None = None,
    currency: str = "usd",
    due_in_days: int = 14,
    session: Session | None = None,
) -> dict[str, Any]:
    request = InvoiceRequest(
        customer_id=customer_id,
        customer_email=customer_email,
        amount_cents=amount_cents,
        currency=currency,
        description=description,
        due_in_days=due_in_days,
    )
    if product_mode() == "http" and session is None:
        return {"mode": "http", **_http_request("POST", "/agents/finance-operations", json_body=request.model_dump())}

    if session is not None:
        return _run_workflow(
            session,
            "finance_operations",
            f"Invoice {request.customer_id}",
            request.model_dump(),
            "mcp",
        )
    ensure_product_ready()
    with SessionLocal() as owned:
        return _run_workflow(
            owned,
            "finance_operations",
            f"Invoice {request.customer_id}",
            request.model_dump(),
            "mcp",
        )


def lead_qualification(
    email: str,
    *,
    name: str | None = None,
    company: str | None = None,
    title: str | None = None,
    source: str = "mcp",
    session: Session | None = None,
) -> dict[str, Any]:
    request = LeadIngestRequest(
        source=source,
        email=email,
        name=name,
        company=company,
        title=title,
    )
    if product_mode() == "http" and session is None:
        return {"mode": "http", **_http_request("POST", "/agents/lead-qualification", json_body=request.model_dump())}

    if session is not None:
        return _run_workflow(
            session,
            "lead_qualification",
            f"Qualify lead {request.email}",
            request.model_dump(),
            request.source,
        )
    ensure_product_ready()
    with SessionLocal() as owned:
        return _run_workflow(
            owned,
            "lead_qualification",
            f"Qualify lead {request.email}",
            request.model_dump(),
            request.source,
        )
