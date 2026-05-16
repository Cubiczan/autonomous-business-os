from typing import Any

from sqlalchemy.orm import Session

from app.agents.base import BaseAgent
from app.config import get_settings
from app.integrations.slack import SlackClient
from app.models import Workflow
from app.services.rag import KnowledgeService


class KnowledgeCommunicationAgent(BaseAgent):
    name = "knowledge_communication_agent"

    def __init__(self, session: Session):
        super().__init__(session)
        settings = get_settings()
        self.slack = SlackClient(settings.slack_bot_token)
        self.knowledge = KnowledgeService(session)

    def run(self, workflow: Workflow) -> dict[str, Any]:
        payload = workflow.payload
        result: dict[str, Any] = {}

        if transcript := payload.get("meeting_transcript"):
            summary = self._summarize_meeting(transcript)
            self.knowledge.ingest(
                "knowledge",
                payload.get("meeting_id", workflow.id),
                summary["summary"],
                {"action_items": summary["action_items"]},
            )
            result["meeting_summary"] = summary

        if question := payload.get("question"):
            answer = self.knowledge.answer("knowledge", question)
            result["answer"] = answer
            if payload.get("channel_id"):
                result["slack"] = self.execute_tool(
                    workflow,
                    "slack_answer_query",
                    {"channel_id": payload["channel_id"], "question": question},
                    lambda: self.slack.post_message(payload["channel_id"], answer["answer"]),
                )

        if not result:
            result["message"] = "No meeting transcript or question supplied."
        return result

    def _summarize_meeting(self, transcript: str) -> dict[str, Any]:
        sentences = [part.strip() for part in transcript.replace("\n", " ").split(".") if part.strip()]
        summary = ". ".join(sentences[:4])
        action_items = [
            sentence
            for sentence in sentences
            if any(marker in sentence.lower() for marker in ["todo", "follow up", "action", "owner"])
        ][:8]
        return {
            "summary": summary,
            "action_items": action_items,
        }
