from typing import Any

from sqlalchemy.orm import Session

from app.agents.base import BaseAgent
from app.config import get_settings
from app.integrations.slack import SlackClient
from app.models import Workflow
from app.services.escalation import EscalationService
from app.services.memory import MemoryService


class DeliveryMonitoringAgent(BaseAgent):
    name = "delivery_monitoring_agent"

    def __init__(self, session: Session):
        super().__init__(session)
        settings = get_settings()
        self.slack = SlackClient(settings.slack_bot_token)
        self.escalations = EscalationService(session)
        self.memory = MemoryService(session)

    def run(self, workflow: Workflow) -> dict[str, Any]:
        payload = workflow.payload
        risks = self._detect_risks(payload)
        status_update = self._draft_status_update(payload, risks)

        self.memory.set(
            "delivery",
            payload["project_id"],
            {"latest_status": payload, "risks": risks, "status_update": status_update},
            text=status_update,
        )

        slack_result = self.execute_tool(
            workflow,
            "slack_post_delivery_update",
            {"project_id": payload["project_id"], "risks": risks},
            lambda: self.slack.post_message("#delivery", status_update),
        )

        escalation = None
        if risks:
            severity = "high" if any(risk["severity"] == "high" for risk in risks) else "medium"
            escalation = self.escalations.create(
                f"Delivery risks detected for {payload['client_name']}",
                workflow_id=workflow.id,
                severity=severity,
                owner="delivery-lead",
                context={"project_id": payload["project_id"], "risks": risks},
            )

        return {
            "risks": risks,
            "status_update": status_update,
            "slack": slack_result,
            "escalation_id": escalation.id if escalation else None,
        }

    def _detect_risks(self, payload: dict[str, Any]) -> list[dict[str, str]]:
        risks: list[dict[str, str]] = []
        completion = payload.get("completion_pct", 0)
        budget = payload.get("budget_used_pct", 0)
        quiet_days = payload.get("days_since_client_contact", 0)
        if budget > completion + 20:
            risks.append(
                {
                    "type": "budget_drift",
                    "severity": "high" if budget > completion + 35 else "medium",
                    "detail": "Budget consumption is outpacing milestone completion.",
                }
            )
        if quiet_days >= 7:
            risks.append(
                {
                    "type": "communication_gap",
                    "severity": "medium",
                    "detail": "No recent client communication recorded.",
                }
            )
        if completion < 40 and payload.get("metadata", {}).get("days_until_deadline", 999) <= 14:
            risks.append(
                {
                    "type": "schedule_delay",
                    "severity": "high",
                    "detail": "Low completion with deadline approaching.",
                }
            )
        return risks

    def _draft_status_update(self, payload: dict[str, Any], risks: list[dict[str, str]]) -> str:
        risk_line = "No delivery risks detected." if not risks else "; ".join(risk["detail"] for risk in risks)
        return (
            f"{payload['client_name']} / {payload['milestone']}: "
            f"{payload['completion_pct']}% complete, {payload['budget_used_pct']}% budget used. "
            f"{risk_line}"
        )
