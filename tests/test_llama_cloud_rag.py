from app.services.rag import KnowledgeService


class EmptyCloud:
    def retrieve(self, query: str) -> list:
        return []


class FailingCloud:
    def retrieve(self, query: str) -> list:
        raise RuntimeError("boom")


def test_llama_cloud_empty_result_falls_back_to_local_memory(db_session) -> None:
    service = KnowledgeService(db_session)
    service.llama_cloud = EmptyCloud()
    service.ingest("knowledge", "runbook", "Escalate overdue approvals after two business days.")

    result = service.answer("knowledge", "overdue approvals")

    assert "Escalate overdue approvals" in result["answer"]
    assert result["sources"][0]["key"] == "runbook"


def test_llama_cloud_failure_returns_warning(db_session) -> None:
    service = KnowledgeService(db_session)
    service.llama_cloud = FailingCloud()

    result = service.answer("knowledge", "anything")

    assert "LlamaCloud retrieval is configured but failed" in result["answer"]
    assert result["warning"] == "boom"
