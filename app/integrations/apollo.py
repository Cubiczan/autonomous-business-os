from app.integrations.base import IntegrationClient, IntegrationResult


class ApolloClient(IntegrationClient):
    provider = "apollo"

    def __init__(self, token: str | None):
        super().__init__(token=token, base_url="https://api.apollo.io/v1")

    def enrich_person(self, email: str) -> IntegrationResult:
        if not self.configured:
            return self.simulated(
                "enrich_person",
                {
                    "email": email,
                    "title": "Founder",
                    "employee_count": 45,
                    "annual_revenue": 2_500_000,
                    "industry": "SaaS",
                },
            )
        data = self._request("POST", "/people/match", json={"api_key": self.token, "email": email})
        return IntegrationResult(ok=True, provider=self.provider, action="enrich_person", data=data)
