import signal
import time

import structlog

from app.config import get_settings
from app.db import init_db
from app.logging_config import configure_logging
from app.services.scheduler import process_pending_workflows

settings = get_settings()
configure_logging(settings.log_level)
log = structlog.get_logger()
running = True


def stop(*_: object) -> None:
    global running
    running = False


def main() -> None:
    init_db()
    signal.signal(signal.SIGINT, stop)
    signal.signal(signal.SIGTERM, stop)
    log.info("worker_started")
    while running:
        process_pending_workflows()
        time.sleep(20)
    log.info("worker_stopped")


if __name__ == "__main__":
    main()
