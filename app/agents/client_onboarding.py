from typing import Any

from sqlalchemy.orm import Session

from app.agents.base import BaseAgent
from app.config import get_settings
from app.integrations.calendar import CalendarClient
from app.integrations.notion import NotionClient
from app.integrations.slack import SlackClient
from app.integrations.task_management import TaskManagementClient
from app.models import Workflow
from app.services.memory import MemoryService


class ClientOnboardingAgent(BaseAgent):
    name = "client_onboarding_agent"

    def __init__(self, session: Session):
        super().__init__(session)
        settings = get_settings()
        self.notion = NotionClient(settings.notion_token, settings.notion_database_id)
        self.tasks = TaskManagementClient(
            "linear" if settings.linear_api_key else "jira",
            settings.linear_api_key,
            settings.jira_api_token,
            settings.jira_base_url,
        )
        calendar_token = (
            settings.google_calendar_credentials_json
            if settings.calendar_provider == "google"
            else settings.microsoft_graph_token
        )
        self.calendar = CalendarClient(settings.calendar_provider, calendar_token)
        self.slack = SlackClient(settings.slack_bot_token)
        self.memory = MemoryService(session)

    def run(self, workflow: Workflow) -> dict[str, Any]:
        payload = workflow.payload
        plan = self._build_project_plan(payload)

        notion = self.execute_tool(
            workflow,
            "notion_create_client_page",
            {"client": payload["client_name"]},
            lambda: self.notion.create_page(
                f"{payload['client_name']} onboarding",
                {"Client": {"title": [{"text": {"content": payload["client_name"]}}]}},
            ),
        )

        created_tasks = []
        for item in plan["tasks"]:
            result = self.execute_tool(
                workflow,
                "task_manager_create_task",
                {"title": item["title"]},
                lambda item=item: self.tasks.create_task(
                    item["title"],
                    item["description"],
                    {"client": payload["client_name"], "contract_id": payload["contract_id"]},
                ),
            )
            created_tasks.append(result)

        attendees = [payload["client_email"]] if payload.get("client_email") else []
        kickoff = self.execute_tool(
            workflow,
            "calendar_schedule_kickoff",
            {"client": payload["client_name"], "attendees": attendees},
            lambda: self.calendar.schedule_meeting(
                f"{payload['client_name']} kickoff",
                attendees,
                {"contract_id": payload["contract_id"], "project_type": payload["project_type"]},
            ),
        )

        notification = self.execute_tool(
            workflow,
            "slack_notify_onboarding",
            {"client": payload["client_name"]},
            lambda: self.slack.post_message(
                "#client-onboarding",
                f"Onboarding started for {payload['client_name']} ({payload['project_type']}).",
            ),
        )

        self.memory.set(
            "clients",
            payload["client_name"],
            {"plan": plan, "notion": notion, "kickoff": kickoff},
            text=f"Onboarding plan for {payload['client_name']}: {plan['summary']}",
        )
        return {
            "plan": plan,
            "notion": notion,
            "tasks": created_tasks,
            "kickoff": kickoff,
            "notification": notification,
        }

    def _build_project_plan(self, payload: dict[str, Any]) -> dict[str, Any]:
        client = payload["client_name"]
        tasks = [
            {
                "title": f"{client}: confirm goals and success metrics",
                "description": "Align stakeholders on desired outcomes, timeline, risks, and owners.",
            },
            {
                "title": f"{client}: provision workspace and access",
                "description": "Create project workspace, invite team, and verify required credentials.",
            },
            {
                "title": f"{client}: implementation plan",
                "description": "Break scope into milestones, acceptance criteria, and delivery checkpoints.",
            },
            {
                "title": f"{client}: kickoff agenda",
                "description": "Prepare agenda, roles, communication rhythm, and next actions.",
            },
        ]
        return {
            "summary": f"{len(tasks)} onboarding tasks prepared for {payload['project_type']}.",
            "tasks": tasks,
        }
