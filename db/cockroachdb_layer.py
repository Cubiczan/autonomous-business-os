# Autonomous Business OS — CockroachDB Persistence Layer
"""
SQLAlchemy ORM layer backed by CockroachDB for distributed,
ACID-compliant storage of all autonomous business OS domain data.

Connection: CockroachDB Serverless on GCP (autonomous_business_os database)

This replaces the SQLite backend with CockroachDB for horizontal scalability,
distributed ACID transactions, and multi-region resilience.

Domain entities (mirrors app/models.py):
  - Workflows: agent workflow orchestration
  - AgentTasks: individual agent tasks within workflows
  - MemoryEntries: key-value memory for agent state
  - HumanApprovals: human-in-the-loop approval gates
  - AuditLogs: immutable audit trail
  - Escalations: escalation tracking
  - Leads: lead qualification and tracking
"""

from __future__ import annotations

import json
import os
import logging
from datetime import date, datetime, timezone
from typing import Optional
from uuid import uuid4

from sqlalchemy import (
    create_engine, Column, String, Integer, Numeric, Date, DateTime,
    Text, ForeignKey, Index, JSON, func, select, desc, update, delete,
    Boolean, Float, Enum as SAEnum,
)
from sqlalchemy.orm import (
    declarative_base, relationship, Session, sessionmaker,
)
from sqlalchemy.dialects.postgresql import JSONB

logger = logging.getLogger("autonomous_business_os.db")

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

COCKROACH_URL = (
    "cockroachdb+psycopg2://cubiczan:oY-hPkgXtZjc6kGqY67Gyg@"
    "vortex-giraffe-15678.jxf.gcp-us-east1.cockroachlabs.cloud:26257/"
    "autonomous_business_os?sslmode=require"
)
DATABASE_URL = os.getenv("ABOS_DATABASE_URL", COCKROACH_URL)

engine = create_engine(
    DATABASE_URL,
    pool_size=10,
    max_overflow=5,
    pool_timeout=30,
    pool_pre_ping=True,
    echo=False,
)

SessionLocal = sessionmaker(bind=engine, autoflush=False)


def get_session() -> Session:
    """Get a new database session."""
    return SessionLocal()


# ---------------------------------------------------------------------------
# Base Model
# ---------------------------------------------------------------------------

Base = declarative_base()


def utcnow() -> datetime:
    return datetime.now(timezone.utc)


def new_id() -> str:
    return str(uuid4())


class TimestampMixin:
    """Common timestamp fields for all models."""
    created_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), server_default=func.now(),
                        onupdate=func.now())


# ---------------------------------------------------------------------------
# ORM Models (mirrors app/models.py — CockroachDB-compatible)
# ---------------------------------------------------------------------------

class WorkflowModel(TimestampMixin, Base):
    __tablename__ = "workflows"

    id = Column(String(36), primary_key=True, default=new_id)
    kind = Column(String(80), nullable=False, index=True)
    status = Column(String(30), default="pending", index=True)
    title = Column(String(255), nullable=False)
    source = Column(String(120), default="api")
    payload = Column(JSONB, default=dict)
    result = Column(JSONB, default=dict)
    attempts = Column(Integer, default=0)
    max_attempts = Column(Integer, default=3)

    tasks = relationship("AgentTaskModel", back_populates="workflow_rel")
    approvals = relationship("HumanApprovalModel", back_populates="workflow_rel")


class AgentTaskModel(TimestampMixin, Base):
    __tablename__ = "agent_tasks"

    id = Column(String(36), primary_key=True, default=new_id)
    workflow_id = Column(String(36), ForeignKey("workflows.id"), nullable=False, index=True)
    agent_name = Column(String(120), nullable=False, index=True)
    tool_name = Column(String(120), default="")
    status = Column(String(30), default="queued")
    input_data = Column("input", JSONB, default=dict)
    output_data = Column("output", JSONB, default=dict)
    error = Column(Text, nullable=True)
    started_at = Column(DateTime(timezone=True), nullable=True)
    completed_at = Column(DateTime(timezone=True), nullable=True)

    workflow_rel = relationship("WorkflowModel", back_populates="tasks")


class MemoryEntryModel(TimestampMixin, Base):
    __tablename__ = "memory_entries"

    id = Column(String(36), primary_key=True, default=new_id)
    namespace = Column(String(120), nullable=False, index=True)
    key = Column(String(255), nullable=False, index=True)
    value = Column(JSONB, default=dict)
    text = Column(Text, default="")


class HumanApprovalModel(TimestampMixin, Base):
    __tablename__ = "human_approvals"

    id = Column(String(36), primary_key=True, default=new_id)
    workflow_id = Column(String(36), ForeignKey("workflows.id"), nullable=False, index=True)
    title = Column(String(255), nullable=False)
    reason = Column(Text, default="")
    proposed_action = Column(JSONB, default=dict)
    status = Column(String(20), default="open", index=True)
    decided_by = Column(String(255), nullable=True)
    decision_note = Column(Text, nullable=True)
    created_at = Column(DateTime(timezone=True), server_default=func.now())
    decided_at = Column(DateTime(timezone=True), nullable=True)

    workflow_rel = relationship("WorkflowModel", back_populates="approvals")


class AuditLogModel(TimestampMixin, Base):
    __tablename__ = "audit_logs"

    id = Column(String(36), primary_key=True, default=new_id)
    workflow_id = Column(String(36), nullable=True, index=True)
    action = Column(String(60), nullable=False, index=True)
    actor = Column(String(120), default="system")
    message = Column(Text, default="")
    metadata_json = Column(JSONB, default=dict)
    created_at = Column(DateTime(timezone=True), server_default=func.now())


class EscalationModel(TimestampMixin, Base):
    __tablename__ = "escalations"

    id = Column(String(36), primary_key=True, default=new_id)
    workflow_id = Column(String(36), nullable=True, index=True)
    severity = Column(String(40), default="medium")
    owner = Column(String(255), default="ops")
    reason = Column(Text, default="")
    context = Column(JSONB, default=dict)
    resolved_at = Column(DateTime(timezone=True), nullable=True)
    created_at = Column(DateTime(timezone=True), server_default=func.now())


class LeadModel(TimestampMixin, Base):
    __tablename__ = "leads"

    id = Column(String(36), primary_key=True, default=new_id)
    email = Column(String(255), nullable=False, index=True)
    company = Column(String(255), default="")
    name = Column(String(255), default="")
    source = Column(String(120), default="unknown")
    score = Column(Float, default=0.0)
    enrichment = Column(JSONB, default=dict)
    outreach = Column(JSONB, default=dict)
    created_at = Column(DateTime(timezone=True), server_default=func.now())


# ---------------------------------------------------------------------------
# Repository Classes
# ---------------------------------------------------------------------------

class WorkflowRepository:
    @staticmethod
    def get_all(session: Session) -> list[dict]:
        rows = session.execute(
            select(WorkflowModel).order_by(desc(WorkflowModel.created_at))
        ).scalars().all()
        return [
            {
                "id": r.id,
                "kind": r.kind,
                "status": r.status,
                "title": r.title,
                "source": r.source,
                "attempts": r.attempts,
                "task_count": len(r.tasks),
                "created": str(r.created_at),
            }
            for r in rows
        ]

    @staticmethod
    def get_by_status(session: Session, status: str) -> list[dict]:
        rows = session.execute(
            select(WorkflowModel).where(
                WorkflowModel.status == status
            ).order_by(desc(WorkflowModel.created_at))
        ).scalars().all()
        return [
            {
                "id": r.id,
                "kind": r.kind,
                "title": r.title,
                "attempts": r.attempts,
                "created": str(r.created_at),
            }
            for r in rows
        ]

    @staticmethod
    def get_summary(session: Session) -> dict:
        total = session.execute(
            select(func.count()).select_from(WorkflowModel)
        ).scalar() or 0
        by_status = session.execute(
            select(
                WorkflowModel.status,
                func.count().label("count"),
            ).group_by(WorkflowModel.status)
        ).all()
        pending_approvals = session.execute(
            select(func.count()).select_from(HumanApprovalModel).where(
                HumanApprovalModel.status == "open"
            )
        ).scalar() or 0
        open_escalations = session.execute(
            select(func.count()).select_from(EscalationModel).where(
                EscalationModel.resolved_at.is_(None)
            )
        ).scalar() or 0
        return {
            "total_workflows": total,
            "by_status": {r.status: r.count for r in by_status},
            "pending_approvals": pending_approvals,
            "open_escalations": open_escalations,
        }


class AgentTaskRepository:
    @staticmethod
    def get_by_workflow(session: Session, workflow_id: str) -> list[dict]:
        rows = session.execute(
            select(AgentTaskModel).where(
                AgentTaskModel.workflow_id == workflow_id
            ).order_by(AgentTaskModel.created_at)
        ).scalars().all()
        return [
            {
                "id": r.id,
                "agent": r.agent_name,
                "tool": r.tool_name,
                "status": r.status,
                "started": str(r.started_at) if r.started_at else None,
                "completed": str(r.completed_at) if r.completed_at else None,
                "error": r.error,
            }
            for r in rows
        ]


class LeadRepository:
    @staticmethod
    def get_top_scored(session: Session, limit: int = 20) -> list[dict]:
        rows = session.execute(
            select(LeadModel).order_by(desc(LeadModel.score)).limit(limit)
        ).scalars().all()
        return [
            {
                "id": r.id,
                "name": r.name,
                "email": r.email,
                "company": r.company,
                "source": r.source,
                "score": float(r.score),
                "created": str(r.created_at),
            }
            for r in rows
        ]

    @staticmethod
    def get_stats(session: Session) -> dict:
        total = session.execute(
            select(func.count()).select_from(LeadModel)
        ).scalar() or 0
        avg_score = session.execute(
            select(func.avg(LeadModel.score)).select_from(LeadModel)
        ).scalar() or 0
        by_source = session.execute(
            select(
                LeadModel.source,
                func.count().label("count"),
                func.avg(LeadModel.score).label("avg_score"),
            ).group_by(LeadModel.source)
        ).all()
        return {
            "total_leads": total,
            "avg_score": round(float(avg_score), 2),
            "by_source": [
                {"source": r.source, "count": r.count,
                 "avg_score": round(float(r.avg_score or 0), 2)}
                for r in by_source
            ],
        }


class AuditLogRepository:
    @staticmethod
    def get_recent(session: Session, limit: int = 50) -> list[dict]:
        rows = session.execute(
            select(AuditLogModel).order_by(desc(AuditLogModel.created_at)).limit(limit)
        ).scalars().all()
        return [
            {
                "action": r.action,
                "actor": r.actor,
                "message": r.message,
                "workflow_id": r.workflow_id,
                "timestamp": str(r.created_at),
            }
            for r in rows
        ]


# ---------------------------------------------------------------------------
# Health Check
# ---------------------------------------------------------------------------

def health_check() -> dict:
    """Verify CockroachDB connection and return cluster info."""
    session = get_session()
    try:
        row = session.execute(func.current_timestamp()).scalar()
        wf_count = session.execute(
            select(func.count()).select_from(WorkflowModel)
        ).scalar()
        task_count = session.execute(
            select(func.count()).select_from(AgentTaskModel)
        ).scalar()
        lead_count = session.execute(
            select(func.count()).select_from(LeadModel)
        ).scalar()
        approval_count = session.execute(
            select(func.count()).select_from(HumanApprovalModel)
        ).scalar()
        return {
            "status": "ok",
            "connected": True,
            "server_time": str(row),
            "workflows": wf_count,
            "agent_tasks": task_count,
            "leads": lead_count,
            "pending_approvals": approval_count,
            "backend": "CockroachDB",
        }
    except Exception as e:
        return {"status": "error", "connected": False, "error": str(e)}
    finally:
        session.close()


# ---------------------------------------------------------------------------
# Schema creation
# ---------------------------------------------------------------------------

def create_tables():
    """Create all tables in CockroachDB."""
    Base.metadata.create_all(bind=engine)
    logger.info("All tables created successfully")


if __name__ == "__main__":
    create_tables()
    print("Tables created.")
    print(health_check())
