from contextlib import asynccontextmanager

import structlog
from fastapi import FastAPI, Request
from fastapi.staticfiles import StaticFiles

from app.api.routes_admin import router as admin_router
from app.api.routes_agents import router as agents_router
from app.api.routes_health import REQUEST_COUNTER, router as health_router
from app.api.routes_webhooks import router as webhooks_router
from app.config import get_settings
from app.db import init_db
from app.logging_config import configure_logging
from app.services.scheduler import start_scheduler

settings = get_settings()
configure_logging(settings.log_level)
log = structlog.get_logger()


@asynccontextmanager
async def lifespan(app: FastAPI):
    init_db()
    scheduler = start_scheduler()
    log.info("business_os_started", environment=settings.environment)
    yield
    scheduler.shutdown(wait=False)
    log.info("business_os_stopped")


app = FastAPI(title=settings.app_name, version="0.1.0", lifespan=lifespan)
app.mount("/static", StaticFiles(directory="app/static"), name="static")


@app.middleware("http")
async def metrics_middleware(request: Request, call_next):
    REQUEST_COUNTER.labels(path=request.url.path).inc()
    response = await call_next(request)
    return response


app.include_router(health_router)
app.include_router(agents_router)
app.include_router(webhooks_router)
app.include_router(admin_router)


@app.get("/")
def root() -> dict[str, str]:
    return {
        "name": settings.app_name,
        "admin": "/admin",
        "docs": "/docs",
        "health": "/health",
    }
