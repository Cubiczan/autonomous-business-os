from dataclasses import dataclass, field
from typing import Any

import httpx
from tenacity import retry, stop_after_attempt, wait_exponential


@dataclass
class IntegrationResult:
    ok: bool
    provider: str
    action: str
    data: dict[str, Any] = field(default_factory=dict)
    error: str | None = None
    simulated: bool = False


class IntegrationClient:
    provider = "generic"

    def __init__(self, token: str | None = None, base_url: str | None = None):
        self.token = token
        self.base_url = base_url

    @property
    def configured(self) -> bool:
        return bool(self.token)

    def simulated(self, action: str, data: dict[str, Any] | None = None) -> IntegrationResult:
        return IntegrationResult(
            ok=True,
            provider=self.provider,
            action=action,
            data=data or {},
            simulated=True,
        )

    @retry(wait=wait_exponential(multiplier=1, min=1, max=8), stop=stop_after_attempt(3))
    def _request(
        self,
        method: str,
        path: str,
        *,
        json: dict[str, Any] | None = None,
        params: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        if not self.base_url:
            raise RuntimeError(f"{self.provider} base URL is not configured")
        headers = {"Authorization": f"Bearer {self.token}"} if self.token else {}
        with httpx.Client(timeout=20) as client:
            response = client.request(
                method,
                f"{self.base_url.rstrip('/')}/{path.lstrip('/')}",
                json=json,
                params=params,
                headers=headers,
            )
            response.raise_for_status()
            if not response.content:
                return {}
            return response.json()
