from fastapi import APIRouter
from prometheus_client import CONTENT_TYPE_LATEST, Counter, Gauge, generate_latest
from sqlalchemy import text
from starlette.responses import Response

from app.db import SessionLocal

router = APIRouter()

REQUEST_COUNTER = Counter("business_os_requests_total", "Total HTTP requests", ["path"])
WORKFLOW_GAUGE = Gauge("business_os_workflows_known", "Known workflows in durable state")


@router.get("/health")
def health() -> dict[str, str]:
    return {"status": "ok"}


@router.get("/ready")
def ready() -> dict[str, str]:
    with SessionLocal() as session:
        session.execute(text("select 1"))
    return {"status": "ready"}


@router.get("/metrics")
def metrics() -> Response:
    return Response(generate_latest(), media_type=CONTENT_TYPE_LATEST)
