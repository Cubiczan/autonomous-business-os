import os
from dataclasses import dataclass
from typing import Any

from app.config import Settings


@dataclass
class LlamaCloudNode:
    text: str
    score: float | None = None
    source: str | None = None
    metadata: dict[str, Any] | None = None


class LlamaCloudKnowledgeClient:
    """Optional LlamaCloud Index retrieval client.

    The app keeps working without LlamaCloud credentials. When configured, this
    uses the official SDK retrieval flow:
    ``client.pipelines.retrieve(pipeline_id=..., query=...)``.
    """

    def __init__(self, settings: Settings):
        self.settings = settings

    @property
    def configured(self) -> bool:
        return bool(
            self.settings.llama_cloud_api_key
            and self.settings.llama_cloud_knowledge_pipeline_id
        )

    def retrieve(self, query: str) -> list[LlamaCloudNode]:
        if not self.configured:
            return []
        try:
            from llama_cloud import LlamaCloud
        except ImportError as exc:
            raise RuntimeError(
                "llama-cloud is not installed. Install requirements or disable LlamaCloud retrieval."
            ) from exc

        old_key = os.environ.get("LLAMA_CLOUD_API_KEY")
        os.environ["LLAMA_CLOUD_API_KEY"] = self.settings.llama_cloud_api_key or ""
        try:
            client = LlamaCloud()
            results = client.pipelines.retrieve(
                pipeline_id=self.settings.llama_cloud_knowledge_pipeline_id,
                query=query,
                dense_similarity_top_k=self.settings.llama_cloud_dense_top_k,
                sparse_similarity_top_k=self.settings.llama_cloud_sparse_top_k,
                enable_reranking=True,
                rerank_top_n=self.settings.llama_cloud_rerank_top_n,
            )
        finally:
            if old_key is None:
                os.environ.pop("LLAMA_CLOUD_API_KEY", None)
            else:
                os.environ["LLAMA_CLOUD_API_KEY"] = old_key

        nodes: list[LlamaCloudNode] = []
        for item in getattr(results, "retrieval_nodes", []) or []:
            node = getattr(item, "node", None)
            metadata = getattr(node, "metadata", None) or {}
            nodes.append(
                LlamaCloudNode(
                    text=getattr(node, "text", "") or "",
                    score=getattr(item, "score", None),
                    source=metadata.get("file_name") or metadata.get("source") or metadata.get("path"),
                    metadata=metadata,
                )
            )
        return [node for node in nodes if node.text]
