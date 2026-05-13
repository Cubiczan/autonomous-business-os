import hmac
import time
from hashlib import sha256

from fastapi import Header, HTTPException, Request, status

from app.config import get_settings


def require_admin_api_key(x_admin_api_key: str | None = Header(default=None)) -> None:
    settings = get_settings()
    if not x_admin_api_key or not hmac.compare_digest(x_admin_api_key, settings.admin_api_key):
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Missing or invalid admin API key",
        )


async def verify_slack_signature(request: Request, body: bytes) -> bool:
    settings = get_settings()
    if not settings.slack_signing_secret:
        return True
    timestamp = request.headers.get("x-slack-request-timestamp")
    signature = request.headers.get("x-slack-signature")
    if not timestamp or not signature:
        return False
    if abs(time.time() - int(timestamp)) > 60 * 5:
        return False
    base = f"v0:{timestamp}:{body.decode('utf-8')}".encode("utf-8")
    digest = hmac.new(settings.slack_signing_secret.encode("utf-8"), base, sha256).hexdigest()
    return hmac.compare_digest(f"v0={digest}", signature)


def verify_shared_secret(header_value: str | None, configured_secret: str | None) -> bool:
    if not configured_secret:
        return True
    return bool(header_value and hmac.compare_digest(header_value, configured_secret))
