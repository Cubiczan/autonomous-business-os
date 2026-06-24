from typing import Any

from airbyte_agent_sdk.connectors.hubspot import HubspotConnector
from airbyte_agent_sdk.connectors.hubspot.models import HubspotPrivateAppAuthConfig
from airbyte_agent_sdk.connectors.salesforce import SalesforceConnector
from airbyte_agent_sdk.connectors.salesforce.models import SalesforceAuthConfig

from app.config import get_settings
from app.integrations.airbyte_runtime import airbyte_hosted_configured, execute_operation
from app.integrations.base import IntegrationClient, IntegrationResult


class CRMClient(IntegrationClient):
    _SUPPORTED_PROVIDERS = {"hubspot", "salesforce"}

    def __init__(self, provider: str, token: str | None):
        if provider not in self._SUPPORTED_PROVIDERS:
            raise ValueError(f"Unsupported CRM provider: {provider}")
        self.provider = provider
        self.token = token
        self._settings = get_settings()
        super().__init__(token=token, base_url=None)

    @property
    def configured(self) -> bool:
        return bool(airbyte_hosted_configured(self._settings) or self.token)

    def upsert_lead(self, lead: dict[str, Any]) -> IntegrationResult:
        if not self.configured:
            return self.simulated("upsert_lead", {"external_id": f"sim-{lead.get('email')}"})

        if self.provider == "salesforce":
            return self._upsert_salesforce_lead(lead)
        return self._upsert_hubspot_lead(lead)

    def _upsert_salesforce_lead(self, lead: dict[str, Any]) -> IntegrationResult:
        name = lead.get("name") or ""
        parts = name.split(" ", 1)
        params: dict[str, Any] = {
            "last_name": parts[-1] if name else "Unknown",
            "company": lead.get("company") or "Unknown",
        }
        if len(parts) > 1:
            params["first_name"] = parts[0]
        if email := lead.get("email"):
            params["email"] = email
        if score := lead.get("score"):
            params["description"] = f"ABO lead score: {score}"

        local_auth = (
            SalesforceAuthConfig(refresh_token=self.token)
            if self.token and not airbyte_hosted_configured(self._settings)
            else None
        )
        data = execute_operation(
            "salesforce",
            "leads",
            "create",
            params,
            local_auth=local_auth,
            connector_cls=SalesforceConnector if local_auth else None,
            settings=self._settings,
        )
        return IntegrationResult(ok=True, provider=self.provider, action="upsert_lead", data=data)

    def _upsert_hubspot_lead(self, lead: dict[str, Any]) -> IntegrationResult:
        email = lead.get("email")
        local_auth = (
            HubspotPrivateAppAuthConfig(private_app_token=self.token)
            if self.token and not airbyte_hosted_configured(self._settings)
            else None
        )
        if email:
            search = execute_operation(
                "hubspot",
                "contacts",
                "api_search",
                {
                    "filter_groups": [
                        {
                            "filters": [
                                {
                                    "propertyName": "email",
                                    "operator": "EQ",
                                    "value": email,
                                }
                            ]
                        }
                    ],
                    "limit": 1,
                },
                local_auth=local_auth,
                connector_cls=HubspotConnector if local_auth else None,
                settings=self._settings,
            )
            results = search.get("results") or search.get("items") or []
            if results:
                return IntegrationResult(
                    ok=True,
                    provider=self.provider,
                    action="upsert_lead",
                    data=results[0],
                )

        return self.simulated(
            "upsert_lead",
            {
                "external_id": f"sim-{email}",
                "create_unsupported": True,
                "note": (
                    "HubSpot agent connector supports contact search but not create; "
                    "returning simulated upsert for new leads."
                ),
            },
        )

    def update_deal_stage(self, deal_id: str, stage: str) -> IntegrationResult:
        if not self.configured:
            return self.simulated("update_deal_stage", {"deal_id": deal_id, "stage": stage})
        return IntegrationResult(
            ok=True,
            provider=self.provider,
            action="update_deal_stage",
            data={"deal_id": deal_id, "stage": stage},
        )
