import enum
from datetime import datetime, timezone
from typing import Any
from uuid import uuid4

from sqlalchemy import JSON, Boolean, DateTime, Enum, Float, ForeignKey, Integer, String, Text
from sqlalchemy.orm import Mapped, mapped_column, relationship

from app.db import Base


def utcnow() -> datetime:
    return datetime.now(timezone.utc)


def new_id() -> str:
    return str(uuid4())


class WorkflowStatus(str, enum.Enum):
    pending = "pending"
    running = "running"
    waiting_for_human = "waiting_for_human"
    completed = "completed"
    failed = "failed"
    cancelled = "cancelled"


class TaskStatus(str, enum.Enum):
    queued = "queued"
    running = "running"
    completed = "completed"
    failed = "failed"
    escalated = "escalated"


class ApprovalStatus(str, enum.Enum):
    open = "open"
    approved = "approved"
    rejected = "rejected"


class DepartmentStatus(str, enum.Enum):
    active = "active"
    paused = "paused"
    killed = "killed"


class AgentRuntimeStatus(str, enum.Enum):
    idle = "idle"
    running = "running"
    waiting_for_approval = "waiting_for_approval"
    paused = "paused"
    error = "error"


class TrustLevel(str, enum.Enum):
    autonomous = "autonomous"
    supervised = "supervised"
    approval_required = "approval_required"


class SkillScope(str, enum.Enum):
    company = "company"
    department = "department"
    agent = "agent"


class SkillStatus(str, enum.Enum):
    active = "active"
    disabled = "disabled"


class ExternalActionStatus(str, enum.Enum):
    proposed = "proposed"
    approved = "approved"
    rejected = "rejected"
    executed = "executed"
    blocked = "blocked"


class SecurityEventStatus(str, enum.Enum):
    open = "open"
    acknowledged = "acknowledged"
    resolved = "resolved"


class AuditAction(str, enum.Enum):
    workflow_created = "workflow_created"
    workflow_started = "workflow_started"
    workflow_completed = "workflow_completed"
    workflow_failed = "workflow_failed"
    task_started = "task_started"
    task_completed = "task_completed"
    task_failed = "task_failed"
    approval_requested = "approval_requested"
    approval_updated = "approval_updated"
    escalation_created = "escalation_created"
    integration_called = "integration_called"
    department_created = "department_created"
    department_updated = "department_updated"
    department_paused = "department_paused"
    department_killed = "department_killed"
    agent_spawned = "agent_spawned"
    skill_registered = "skill_registered"
    skill_assigned = "skill_assigned"
    external_action_requested = "external_action_requested"
    external_action_blocked = "external_action_blocked"
    evidence_packet_recorded = "evidence_packet_recorded"
    security_event_created = "security_event_created"
    circuit_breaker_opened = "circuit_breaker_opened"


class Workflow(Base):
    __tablename__ = "workflows"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=new_id)
    kind: Mapped[str] = mapped_column(String(80), index=True)
    # Stable per-workflow key so downstream integrations (invoicing, CRM,
    # outbound email) can deduplicate even if a dispatch is retried. Populated
    # at creation time; unique to reject accidental double-inserts.
    idempotency_key: Mapped[str] = mapped_column(
        String(36), unique=True, index=True, default=new_id
    )
    status: Mapped[WorkflowStatus] = mapped_column(
        Enum(WorkflowStatus), default=WorkflowStatus.pending, index=True
    )
    title: Mapped[str] = mapped_column(String(255))
    source: Mapped[str] = mapped_column(String(120), default="api")
    payload: Mapped[dict[str, Any]] = mapped_column(JSON, default=dict)
    result: Mapped[dict[str, Any]] = mapped_column(JSON, default=dict)
    attempts: Mapped[int] = mapped_column(Integer, default=0)
    max_attempts: Mapped[int] = mapped_column(Integer, default=3)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)
    updated_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)

    tasks: Mapped[list["AgentTask"]] = relationship(back_populates="workflow")
    approvals: Mapped[list["HumanApproval"]] = relationship(back_populates="workflow")


class AgentTask(Base):
    __tablename__ = "agent_tasks"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=new_id)
    workflow_id: Mapped[str] = mapped_column(ForeignKey("workflows.id"), index=True)
    agent_name: Mapped[str] = mapped_column(String(120), index=True)
    tool_name: Mapped[str] = mapped_column(String(120), default="")
    status: Mapped[TaskStatus] = mapped_column(Enum(TaskStatus), default=TaskStatus.queued)
    input: Mapped[dict[str, Any]] = mapped_column(JSON, default=dict)
    output: Mapped[dict[str, Any]] = mapped_column(JSON, default=dict)
    error: Mapped[str | None] = mapped_column(Text, nullable=True)
    started_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True), nullable=True)
    completed_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True), nullable=True)

    workflow: Mapped[Workflow] = relationship(back_populates="tasks")


class MemoryEntry(Base):
    __tablename__ = "memory_entries"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=new_id)
    namespace: Mapped[str] = mapped_column(String(120), index=True)
    key: Mapped[str] = mapped_column(String(255), index=True)
    value: Mapped[dict[str, Any]] = mapped_column(JSON, default=dict)
    text: Mapped[str] = mapped_column(Text, default="")
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)
    updated_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)


class HumanApproval(Base):
    __tablename__ = "human_approvals"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=new_id)
    workflow_id: Mapped[str] = mapped_column(ForeignKey("workflows.id"), index=True)
    title: Mapped[str] = mapped_column(String(255))
    reason: Mapped[str] = mapped_column(Text)
    proposed_action: Mapped[dict[str, Any]] = mapped_column(JSON, default=dict)
    status: Mapped[ApprovalStatus] = mapped_column(
        Enum(ApprovalStatus), default=ApprovalStatus.open, index=True
    )
    decided_by: Mapped[str | None] = mapped_column(String(255), nullable=True)
    decision_note: Mapped[str | None] = mapped_column(Text, nullable=True)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)
    decided_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True), nullable=True)

    workflow: Mapped[Workflow] = relationship(back_populates="approvals")


class AuditLog(Base):
    __tablename__ = "audit_logs"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=new_id)
    workflow_id: Mapped[str | None] = mapped_column(String(36), index=True, nullable=True)
    action: Mapped[AuditAction] = mapped_column(Enum(AuditAction), index=True)
    actor: Mapped[str] = mapped_column(String(120), default="system")
    message: Mapped[str] = mapped_column(Text)
    metadata_json: Mapped[dict[str, Any]] = mapped_column(JSON, default=dict)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)


class Escalation(Base):
    __tablename__ = "escalations"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=new_id)
    workflow_id: Mapped[str | None] = mapped_column(String(36), index=True, nullable=True)
    severity: Mapped[str] = mapped_column(String(40), default="medium")
    owner: Mapped[str] = mapped_column(String(255), default="ops")
    reason: Mapped[str] = mapped_column(Text)
    context: Mapped[dict[str, Any]] = mapped_column(JSON, default=dict)
    resolved_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True), nullable=True)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)


class Lead(Base):
    __tablename__ = "leads"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=new_id)
    email: Mapped[str] = mapped_column(String(255), index=True)
    company: Mapped[str] = mapped_column(String(255), default="")
    name: Mapped[str] = mapped_column(String(255), default="")
    source: Mapped[str] = mapped_column(String(120), default="unknown")
    score: Mapped[float] = mapped_column(Float, default=0.0)
    enrichment: Mapped[dict[str, Any]] = mapped_column(JSON, default=dict)
    outreach: Mapped[dict[str, Any]] = mapped_column(JSON, default=dict)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)


class Department(Base):
    __tablename__ = "departments"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=new_id)
    name: Mapped[str] = mapped_column(String(255), index=True)
    department_type: Mapped[str] = mapped_column(String(80), index=True)
    purpose: Mapped[str] = mapped_column(Text)
    goals: Mapped[list[str]] = mapped_column(JSON, default=list)
    operating_rules: Mapped[list[str]] = mapped_column(JSON, default=list)
    status: Mapped[DepartmentStatus] = mapped_column(
        Enum(DepartmentStatus), default=DepartmentStatus.active, index=True
    )
    health_score: Mapped[float] = mapped_column(Float, default=1.0)
    revenue_signals: Mapped[dict[str, Any]] = mapped_column(JSON, default=dict)
    last_output: Mapped[dict[str, Any]] = mapped_column(JSON, default=dict)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)
    updated_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)

    agents: Mapped[list["AgentInstance"]] = relationship(back_populates="department")
    schedules: Mapped[list["DepartmentSchedule"]] = relationship(back_populates="department")


class AgentInstance(Base):
    __tablename__ = "agent_instances"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=new_id)
    department_id: Mapped[str | None] = mapped_column(
        ForeignKey("departments.id"), index=True, nullable=True
    )
    name: Mapped[str] = mapped_column(String(255), index=True)
    role: Mapped[str] = mapped_column(String(120), index=True)
    description: Mapped[str] = mapped_column(Text, default="")
    trust_level: Mapped[TrustLevel] = mapped_column(
        Enum(TrustLevel), default=TrustLevel.supervised, index=True
    )
    status: Mapped[AgentRuntimeStatus] = mapped_column(
        Enum(AgentRuntimeStatus), default=AgentRuntimeStatus.idle, index=True
    )
    tools: Mapped[list[str]] = mapped_column(JSON, default=list)
    memory_namespace: Mapped[str] = mapped_column(String(255), default="")
    schedule: Mapped[dict[str, Any]] = mapped_column(JSON, default=dict)
    current_task: Mapped[str | None] = mapped_column(Text, nullable=True)
    last_output: Mapped[dict[str, Any]] = mapped_column(JSON, default=dict)
    last_heartbeat: Mapped[datetime | None] = mapped_column(DateTime(timezone=True), nullable=True)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)
    updated_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)

    department: Mapped[Department | None] = relationship(back_populates="agents")
    skill_assignments: Mapped[list["AgentSkillAssignment"]] = relationship(back_populates="agent")


class Skill(Base):
    __tablename__ = "skills"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=new_id)
    name: Mapped[str] = mapped_column(String(255), index=True)
    slug: Mapped[str] = mapped_column(String(120), index=True)
    version: Mapped[str] = mapped_column(String(40), default="1.0.0")
    description: Mapped[str] = mapped_column(Text, default="")
    scope: Mapped[SkillScope] = mapped_column(Enum(SkillScope), default=SkillScope.company)
    department_id: Mapped[str | None] = mapped_column(String(36), index=True, nullable=True)
    agent_id: Mapped[str | None] = mapped_column(String(36), index=True, nullable=True)
    manifest: Mapped[dict[str, Any]] = mapped_column(JSON, default=dict)
    status: Mapped[SkillStatus] = mapped_column(Enum(SkillStatus), default=SkillStatus.active)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)
    updated_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)

    assignments: Mapped[list["AgentSkillAssignment"]] = relationship(back_populates="skill")


class AgentSkillAssignment(Base):
    __tablename__ = "agent_skill_assignments"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=new_id)
    agent_id: Mapped[str] = mapped_column(ForeignKey("agent_instances.id"), index=True)
    skill_id: Mapped[str] = mapped_column(ForeignKey("skills.id"), index=True)
    assigned_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)

    agent: Mapped[AgentInstance] = relationship(back_populates="skill_assignments")
    skill: Mapped[Skill] = relationship(back_populates="assignments")


class DepartmentSchedule(Base):
    __tablename__ = "department_schedules"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=new_id)
    department_id: Mapped[str] = mapped_column(ForeignKey("departments.id"), index=True)
    name: Mapped[str] = mapped_column(String(255))
    cadence: Mapped[str] = mapped_column(String(80), default="daily")
    workflow_kind: Mapped[str] = mapped_column(String(80), default="department_operation")
    payload_template: Mapped[dict[str, Any]] = mapped_column(JSON, default=dict)
    enabled: Mapped[bool] = mapped_column(Boolean, default=True)
    next_run_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True), nullable=True)
    last_run_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True), nullable=True)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)
    updated_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)

    department: Mapped[Department] = relationship(back_populates="schedules")


class ExternalAction(Base):
    __tablename__ = "external_actions"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=new_id)
    workflow_id: Mapped[str | None] = mapped_column(String(36), index=True, nullable=True)
    agent_id: Mapped[str | None] = mapped_column(String(36), index=True, nullable=True)
    action_type: Mapped[str] = mapped_column(String(120), index=True)
    summary: Mapped[str] = mapped_column(Text)
    payload: Mapped[dict[str, Any]] = mapped_column(JSON, default=dict)
    risk_level: Mapped[str] = mapped_column(String(40), default="medium")
    requires_approval: Mapped[bool] = mapped_column(Boolean, default=True)
    approval_id: Mapped[str | None] = mapped_column(String(36), index=True, nullable=True)
    status: Mapped[ExternalActionStatus] = mapped_column(
        Enum(ExternalActionStatus), default=ExternalActionStatus.proposed, index=True
    )
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)
    executed_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True), nullable=True)


class EvidencePacket(Base):
    __tablename__ = "evidence_packets"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=new_id)
    workflow_id: Mapped[str | None] = mapped_column(String(36), index=True, nullable=True)
    external_action_id: Mapped[str | None] = mapped_column(String(36), index=True, nullable=True)
    agent_id: Mapped[str | None] = mapped_column(String(36), index=True, nullable=True)
    packet_type: Mapped[str] = mapped_column(String(120), index=True)
    intent: Mapped[str] = mapped_column(Text)
    policy_version: Mapped[str] = mapped_column(String(80), default="vga-2026-01")
    context_hash: Mapped[str] = mapped_column(String(64), index=True)
    status: Mapped[str] = mapped_column(String(80), default="recorded", index=True)
    artifacts: Mapped[dict[str, Any]] = mapped_column(JSON, default=dict)
    attribution: Mapped[str] = mapped_column(Text, default="")
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)


class SecurityEvent(Base):
    __tablename__ = "security_events"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=new_id)
    workflow_id: Mapped[str | None] = mapped_column(String(36), index=True, nullable=True)
    agent_id: Mapped[str | None] = mapped_column(String(36), index=True, nullable=True)
    severity: Mapped[str] = mapped_column(String(40), default="medium", index=True)
    category: Mapped[str] = mapped_column(String(120), index=True)
    message: Mapped[str] = mapped_column(Text)
    evidence: Mapped[dict[str, Any]] = mapped_column(JSON, default=dict)
    status: Mapped[SecurityEventStatus] = mapped_column(
        Enum(SecurityEventStatus), default=SecurityEventStatus.open, index=True
    )
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)
    resolved_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True), nullable=True)


class CircuitBreaker(Base):
    __tablename__ = "circuit_breakers"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=new_id)
    integration: Mapped[str] = mapped_column(String(120), unique=True, index=True)
    limit_per_minute: Mapped[int] = mapped_column(Integer, default=120)
    calls_this_window: Mapped[int] = mapped_column(Integer, default=0)
    window_started_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)
    state: Mapped[str] = mapped_column(String(40), default="closed")
    opened_until: Mapped[datetime | None] = mapped_column(DateTime(timezone=True), nullable=True)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)
    updated_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)
