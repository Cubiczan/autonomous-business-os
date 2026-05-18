# LlamaCloud Hardening

The Knowledge & Communication Agent can now use LlamaCloud Index as its first
retrieval layer, with durable local memory as fallback.

## Setup

1. Create an index/pipeline in LlamaCloud for the company knowledge base.
2. Add the secrets to the deployment environment:

```bash
LLAMA_CLOUD_API_KEY=llx-...
LLAMA_CLOUD_KNOWLEDGE_PIPELINE_ID=<pipeline-id>
```

Optional retrieval tuning:

```bash
LLAMA_CLOUD_DENSE_TOP_K=12
LLAMA_CLOUD_SPARSE_TOP_K=12
LLAMA_CLOUD_RERANK_TOP_N=5
```

## Behavior

- When both the API key and pipeline id are configured, Slack/admin knowledge
  queries call `client.pipelines.retrieve(...)`.
- Returned nodes are stitched into a concise answer with source metadata.
- If LlamaCloud is not configured, the agent falls back to SQL-backed shared
  memory.
- If LlamaCloud errors, the response includes a warning rather than failing the
  workflow.

## Good First Pipelines

- Internal operating docs, runbooks, SOPs, and onboarding material.
- Customer contracts and onboarding packs after redaction.
- Finance policies, invoice templates, payment follow-up language, and board
  reporting packs.
