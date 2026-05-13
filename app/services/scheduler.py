import structlog
from apscheduler.schedulers.background import BackgroundScheduler
from sqlalchemy import select

from app.db import SessionLocal
from app.models import Workflow, WorkflowStatus

log = structlog.get_logger()


def process_pending_workflows() -> None:
    from app.agents.orchestrator import MasterOrchestrator

    with SessionLocal() as session:
        workflows = session.scalars(
            select(Workflow)
            .where(Workflow.status == WorkflowStatus.pending)
            .order_by(Workflow.created_at)
            .limit(10)
        ).all()
        orchestrator = MasterOrchestrator(session)
        for workflow in workflows:
            try:
                orchestrator.run_workflow(workflow)
            except Exception as exc:  # pragma: no cover - last line defense for scheduler
                log.exception("scheduled_workflow_failed", workflow_id=workflow.id, error=str(exc))


def start_scheduler() -> BackgroundScheduler:
    scheduler = BackgroundScheduler(timezone="UTC")
    scheduler.add_job(process_pending_workflows, "interval", seconds=20, id="pending-workflows")
    scheduler.start()
    return scheduler
