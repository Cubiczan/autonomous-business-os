from app.integrations.base import IntegrationClient, IntegrationResult


class SlackClient(IntegrationClient):
    provider = "slack"

    def __init__(self, token: str | None):
        super().__init__(token=token, base_url="https://slack.com/api")

    def post_message(self, channel: str, text: str) -> IntegrationResult:
        if not self.configured:
            return self.simulated("post_message", {"channel": channel, "text": text})
        data = self._request("POST", "/chat.postMessage", json={"channel": channel, "text": text})
        return IntegrationResult(ok=True, provider=self.provider, action="post_message", data=data)
