from typing import Any

from app.integrations.base import IntegrationClient, IntegrationResult


class CalendarClient(IntegrationClient):
    def __init__(self, provider: str, token: str | None):
        self.provider = provider
        base_url = "https://www.googleapis.com/calendar/v3" if provider == "google" else "https://graph.microsoft.com/v1.0"
        super().__init__(token=token, base_url=base_url)

    def schedule_meeting(self, title: str, attendees: list[str], metadata: dict[str, Any]) -> IntegrationResult:
        if not self.configured:
            return self.simulated(
                "schedule_meeting",
                {"title": title, "attendees": attendees, "metadata": metadata},
            )
        return IntegrationResult(
            ok=True,
            provider=self.provider,
            action="schedule_meeting",
            data={"title": title, "attendees": attendees, "metadata": metadata},
        )
