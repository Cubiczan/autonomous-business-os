from datetime import timedelta

import structlog
from apscheduler.schedulers.background import BackgroundScheduler
from sqlalchemy import select

from app.db import SessionLocal
from app.models import (
    AuditAction,
    Department,
    DepartmentSchedule,
    DepartmentStatus,
    Workflow,
    WorkflowStatus,
    utcnow,
)
from app.services.audit import AuditService

log = structlog.get_logger()


def process_pending_workflows() -> None:
    from app.agents.orchestrator import MasterOrchestrator

    with SessionLocal() as session:
        claimed = _claim_pending_workflows(session, limit=10)
        orchestrator = MasterOrchestrator(session)
        for workflow in claimed:
            try:
                orchestrator.run_workflow(workflow)
            except Exception as exc:  # pragma: no cover - last line defense for scheduler
                log.exception("scheduled_workflow_failed", workflow_id=workflow.id, error=str(exc))


def _claim_pending_workflows(session, limit: int) -> list[Workflow]:
    """Atomically claim pending workflows so two worker processes never
    dispatch the same row twice (which would create duplicate invoices, CRM
    writes and outbound emails).

    On Postgres / CockroachDB we use ``SELECT ... FOR UPDATE SKIP LOCKED`` and
    flip the status to ``running`` inside the same transaction before any
    worker yields to the orchestrator. SQLite does not support row locking, so
    we rely on its single-writer transaction semantics instead.
    """
    dialect = session.bind.dialect.name if session.bind is not None else ""
    stmt = (
        select(Workflow)
        .where(Workflow.status == WorkflowStatus.pending)
        .order_by(Workflow.created_at)
        .limit(limit)
    )
    if dialect != "sqlite":
        stmt = stmt.with_for_update(skip_locked=True)

    workflows = session.scalars(stmt).all()
    if not workflows:
        return []

    audit = AuditService(session)
    now = utcnow()
    for workflow in workflows:
        # Claim inside the same transaction that holds the row lock so a
        # concurrent worker observes a non-pending status (or is blocked /
        # skips the locked row) and cannot re-fetch the same workflow.
        workflow.status = WorkflowStatus.running
        workflow.attempts += 1
        workflow.updated_at = now
    session.commit()
    for workflow in workflows:
        audit.record(
            AuditAction.workflow_started,
            f"Workflow started: {workflow.title}",
            workflow_id=workflow.id,
        )
    return list(workflows)


def process_due_department_schedules() -> None:
    from app.agents.orchestrator import MasterOrchestrator
    from app.services.departments import DepartmentFactory
    from app.services.workflows import WorkflowService

    now = utcnow()
    with SessionLocal() as session:
        DepartmentFactory(session).ensure_company_layer()
        schedules = session.scalars(
            select(DepartmentSchedule)
            .join(Department)
            .where(
                DepartmentSchedule.enabled.is_(True),
                DepartmentSchedule.next_run_at.is_not(None),
                DepartmentSchedule.next_run_at <= now,
                Department.status == DepartmentStatus.active,
            )
            .order_by(DepartmentSchedule.next_run_at)
            .limit(10)
        ).all()
        workflow_service = WorkflowService(session)
        orchestrator = MasterOrchestrator(session)
        for schedule in schedules:
            department = session.get(Department, schedule.department_id)
            if not department:
                continue
            payload = dict(schedule.payload_template or {})
            payload.setdefault("department_id", department.id)
            payload.setdefault("department_type", department.department_type)
            workflow = workflow_service.create(
                schedule.workflow_kind,
                f"{department.name}: {schedule.name}",
                payload,
                source=f"schedule:{schedule.id}",
            )
            schedule.last_run_at = now
            schedule.next_run_at = _next_run(now, schedule.cadence)
            schedule.updated_at = now
            session.commit()
            try:
                orchestrator.run_workflow(workflow)
            except Exception as exc:  # pragma: no cover - scheduler protection
                log.exception(
                    "department_schedule_failed",
                    department_id=department.id,
                    schedule_id=schedule.id,
                    workflow_id=workflow.id,
                    error=str(exc),
                )


def start_scheduler() -> BackgroundScheduler:
    scheduler = BackgroundScheduler(timezone="UTC")
    scheduler.add_job(process_pending_workflows, "interval", seconds=20, id="pending-workflows")
    scheduler.add_job(
        process_due_department_schedules,
        "interval",
        seconds=60,
        id="department-schedules",
    )
    scheduler.start()
    return scheduler


def _next_run(now, cadence: str):
    if cadence == "hourly":
        return now + timedelta(hours=1)
    if cadence == "weekly":
        return now + timedelta(days=7)
    if cadence == "business_daily":
        return now + timedelta(days=1)
    return now + timedelta(days=1)
