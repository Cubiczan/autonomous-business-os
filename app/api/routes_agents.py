from fastapi import APIRouter, Depends, HTTPException
from sqlalchemy import desc, select
from sqlalchemy.orm import Session

from app.agents.orchestrator import MasterOrchestrator
from app.db import get_session
from app.models import (
    AgentInstance,
    ApprovalStatus,
    Department,
    ExternalAction,
    HumanApproval,
    SecurityEvent,
    Skill,
    SkillScope,
    Workflow,
)
from app.schemas import (
    ApprovalDecisionRequest,
    ContractSignedRequest,
    DepartmentLaunchRequest,
    DeliveryStatusRequest,
    InvoiceRequest,
    KnowledgeQueryRequest,
    LeadIngestRequest,
    SkillCreateRequest,
    WorkflowCreateRequest,
)
from app.security import require_admin_api_key
from app.services.approval import ApprovalService
from app.services.departments import DepartmentFactory
from app.services.guardrails import GuardrailService
from app.services.skills import SkillRegistry
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


@router.get("/mission")
def mission_board(session: Session = Depends(get_session)) -> dict:
    departments = session.scalars(select(Department).order_by(desc(Department.created_at))).all()
    agents = session.scalars(select(AgentInstance).order_by(AgentInstance.role)).all()
    approvals = session.scalars(
        select(HumanApproval)
        .where(HumanApproval.status == ApprovalStatus.open)
        .order_by(desc(HumanApproval.created_at))
        .limit(50)
    ).all()
    workflows = session.scalars(select(Workflow).order_by(desc(Workflow.created_at)).limit(50)).all()
    external_actions = session.scalars(
        select(ExternalAction).order_by(desc(ExternalAction.created_at)).limit(50)
    ).all()
    alerts = session.scalars(select(SecurityEvent).order_by(desc(SecurityEvent.created_at)).limit(50)).all()
    skills = session.scalars(select(Skill).order_by(Skill.name)).all()
    return {
        "departments": [
            {
                "id": department.id,
                "name": department.name,
                "type": department.department_type,
                "status": department.status.value,
                "health_score": department.health_score,
                "revenue_signals": department.revenue_signals,
                "last_output": department.last_output,
            }
            for department in departments
        ],
        "agents": [
            {
                "id": agent.id,
                "department_id": agent.department_id,
                "name": agent.name,
                "role": agent.role,
                "status": agent.status.value,
                "trust_level": agent.trust_level.value,
                "current_task": agent.current_task,
            }
            for agent in agents
        ],
        "task_queue": [
            {
                "id": workflow.id,
                "kind": workflow.kind,
                "title": workflow.title,
                "status": workflow.status.value,
                "created_at": workflow.created_at.isoformat(),
            }
            for workflow in workflows
        ],
        "approval_inbox": [
            {
                "id": approval.id,
                "title": approval.title,
                "reason": approval.reason,
                "proposed_action": approval.proposed_action,
                "created_at": approval.created_at.isoformat(),
            }
            for approval in approvals
        ],
        "recent_outputs": [
            {"department": department.name, "output": department.last_output}
            for department in departments
            if department.last_output
        ],
        "external_actions": [
            {
                "id": action.id,
                "summary": action.summary,
                "action_type": action.action_type,
                "risk_level": action.risk_level,
                "status": action.status.value,
            }
            for action in external_actions
        ],
        "alert_feed": [
            {
                "id": alert.id,
                "severity": alert.severity,
                "category": alert.category,
                "message": alert.message,
                "created_at": alert.created_at.isoformat(),
            }
            for alert in alerts
        ],
        "skill_registry": [
            {
                "id": skill.id,
                "slug": skill.slug,
                "name": skill.name,
                "scope": skill.scope.value,
                "status": skill.status.value,
            }
            for skill in skills
        ],
    }


@router.post("/departments")
def launch_department(
    request: DepartmentLaunchRequest,
    session: Session = Depends(get_session),
) -> dict:
    department = DepartmentFactory(session).launch_from_description(
        request.description,
        requested_by=request.requested_by,
    )
    result = None
    if request.run_immediately:
        workflow = WorkflowService(session).create(
            "department_operation",
            f"Run {department.name}",
            {
                "department_id": department.id,
                "department_type": department.department_type,
                "mode": "launch",
            },
            source="department_factory",
        )
        result = MasterOrchestrator(session).run_workflow(workflow)
    return {
        "department": DepartmentFactory(session).serialize_department(department),
        "initial_run": result,
    }


@router.get("/departments")
def list_departments(session: Session = Depends(get_session)) -> list[dict]:
    departments = session.scalars(select(Department).order_by(desc(Department.created_at))).all()
    factory = DepartmentFactory(session)
    return [factory.serialize_department(department) for department in departments]


@router.post("/departments/{department_id}/run")
def run_department(department_id: str, session: Session = Depends(get_session)) -> dict:
    department = session.get(Department, department_id)
    if not department:
        raise HTTPException(status_code=404, detail="Department not found")
    workflow = WorkflowService(session).create(
        "department_operation",
        f"Run {department.name}",
        {
            "department_id": department.id,
            "department_type": department.department_type,
            "mode": "manual",
        },
        source="manual",
    )
    result = MasterOrchestrator(session).run_workflow(workflow)
    return {"workflow_id": workflow.id, "status": workflow.status.value, "result": result}


@router.post("/departments/{department_id}/pause")
def pause_department(department_id: str, session: Session = Depends(get_session)) -> dict:
    department = session.get(Department, department_id)
    if not department:
        raise HTTPException(status_code=404, detail="Department not found")
    updated = DepartmentFactory(session).pause(department)
    return {"department_id": updated.id, "status": updated.status.value}


@router.post("/departments/{department_id}/kill")
def kill_department(department_id: str, session: Session = Depends(get_session)) -> dict:
    department = session.get(Department, department_id)
    if not department:
        raise HTTPException(status_code=404, detail="Department not found")
    updated = DepartmentFactory(session).kill(department)
    return {"department_id": updated.id, "status": updated.status.value}


@router.post("/company/ensure")
def ensure_company_layer(session: Session = Depends(get_session)) -> dict:
    agents = DepartmentFactory(session).ensure_company_layer()
    return {
        "agents": [
            {
                "id": agent.id,
                "name": agent.name,
                "role": agent.role,
                "status": agent.status.value,
                "trust_level": agent.trust_level.value,
            }
            for agent in agents
        ]
    }


@router.get("/skills")
def list_skills(session: Session = Depends(get_session)) -> list[dict]:
    skills = SkillRegistry(session).list_active()
    return [
        {
            "id": skill.id,
            "name": skill.name,
            "slug": skill.slug,
            "version": skill.version,
            "scope": skill.scope.value,
            "department_id": skill.department_id,
            "agent_id": skill.agent_id,
            "status": skill.status.value,
            "description": skill.description,
            "manifest": skill.manifest,
        }
        for skill in skills
    ]


@router.post("/skills")
def create_skill(request: SkillCreateRequest, session: Session = Depends(get_session)) -> dict:
    skill = SkillRegistry(session).create_from_description(
        request.description,
        name=request.name,
        scope=SkillScope(request.scope),
        department_id=request.department_id,
        agent_id=request.agent_id,
    )
    return {
        "id": skill.id,
        "name": skill.name,
        "slug": skill.slug,
        "scope": skill.scope.value,
        "manifest": skill.manifest,
    }


@router.post("/skills/refresh")
def refresh_skills(session: Session = Depends(get_session)) -> dict:
    skills = SkillRegistry(session).refresh_from_manifest_directory()
    return {"registered": [{"id": skill.id, "slug": skill.slug, "name": skill.name} for skill in skills]}


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
    GuardrailService(session).sync_external_action_decision(updated)
    return {"approval_id": updated.id, "status": updated.status.value}
