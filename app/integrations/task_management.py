from typing import Any

from airbyte_agent_sdk.connectors.jira import JiraConnector
from airbyte_agent_sdk.connectors.jira.models import JiraJiraApiTokenAuthenticationAuthConfig
from airbyte_agent_sdk.connectors.linear import LinearConnector
from airbyte_agent_sdk.connectors.linear.models import LinearLinearApiKeyAuthenticationAuthConfig

from app.config import get_settings
from app.integrations.airbyte_runtime import airbyte_hosted_configured, execute_operation
from app.integrations.base import IntegrationClient, IntegrationResult


class TaskManagementClient(IntegrationClient):
    _SUPPORTED_PROVIDERS = {"linear", "jira"}

    def __init__(self, provider: str, linear_key: str | None, jira_token: str | None, jira_url: str | None):
        if provider not in self._SUPPORTED_PROVIDERS:
            raise ValueError(f"Unsupported task provider: {provider}")
        self.provider = provider
        self.linear_key = linear_key
        self.jira_token = jira_token
        self.jira_url = jira_url
        self._settings = get_settings()
        token = linear_key if provider == "linear" else jira_token
        super().__init__(token=token, base_url=jira_url if provider == "jira" else "https://api.linear.app")

    @property
    def configured(self) -> bool:
        if airbyte_hosted_configured(self._settings):
            return True
        if self.provider == "linear":
            return bool(self.linear_key)
        return bool(self.jira_token and self.jira_url)

    def create_task(self, title: str, description: str, metadata: dict[str, Any]) -> IntegrationResult:
        if not self.configured:
            return self.simulated(
                "create_task",
                {"title": title, "description": description, "metadata": metadata},
            )

        if self.provider == "linear":
            return self._create_linear_task(title, description, metadata)
        return self._create_jira_task(title, description, metadata)

    def _create_linear_task(
        self,
        title: str,
        description: str,
        metadata: dict[str, Any],
    ) -> IntegrationResult:
        team_id = metadata.get("team_id") or self._settings.linear_team_id
        if not team_id:
            teams = execute_operation(
                "linear",
                "teams",
                "list",
                {"first": 1},
                local_auth=self._linear_local_auth(),
                connector_cls=LinearConnector if self._linear_local_auth() else None,
                settings=self._settings,
            )
            nodes = teams.get("nodes") or teams.get("items") or []
            if nodes and isinstance(nodes[0], dict):
                team_id = nodes[0].get("id")
        if not team_id:
            return self.simulated(
                "create_task",
                {
                    "title": title,
                    "description": description,
                    "metadata": metadata,
                    "note": "Set LINEAR_TEAM_ID or configure a Linear team in Airbyte.",
                },
            )

        params: dict[str, Any] = {"team_id": team_id, "title": title}
        if description:
            params["description"] = description
        if project_id := metadata.get("project_id"):
            params["project_id"] = project_id

        data = execute_operation(
            "linear",
            "issues",
            "create",
            params,
            local_auth=self._linear_local_auth(),
            connector_cls=LinearConnector if self._linear_local_auth() else None,
            settings=self._settings,
        )
        return IntegrationResult(ok=True, provider=self.provider, action="create_task", data=data)

    def _create_jira_task(
        self,
        title: str,
        description: str,
        metadata: dict[str, Any],
    ) -> IntegrationResult:
        project_key = metadata.get("project_key") or self._settings.jira_project_key
        if not project_key:
            return self.simulated(
                "create_task",
                {
                    "title": title,
                    "description": description,
                    "metadata": metadata,
                    "note": "Set JIRA_PROJECT_KEY or pass metadata.project_key.",
                },
            )

        issue_type = metadata.get("issue_type") or self._settings.jira_issue_type
        fields: dict[str, Any] = {
            "project": {"key": project_key},
            "issuetype": {"name": issue_type},
            "summary": title,
        }
        if description:
            fields["description"] = {
                "type": "doc",
                "version": 1,
                "content": [
                    {
                        "type": "paragraph",
                        "content": [{"type": "text", "text": description}],
                    }
                ],
            }

        data = execute_operation(
            "jira",
            "issues",
            "create",
            {"fields": fields},
            local_auth=self._jira_local_auth(),
            connector_cls=JiraConnector if self._jira_local_auth() else None,
            settings=self._settings,
        )
        return IntegrationResult(ok=True, provider=self.provider, action="create_task", data=data)

    def _linear_local_auth(self) -> LinearLinearApiKeyAuthenticationAuthConfig | None:
        if self.linear_key and not airbyte_hosted_configured(self._settings):
            return LinearLinearApiKeyAuthenticationAuthConfig(api_key=self.linear_key)
        return None

    def _jira_local_auth(self) -> JiraJiraApiTokenAuthenticationAuthConfig | None:
        if self.jira_token and self._settings.jira_email and not airbyte_hosted_configured(self._settings):
            return JiraJiraApiTokenAuthenticationAuthConfig(
                username=self._settings.jira_email,
                password=self.jira_token,
            )
        return None
