from fastapi import APIRouter, Form, HTTPException, Request
from fastapi.responses import HTMLResponse, RedirectResponse
from fastapi.templating import Jinja2Templates
from sqlalchemy import desc, func, select

from app.db import SessionLocal
from app.models import ApprovalStatus, AuditLog, Escalation, HumanApproval, Workflow
from app.services.approval import ApprovalService

router = APIRouter(prefix="/admin", tags=["admin"])
templates = Jinja2Templates(directory="app/templates")


@router.get("", response_class=HTMLResponse)
def dashboard(request: Request) -> HTMLResponse:
    with SessionLocal() as session:
        status_counts = dict(
            session.execute(select(Workflow.status, func.count(Workflow.id)).group_by(Workflow.status)).all()
        )
        workflows = session.scalars(select(Workflow).order_by(desc(Workflow.created_at)).limit(8)).all()
        approvals = session.scalars(
            select(HumanApproval)
            .where(HumanApproval.status == ApprovalStatus.open)
            .order_by(desc(HumanApproval.created_at))
            .limit(8)
        ).all()
        escalations = session.scalars(
            select(Escalation).where(Escalation.resolved_at.is_(None)).order_by(desc(Escalation.created_at)).limit(8)
        ).all()
        return templates.TemplateResponse(
            "dashboard.html",
            {
                "request": request,
                "status_counts": status_counts,
                "workflows": workflows,
                "approvals": approvals,
                "escalations": escalations,
            },
        )


@router.get("/workflows", response_class=HTMLResponse)
def workflows(request: Request) -> HTMLResponse:
    with SessionLocal() as session:
        items = session.scalars(select(Workflow).order_by(desc(Workflow.created_at)).limit(100)).all()
        return templates.TemplateResponse("workflows.html", {"request": request, "workflows": items})


@router.get("/approvals", response_class=HTMLResponse)
def approvals(request: Request) -> HTMLResponse:
    with SessionLocal() as session:
        items = session.scalars(select(HumanApproval).order_by(desc(HumanApproval.created_at)).limit(100)).all()
        return templates.TemplateResponse("approvals.html", {"request": request, "approvals": items})


@router.post("/approvals/{approval_id}/decision")
def admin_decide_approval(
    approval_id: str,
    status: str = Form(...),
    decided_by: str = Form(...),
    decision_note: str | None = Form(default=None),
) -> RedirectResponse:
    with SessionLocal() as session:
        approval = session.get(HumanApproval, approval_id)
        if not approval:
            raise HTTPException(status_code=404, detail="Approval not found")
        decision = ApprovalStatus.approved if status == "approved" else ApprovalStatus.rejected
        ApprovalService(session).decide(approval, decision, decided_by, decision_note)
    return RedirectResponse(url="/admin/approvals", status_code=303)


@router.get("/audit", response_class=HTMLResponse)
def audit(request: Request) -> HTMLResponse:
    with SessionLocal() as session:
        logs = session.scalars(select(AuditLog).order_by(desc(AuditLog.created_at)).limit(200)).all()
        return templates.TemplateResponse("audit.html", {"request": request, "logs": logs})
