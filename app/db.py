import logging
import os
from collections.abc import Generator

from sqlalchemy import create_engine
from sqlalchemy.orm import DeclarativeBase, Session, sessionmaker

from app.config import get_settings

logger = logging.getLogger("app.db")


class Base(DeclarativeBase):
    pass


settings = get_settings()

USE_COCKROACH = os.getenv("USE_COCKROACHDB", "false").lower() in ("true", "1", "yes")

if USE_COCKROACH:
    cockroach_url = os.getenv("ABOS_DATABASE_URL")
    if not cockroach_url:
        raise RuntimeError("USE_COCKROACHDB is enabled, but ABOS_DATABASE_URL is not set")
    logger.info("Using CockroachDB backend")
    _url = cockroach_url
    _connect_args = {}
    _pool_options = {"pool_size": 10, "max_overflow": 5}
else:
    _url = settings.database_url
    _connect_args = {"check_same_thread": False} if _url.startswith("sqlite") else {}
    _pool_options = {}

engine = create_engine(
    _url,
    connect_args=_connect_args,
    pool_pre_ping=True,
    echo=False,
    **_pool_options,
)
SessionLocal = sessionmaker(bind=engine, autoflush=False, autocommit=False, expire_on_commit=False)


def init_db() -> None:
    from app import models  # noqa: F401

    if not USE_COCKROACH:
        settings.storage_dir.mkdir(parents=True, exist_ok=True)
    Base.metadata.create_all(bind=engine)
    backend = "CockroachDB" if USE_COCKROACH else "SQLite"
    logger.info(f"Database initialized: {backend}")


def get_session() -> Generator[Session, None, None]:
    session = SessionLocal()
    try:
        yield session
    finally:
        session.close()
