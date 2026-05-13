# Autonomous Business Operating System

Production-ready starter system for orchestrating business agents across sales, onboarding,
delivery, finance, and knowledge operations.

The system includes:

- Master orchestrator with retries, audit logging, failure escalation, and shared state.
- Tool-calling agents for lead qualification, onboarding, delivery monitoring, finance
  operations, and knowledge/communication workflows.
- Webhook pipelines for lead intake, contract completion, Stripe events, Slack events, and
  calendar changes.
- Persistent workflow, task, memory, approval, and audit tables.
- Human-in-the-loop approval queues and override controls.
- Admin dashboard, health checks, Prometheus metrics, and structured logs.
- Integration adapters for Apollo, Hunter, Gmail/SMTP, Outlook, HubSpot/Salesforce,
  DocuSign, Notion, Linear/Jira, Slack, Calendar APIs, Stripe, and accounting tools.

## Quick Start

```powershell
cd autonomous-business-os
copy .env.example .env
python -m venv .venv
.\.venv\Scripts\Activate.ps1
python -m pip install -r requirements.txt
uvicorn app.main:app --reload
```

Open:

- API: `http://localhost:8000`
- Admin dashboard: `http://localhost:8000/admin`
- OpenAPI docs: `http://localhost:8000/docs`
- Metrics: `http://localhost:8000/metrics`

Use the `x-admin-api-key` header with `ADMIN_API_KEY` for protected agent and admin APIs.

## Run With Docker

```powershell
cd autonomous-business-os
copy .env.example .env
docker compose up --build
```

## Project Layout

- `app/agents`: Business agents and master orchestrator.
- `app/integrations`: External tool adapters with safe no-credential behavior.
- `app/services`: Shared memory, workflow, approval, RAG, scheduling, scoring, and audit services.
- `app/api`: REST, webhook, dashboard, health, and metrics routes.
- `docs`: Architecture, deployment, security, runbook, and support handoff.
- `infra`: Prometheus and cloud deployment templates.

## Important Security Note

Do not commit `.env`, API keys, OAuth refresh tokens, customer records, exported CRM data,
meeting transcripts, invoices, or internal documents. Store secrets in the deployment platform
secret manager and reference them through environment variables.
