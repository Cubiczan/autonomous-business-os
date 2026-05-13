from app.integrations.base import IntegrationClient, IntegrationResult


class DocuSignClient(IntegrationClient):
    provider = "docusign"

    def __init__(self, token: str | None):
        super().__init__(token=token, base_url="https://demo.docusign.net/restapi")

    def get_envelope(self, envelope_id: str) -> IntegrationResult:
        if not self.configured:
            return self.simulated("get_envelope", {"envelope_id": envelope_id, "status": "completed"})
        data = self._request("GET", f"/v2.1/accounts/me/envelopes/{envelope_id}")
        return IntegrationResult(ok=True, provider=self.provider, action="get_envelope", data=data)
