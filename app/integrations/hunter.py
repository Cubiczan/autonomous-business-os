from app.integrations.base import IntegrationClient, IntegrationResult


class HunterClient(IntegrationClient):
    provider = "hunter"

    def __init__(self, token: str | None):
        super().__init__(token=token, base_url="https://api.hunter.io/v2")

    def verify_email(self, email: str) -> IntegrationResult:
        if not self.configured:
            return self.simulated("verify_email", {"email": email, "email_confidence": 85})
        data = self._request(
            "GET",
            "/email-verifier",
            params={"email": email, "api_key": self.token},
        )
        return IntegrationResult(ok=True, provider=self.provider, action="verify_email", data=data)
