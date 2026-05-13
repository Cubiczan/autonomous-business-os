from typing import Any

from app.integrations.base import IntegrationClient, IntegrationResult


class CRMClient(IntegrationClient):
    def __init__(self, provider: str, token: str | None):
        base_url = {
            "hubspot": "https://api.hubapi.com",
            "salesforce": "https://login.salesforce.com",
        }.get(provider, "https://api.hubapi.com")
        self.provider = provider
        super().__init__(token=token, base_url=base_url)

    def upsert_lead(self, lead: dict[str, Any]) -> IntegrationResult:
        if not self.configured:
            return self.simulated("upsert_lead", {"external_id": f"sim-{lead.get('email')}"})
        if self.provider == "hubspot":
            data = self._request(
                "POST",
                "/crm/v3/objects/contacts",
                json={
                    "properties": {
                        "email": lead.get("email"),
                        "firstname": lead.get("name", "").split(" ")[0],
                        "company": lead.get("company"),
                        "lifecyclestage": "lead",
                    }
                },
            )
        else:
            data = self._request("POST", "/services/data/v60.0/sobjects/Lead", json=lead)
        return IntegrationResult(ok=True, provider=self.provider, action="upsert_lead", data=data)

    def update_deal_stage(self, deal_id: str, stage: str) -> IntegrationResult:
        if not self.configured:
            return self.simulated("update_deal_stage", {"deal_id": deal_id, "stage": stage})
        return IntegrationResult(
            ok=True,
            provider=self.provider,
            action="update_deal_stage",
            data={"deal_id": deal_id, "stage": stage},
        )
