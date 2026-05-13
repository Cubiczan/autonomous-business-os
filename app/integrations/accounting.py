from app.integrations.base import IntegrationClient, IntegrationResult


class AccountingClient(IntegrationClient):
    def __init__(self, provider: str, token: str | None):
        self.provider = provider
        base_url = {
            "quickbooks": "https://quickbooks.api.intuit.com",
            "xero": "https://api.xero.com",
        }.get(provider, "https://quickbooks.api.intuit.com")
        super().__init__(token=token, base_url=base_url)

    def reconcile_payments(self) -> IntegrationResult:
        if not self.configured:
            return self.simulated(
                "reconcile_payments",
                {"matched": 12, "unmatched": 1, "variance_cents": 0},
            )
        return IntegrationResult(
            ok=True,
            provider=self.provider,
            action="reconcile_payments",
            data={"status": "queued"},
        )
