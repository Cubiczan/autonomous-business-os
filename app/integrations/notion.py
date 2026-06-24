from typing import Any

from airbyte_agent_sdk.connectors.notion import NotionConnector
from airbyte_agent_sdk.connectors.notion.models import NotionAccessTokenAuthConfig

from app.config import get_settings
from app.integrations.airbyte_runtime import airbyte_hosted_configured, execute_operation
from app.integrations.base import IntegrationClient, IntegrationResult


class NotionClient(IntegrationClient):
    provider = "notion"

    def __init__(self, token: str | None, database_id: str | None):
        super().__init__(token=token, base_url="https://api.notion.com/v1")
        self.database_id = database_id
        self._settings = get_settings()

    @property
    def configured(self) -> bool:
        return bool(
            (airbyte_hosted_configured(self._settings) or self.token)
            and self.database_id
        )

    def create_page(self, title: str, properties: dict[str, Any]) -> IntegrationResult:
        if not self.configured:
            return self.simulated("create_page", {"title": title, "properties": properties})

        params = {
            "parent": {"database_id": self.database_id},
            "properties": properties or {"Name": {"title": [{"text": {"content": title}}]}},
        }
        local_auth = (
            NotionAccessTokenAuthConfig(token=self.token)
            if self.token and not airbyte_hosted_configured(self._settings)
            else None
        )
        data = execute_operation(
            "notion",
            "pages",
            "create",
            params,
            local_auth=local_auth,
            connector_cls=NotionConnector if local_auth else None,
            settings=self._settings,
        )
        return IntegrationResult(ok=True, provider=self.provider, action="create_page", data=data)
