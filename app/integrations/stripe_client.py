from typing import Any

from app.integrations.base import IntegrationClient, IntegrationResult


class StripeClient(IntegrationClient):
    provider = "stripe"

    def __init__(self, token: str | None):
        super().__init__(token=token, base_url="https://api.stripe.com/v1")

    def create_invoice(self, invoice: dict[str, Any]) -> IntegrationResult:
        if not self.configured:
            return self.simulated("create_invoice", {"invoice_id": "sim-invoice", **invoice})
        return IntegrationResult(
            ok=True,
            provider=self.provider,
            action="create_invoice",
            data={"status": "created", **invoice},
        )

    def list_overdue_invoices(self) -> IntegrationResult:
        if not self.configured:
            return self.simulated(
                "list_overdue_invoices",
                {"items": [{"invoice_id": "sim-overdue-1", "amount_cents": 125000}]},
            )
        data = self._request("GET", "/invoices", params={"status": "open"})
        return IntegrationResult(ok=True, provider=self.provider, action="list_overdue_invoices", data=data)
