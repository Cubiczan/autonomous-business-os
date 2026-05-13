from typing import Any

from app.integrations.base import IntegrationClient, IntegrationResult


class NotionClient(IntegrationClient):
    provider = "notion"

    def __init__(self, token: str | None, database_id: str | None):
        super().__init__(token=token, base_url="https://api.notion.com/v1")
        self.database_id = database_id

    def create_page(self, title: str, properties: dict[str, Any]) -> IntegrationResult:
        if not self.configured or not self.database_id:
            return self.simulated("create_page", {"title": title, "properties": properties})
        data = self._request(
            "POST",
            "/pages",
            json={"parent": {"database_id": self.database_id}, "properties": properties},
        )
        return IntegrationResult(ok=True, provider=self.provider, action="create_page", data=data)
