from sqlalchemy.orm import Session

from app.services.memory import MemoryService


class KnowledgeService:
    def __init__(self, session: Session):
        self.memory = MemoryService(session)

    def ingest(self, namespace: str, key: str, text: str, metadata: dict | None = None) -> None:
        self.memory.set(namespace, key, metadata or {}, text=text)

    def answer(self, namespace: str, question: str) -> dict:
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
