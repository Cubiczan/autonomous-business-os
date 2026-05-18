from fastapi import APIRouter, Form, HTTPException, Request
from fastapi.responses import HTMLResponse, RedirectResponse
from fastapi.templating import Jinja2Templates
from sqlalchemy import desc, func, select

from app.db import SessionLocal
from app.models import (
    AgentInstance,
    AgentSkillAssignment,
    ApprovalStatus,
    AuditLog,
    Department,
    Escalation,
    ExternalAction,
    HumanApproval,
    SecurityEvent,
    Skill,
    Workflow,
)
from app.services.approval import ApprovalService
from app.services.guardrails import GuardrailService

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
        department_count = session.scalar(select(func.count(Department.id))) or 0
        active_agents = session.scalar(select(func.count(AgentInstance.id))) or 0
        skill_count = session.scalar(select(func.count(Skill.id))) or 0
        return templates.TemplateResponse(
            "dashboard.html",
            {
                "request": request,
                "status_counts": status_counts,
                "workflows": workflows,
                "approvals": approvals,
                "escalations": escalations,
                "department_count": department_count,
                "active_agents": active_agents,
                "skill_count": skill_count,
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


@router.get("/mission", response_class=HTMLResponse)
def mission(request: Request) -> HTMLResponse:
    with SessionLocal() as session:
        departments = session.scalars(select(Department).order_by(desc(Department.created_at))).all()
        company_agents = session.scalars(
            select(AgentInstance)
            .where(AgentInstance.department_id.is_(None))
            .order_by(AgentInstance.role)
        ).all()
        agents = session.scalars(select(AgentInstance).order_by(AgentInstance.role)).all()
        workflows = session.scalars(select(Workflow).order_by(desc(Workflow.created_at)).limit(20)).all()
        approvals = session.scalars(
            select(HumanApproval)
            .where(HumanApproval.status == ApprovalStatus.open)
            .order_by(desc(HumanApproval.created_at))
            .limit(20)
        ).all()
        external_actions = session.scalars(
            select(ExternalAction).order_by(desc(ExternalAction.created_at)).limit(20)
        ).all()
        security_events = session.scalars(
            select(SecurityEvent).order_by(desc(SecurityEvent.created_at)).limit(20)
        ).all()
        skills = session.scalars(select(Skill).order_by(Skill.name)).all()
        assignment_counts = dict(
            session.execute(
                select(AgentSkillAssignment.skill_id, func.count(AgentSkillAssignment.id)).group_by(
                    AgentSkillAssignment.skill_id
                )
            ).all()
        )
        return templates.TemplateResponse(
            "mission.html",
            {
                "request": request,
                "departments": departments,
                "company_agents": company_agents,
                "agents": agents,
                "workflows": workflows,
                "approvals": approvals,
                "external_actions": external_actions,
                "security_events": security_events,
                "skills": skills,
                "assignment_counts": assignment_counts,
            },
        )


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
        updated = ApprovalService(session).decide(approval, decision, decided_by, decision_note)
        GuardrailService(session).sync_external_action_decision(updated)
    return RedirectResponse(url="/admin/approvals", status_code=303)


@router.get("/audit", response_class=HTMLResponse)
def audit(request: Request) -> HTMLResponse:
    with SessionLocal() as session:
        logs = session.scalars(select(AuditLog).order_by(desc(AuditLog.created_at)).limit(200)).all()
        return templates.TemplateResponse("audit.html", {"request": request, "logs": logs})
