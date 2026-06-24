from airbyte_agent_sdk.connectors.slack import SlackConnector
from airbyte_agent_sdk.connectors.slack.models import SlackTokenAuthenticationAuthConfig

from app.config import get_settings
from app.integrations.airbyte_runtime import airbyte_hosted_configured, execute_operation
from app.integrations.base import IntegrationClient, IntegrationResult


class SlackClient(IntegrationClient):
    provider = "slack"

    def __init__(self, token: str | None):
        super().__init__(token=token, base_url="https://slack.com/api")
        self._settings = get_settings()

    @property
    def configured(self) -> bool:
        return bool(airbyte_hosted_configured(self._settings) or self.token)

    def post_message(self, channel: str, text: str) -> IntegrationResult:
        if not self.configured:
            return self.simulated("post_message", {"channel": channel, "text": text})

        local_auth = (
            SlackTokenAuthenticationAuthConfig(bot_key=self.token)
            if self.token and not airbyte_hosted_configured(self._settings)
            else None
        )
        data = execute_operation(
            "slack",
            "messages",
            "create",
            {"channel": channel, "text": text},
            local_auth=local_auth,
            connector_cls=SlackConnector if local_auth else None,
            settings=self._settings,
        )
        return IntegrationResult(ok=True, provider=self.provider, action="post_message", data=data)
