from typing import Any

from app.integrations.base import IntegrationClient, IntegrationResult


class TaskManagementClient(IntegrationClient):
    def __init__(self, provider: str, linear_key: str | None, jira_token: str | None, jira_url: str | None):
        self.provider = provider
        token = linear_key if provider == "linear" else jira_token
        base_url = "https://api.linear.app/graphql" if provider == "linear" else jira_url
        super().__init__(token=token, base_url=base_url)

    def create_task(self, title: str, description: str, metadata: dict[str, Any]) -> IntegrationResult:
        if not self.configured:
            return self.simulated(
                "create_task",
                {"title": title, "description": description, "metadata": metadata},
            )
        return IntegrationResult(
            ok=True,
            provider=self.provider,
            action="create_task",
            data={"title": title, "description": description, "metadata": metadata},
        )
