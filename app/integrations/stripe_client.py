from typing import Any

from airbyte_agent_sdk.connectors.stripe import StripeConnector
from airbyte_agent_sdk.connectors.stripe.models import StripeAuthConfig

from app.config import get_settings
from app.integrations.airbyte_runtime import airbyte_hosted_configured, execute_operation
from app.integrations.base import IntegrationClient, IntegrationResult


class StripeClient(IntegrationClient):
    provider = "stripe"

    def __init__(self, token: str | None):
        super().__init__(token=token, base_url="https://api.stripe.com/v1")
        self._settings = get_settings()

    @property
    def configured(self) -> bool:
        return bool(airbyte_hosted_configured(self._settings) or self.token)

    def create_invoice(self, invoice: dict[str, Any]) -> IntegrationResult:
        if not self.configured:
            return self.simulated("create_invoice", {"invoice_id": "sim-invoice", **invoice})

        params: dict[str, Any] = {}
        if customer_id := invoice.get("customer_id"):
            params["customer"] = customer_id

        local_auth = (
            StripeAuthConfig(api_key=self.token)
            if self.token and not airbyte_hosted_configured(self._settings)
            else None
        )
        data = execute_operation(
            "stripe",
            "invoices",
            "create",
            params,
            local_auth=local_auth,
            connector_cls=StripeConnector if local_auth else None,
            settings=self._settings,
        )
        return IntegrationResult(
            ok=True,
            provider=self.provider,
            action="create_invoice",
            data={"status": "created", **invoice, **data},
        )

    def list_overdue_invoices(self) -> IntegrationResult:
        if not self.configured:
            return self.simulated(
                "list_overdue_invoices",
                {"items": [{"invoice_id": "sim-overdue-1", "amount_cents": 125000}]},
            )

        local_auth = (
            StripeAuthConfig(api_key=self.token)
            if self.token and not airbyte_hosted_configured(self._settings)
            else None
        )
        data = execute_operation(
            "stripe",
            "invoices",
            "list",
            {"status": "open", "limit": 100},
            local_auth=local_auth,
            connector_cls=StripeConnector if local_auth else None,
            settings=self._settings,
        )
        items = []
        for invoice in data.get("data", []):
            if not isinstance(invoice, dict):
                continue
            items.append(
                {
                    "invoice_id": invoice.get("id"),
                    "amount_cents": invoice.get("amount_due") or invoice.get("total") or 0,
                    "customer_id": invoice.get("customer"),
                    "status": invoice.get("status"),
                }
            )
        return IntegrationResult(
            ok=True,
            provider=self.provider,
            action="list_overdue_invoices",
            data={"items": items, "raw": data},
        )
