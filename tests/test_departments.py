from sqlalchemy import select

from app.agents.orchestrator import MasterOrchestrator
from app.models import (
    AgentInstance,
    DepartmentSchedule,
    EvidencePacket,
    ExternalAction,
    HumanApproval,
    Skill,
    WorkflowStatus,
)
from app.services.departments import DepartmentFactory
from app.rust_core import department_metrics
from app.services.skills import SkillRegistry
from app.services.workflows import WorkflowService


def test_launch_content_department_spawns_agents_skills_and_schedules(db_session) -> None:
    department = DepartmentFactory(db_session).launch_from_description(
        "Create a Content department for YouTube"
    )

    agents = db_session.scalars(
        select(AgentInstance).where(AgentInstance.department_id == department.id)
    ).all()
    schedules = db_session.scalars(
        select(DepartmentSchedule).where(DepartmentSchedule.department_id == department.id)
    ).all()
    skills = db_session.scalars(select(Skill)).all()

    roles = {agent.role for agent in agents}
    assert department.department_type == "content"
    assert "department_ceo" in roles
    assert "video_script_agent" in roles
    assert "social_media_scheduler" in roles
    assert len(agents) >= 9
    assert schedules
    assert any(skill.slug == "youtube_scriptwriting" for skill in skills)
    assert all(agent.memory_namespace.startswith(f"department:{department.id}:") for agent in agents)


def test_plain_language_skill_is_registered(db_session) -> None:
    skill = SkillRegistry(db_session).create_from_description(
        "Draft GDPR-aware LinkedIn carousel copy and route it for approval",
        name="GDPR LinkedIn Carousel Writer",
    )

    assert skill.slug == "gdpr_linkedin_carousel_writer"
    assert "social_draft" in skill.manifest["tool_permissions"]
    assert skill.manifest["approval_policy"] == "human_approval_required_before_external_action"


def test_department_operation_creates_approval_gated_external_actions(db_session) -> None:
    department = DepartmentFactory(db_session).launch_from_description(
        "Launch a Content department for YouTube"
    )
    workflow = WorkflowService(db_session).create(
        "department_operation",
        "Run content department",
        {
            "department_id": department.id,
            "department_type": department.department_type,
            "channels": ["youtube", "linkedin"],
        },
    )

    result = MasterOrchestrator(db_session).run_workflow(workflow)
    actions = db_session.scalars(select(ExternalAction)).all()
    packets = db_session.scalars(select(EvidencePacket)).all()
    approvals = db_session.scalars(select(HumanApproval)).all()

    assert workflow.status == WorkflowStatus.waiting_for_human
    assert len(result["approval_requests"]) == 2
    assert len(actions) == 2
    assert len(packets) == 2
    assert len(approvals) == 2
    assert all(action.requires_approval for action in actions)
    assert all(packet.status == "waiting_for_approval" for packet in packets)
    assert all("Verifiable Governance Architecture" in packet.attribution for packet in packets)


def test_department_metrics_rust_bridge_merges_signals_and_health() -> None:
    metrics = department_metrics(
        {
            "department_type": "sales",
            "status": "active",
            "revenue_signals": {"lead_count": 2, "pipeline_value": 10_000},
            "output": {"leads": [{}, {}]},
            "approval_count": 3,
        }
    )

    assert metrics["health_score"] == 0.95
    assert metrics["revenue_signals"]["lead_count"] == 4
    assert metrics["revenue_signals"]["pipeline_value"] == 20_000
