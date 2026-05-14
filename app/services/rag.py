from sqlalchemy.orm import Session

from app.config import get_settings
from app.integrations.llama_cloud import LlamaCloudKnowledgeClient
from app.services.memory import MemoryService


class KnowledgeService:
    def __init__(self, session: Session):
        self.memory = MemoryService(session)
        self.llama_cloud = LlamaCloudKnowledgeClient(get_settings())

    def ingest(self, namespace: str, key: str, text: str, metadata: dict | None = None) -> None:
        self.memory.set(namespace, key, metadata or {}, text=text)

    def answer(self, namespace: str, question: str) -> dict:
        cloud_answer = self._answer_from_llama_cloud(question)
        if cloud_answer:
            return cloud_answer

        matches = self.memory.search(namespace, question, limit=5)
        context = [entry.text for entry in matches if entry.text]
        if not context:
            return {
                "answer": "I do not have enough internal knowledge to answer that yet.",
                "sources": [],
            }

        stitched = " ".join(context)
        answer = stitched[:900]
        if len(stitched) > 900:
            answer += "..."
        return {
            "answer": answer,
            "sources": [{"key": entry.key, "id": entry.id} for entry in matches],
        }

    def _answer_from_llama_cloud(self, question: str) -> dict | None:
        try:
            nodes = self.llama_cloud.retrieve(question)
        except Exception as exc:  # noqa: BLE001 - retrieval is optional; local memory is fallback
            return {
                "answer": (
                    "LlamaCloud retrieval is configured but failed. "
                    "Falling back requires local memory for this query."
                ),
                "sources": [],
                "warning": str(exc),
            }
        if not nodes:
            return None
        stitched = " ".join(node.text for node in nodes)
        answer = stitched[:1200]
        if len(stitched) > 1200:
            answer += "..."
        return {
            "answer": answer,
            "sources": [
                {
                    "provider": "llamacloud",
                    "source": node.source,
                    "score": node.score,
                    "metadata": node.metadata or {},
                }
                for node in nodes
            ],
        }
