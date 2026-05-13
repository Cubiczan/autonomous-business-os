from typing import Any

from sqlalchemy import select
from sqlalchemy.orm import Session

from app.models import MemoryEntry, utcnow


class MemoryService:
    def __init__(self, session: Session):
        self.session = session

    def set(
        self,
        namespace: str,
        key: str,
        value: dict[str, Any],
        *,
        text: str = "",
    ) -> MemoryEntry:
        existing = self.session.scalar(
            select(MemoryEntry).where(MemoryEntry.namespace == namespace, MemoryEntry.key == key)
        )
        if existing:
            existing.value = value
            existing.text = text
            existing.updated_at = utcnow()
            entry = existing
        else:
            entry = MemoryEntry(namespace=namespace, key=key, value=value, text=text)
            self.session.add(entry)
        self.session.commit()
        return entry

    def get(self, namespace: str, key: str) -> MemoryEntry | None:
        return self.session.scalar(
            select(MemoryEntry).where(MemoryEntry.namespace == namespace, MemoryEntry.key == key)
        )

    def search(self, namespace: str, query: str, limit: int = 5) -> list[MemoryEntry]:
        tokens = [token.lower() for token in query.split() if len(token) > 2]
        if not tokens:
            return []
        entries = self.session.scalars(
            select(MemoryEntry).where(MemoryEntry.namespace == namespace).limit(200)
        ).all()
        scored: list[tuple[int, MemoryEntry]] = []
        for entry in entries:
            haystack = f"{entry.key} {entry.text} {entry.value}".lower()
            score = sum(1 for token in tokens if token in haystack)
            if score:
                scored.append((score, entry))
        scored.sort(key=lambda item: item[0], reverse=True)
        return [entry for _, entry in scored[:limit]]
