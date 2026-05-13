# 🏢 Autonomous Business Operating System

**Production-ready multi-agent orchestration for the entire business lifecycle — from lead to revenue.**

[![Python](https://img.shields.io/badge/Python-3.11-3776AB?logo=python)](https://python.org)
[![FastAPI](https://img.shields.io/badge/FastAPI-0.115-009688?logo=fastapi)](https://fastapi.tiangolo.com)
[![SQLAlchemy](https://img.shields.io/badge/SQLAlchemy-2.0-D71F00?logo=python)](https://sqlalchemy.org)
[![Docker](https://img.shields.io/badge/Docker-Compose-2496ED?logo=docker)](https://docker.com)
[![Prometheus](https://img.shields.io/badge/Prometheus-E6522C?logo=prometheus)](https://prometheus.io)

---

## What It Does

ABO orchestrates **5 specialized AI agents** that automate the full business operations pipeline:

```
Lead Intake → Qualification → Onboarding → Delivery → Finance → Knowledge
     ↓              ↓            ↓          ↓          ↓          ↓
  Webhook       Score A/B/C   Notion +    Risk      Invoice    Meeting
  / API         CRM Sync      Linear      Detect    + Reconcile Summaries
                              + Calendar  + Alert
```

Every workflow is **durable** (survives restarts), **auditable** (append-only logs), **retryable** (automatic backoff), and **human-overridable** (approval queues for high-impact actions).

## Zero-Credential Operation

Every integration returns **realistic simulated data** when API keys are absent. The entire 5-agent pipeline runs end-to-end with just:

```bash
cp .env.example .env
uvicorn app.main:app --reload
```

No API keys needed. No external services. No configuration. Ship → Run → Demo.

---

## Architecture

### Agent Pipeline

```
┌──────────────┐     ┌───────────────────┐     ┌──────────────────┐
│   Webhooks   │────▶│   Master          │────▶│   Domain Agent   │
│   / API      │     │   Orchestrator    │     │   (5 types)      │
└──────────────┘     └───────┬───────────┘     └────────┬─────────┘
                             │                          │
                     ┌───────▼───────┐          ┌───────▼───────┐
                     │   Workflow    │          │  Integration  │
                     │   Engine      │          │  Adapters     │
                     │   (SQLite)    │          │  (11 types)   │
                     └───────┬───────┘          └───────┬───────┘
                             │                          │
                     ┌───────▼──────────────────────────▼───────┐
                     │  Shared Services: Audit | Memory | RAG  │
                     │  Approval Queue | Escalation | Scoring  │
                     └────────────────────────────────────────┘
```

### 5 Specialized Agents

| Agent | Workflow Kind | Integrations Used | Key Capabilities |
|---|---|---|---|
| **Lead Qualification** | `lead_qualification` | Apollo, Hunter, CRM, Email | Enrich leads, verify emails, score A/B/C, draft outreach, sync CRM, HITL for Tier A |
| **Client Onboarding** | `client_onboarding` | Notion, Linear/Jira, Calendar, Slack | Build project plan, create Notion page, create tasks, schedule kickoff, notify team |
| **Delivery Monitoring** | `delivery_monitoring` | Slack | Detect budget drift, communication gaps, schedule delays, post status, create escalations |
| **Finance Operations** | `finance_operations` | Stripe, Accounting, Email | Create invoices, check overdue, reconcile payments, weekly summaries, HITL for follow-ups |
| **Knowledge & Comms** | `knowledge_communication` | Slack, internal RAG | Ingest meeting summaries, answer knowledge queries, post to Slack |

### 11 Integration Adapters (18 Services)

| Adapter | Providers | Actions |
|---|---|---|
| **CRM** | HubSpot, Salesforce | Upsert leads, update deal stage |
| **Email** | SMTP (generic) | Draft outreach, send email |
| **Accounting** | QuickBooks, Xero | Reconcile payments |
| **Payments** | Stripe | Create invoices, list overdue |
| **Lead Enrichment** | Apollo.io | Enrich person by email |
| **Email Verification** | Hunter.io | Verify deliverability |
| **Messaging** | Slack | Post messages to channels |
| **Documentation** | Notion | Create database pages |
| **Calendar** | Google Calendar, Microsoft Graph | Schedule meetings |
| **E-Signature** | DocuSign | Get envelope status |
| **Task Management** | Linear, Jira | Create tasks |

### Workflow State Machine

```
  pending ──────▶ running ──────▶ completed
     │               │
     │               ├──▶ waiting_for_human ──▶ completed
     │               │
     │               └──▶ failed ──▶ (retry) ──▶ pending
     │                                │
     │                                └──▶ (exhausted) ──▶ escalation created
     │
     └──▶ cancelled
```

- **Automatic retry**: Up to 3 attempts with exponential backoff
- **Human-in-the-loop**: Agents pause for approval on high-impact actions
- **Escalation**: Failed-after-retry workflows create ops team alerts
- **Task tracing**: Every tool call recorded with input, output, error, and timing

---

## Human-in-the-Loop Approvals

Agents can pause their own workflows when human judgment is required:

- **Lead Qualification**: Tier A leads require approval before outreach is sent
- **Finance Operations**: Overdue invoice follow-ups require approval before email
- **Admin Dashboard**: Inline approve/reject forms with decision notes

```python
# Agent sets workflow to waiting_for_human
self.workflow_service.mark_waiting_for_human(workflow.id)
self.approval_service.request(
    workflow_id=workflow.id,
    proposed_action="Send outreach email to Tier A lead",
    context={"lead": "john@acme.com", "score": 85, "tier": "A"}
)
```

---

## Webhook Pipelines

| Webhook | Trigger | Agent | Security |
|---|---|---|---|
| `POST /webhooks/leads` | New lead form submission | Lead Qualification | API key |
| `POST /webhooks/docusign` | Contract signed | Client Onboarding | DocuSign signature |
| `POST /webhooks/stripe` | Payment event | Finance Operations | Stripe signature |
| `POST /webhooks/slack` | Slack message/event | Knowledge & Comms | HMAC-SHA256 |
| `POST /webhooks/calendar` | Meeting scheduled | Delivery Monitoring | API key |

---

## Admin Dashboard

Built-in operations console at `/admin`:

- **Dashboard**: Workflow counts, recent activity, open approvals, active escalations
- **Workflows**: Full history with expandable task traces and results
- **Approvals**: Inline approve/reject forms with decision notes
- **Audit Trail**: Immutable append-only log (last 200 entries)

---

## Observability

| Tool | Endpoint | What It Tracks |
|---|---|---|
| **Prometheus** | `/metrics` | Request counter (by path), workflow gauge |
| **Health** | `/health` | Liveness probe |
| **Readiness** | `/ready` | Database connectivity check |
| **Structured Logs** | stdout | JSON logs with ISO timestamps via structlog |

Docker Compose includes Prometheus with pre-configured scrape target.

---

## Quick Start

### Local Development

```bash
git clone https://github.com/zan-maker/autonomous-business-os.git
cd autonomous-business-os
cp .env.example .env
python -m venv .venv
source .venv/bin/activate  # or .\ .venv\Scripts\Activate on Windows
pip install -r requirements.txt
uvicorn app.main:app --reload
```

### Docker Compose (Recommended)

```bash
docker compose up --build
```

Starts: **API server** (8000), **Background worker**, **Redis**, **Prometheus** (9090).

### Endpoints

| URL | Description |
|---|---|
| `http://localhost:8000/docs` | OpenAPI interactive docs |
| `http://localhost:8000/admin` | Operations dashboard |
| `http://localhost:8000/health` | Health check |
| `http://localhost:8000/metrics` | Prometheus metrics |
| `http://localhost:9090` | Prometheus dashboard |

---

## API Examples

### Qualify a Lead
```bash
curl -X POST http://localhost:8000/agents/lead-qualification \
  -H "x-admin-api-key: your-key" \
  -H "Content-Type: application/json" \
  -d '{"email":"sarah@acme.com","name":"Sarah Chen","company":"Acme Corp","title":"VP Engineering"}'
```

### Trigger Client Onboarding
```bash
curl -X POST http://localhost:8000/agents/client-onboarding \
  -H "x-admin-api-key: your-key" \
  -H "Content-Type: application/json" \
  -d '{"client_name":"Acme Corp","client_email":"sarah@acme.com","contract_id":"CTR-001","project_type":"AI Integration"}'
```

### Create a Workflow
```bash
curl -X POST "http://localhost:8000/agents/workflows?run_immediately=true" \
  -H "x-admin-api-key: your-key" \
  -H "Content-Type: application/json" \
  -d '{"kind":"finance_operations","payload":{"customer_id":"cus_123","amount_cents":500000,"currency":"usd","description":"Q2 retainer"}}'
```

---

## Security

- **Admin API key**: HMAC-safe comparison on all `/agents/*` endpoints
- **Slack webhook signing**: Full HMAC-SHA256 with 5-minute replay protection
- **Stripe/DocuSign verification**: Signature validation on webhook payloads
- **Append-only audit trail**: Every workflow transition, tool call, and approval recorded
- **No secrets in code**: All credentials via environment variables
- **Safe simulation mode**: No external calls without explicit credentials

---

## Deployment

| Platform | Guide |
|---|---|
| **Local** | `uvicorn app.main:app --reload` |
| **Docker** | `docker compose up --build` |
| **Railway** | Git connect + Postgres + Redis |
| **AWS** | ECR + ECS Fargate + RDS + ALB |
| **GCP** | Artifact Registry + Cloud Run + Cloud SQL |

See [`docs/DEPLOYMENT.md`](docs/DEPLOYMENT.md) for step-by-step instructions.

---

## Project Structure

```
autonomous-business-os/
├── app/
│   ├── agents/           # 5 domain agents + master orchestrator
│   │   ├── base.py       # BaseAgent with execute_tool() instrumentation
│   │   ├── orchestrator.py    # Workflow dispatch + retry + escalation
│   │   ├── lead_qualification.py   # Enrich → Score → CRM → Outreach
│   │   ├── client_onboarding.py    # Notion → Tasks → Calendar → Slack
│   │   ├── delivery_monitoring.py  # Risk detect → Status → Escalate
│   │   ├── finance_operations.py   # Invoice → Overdue → Reconcile
│   │   └── knowledge_communication.py  # Ingest → Query → Post
│   ├── api/              # REST + webhook + admin routes
│   ├── integrations/     # 11 adapters with safe simulation
│   ├── services/         # Workflow, memory, RAG, audit, approval, scoring
│   ├── templates/        # Jinja2 admin dashboard
│   └── static/           # Dashboard CSS
├── docs/                 # Architecture, deployment, security, runbook
├── infra/                # Prometheus, AWS, GCP, Railway configs
├── tests/                # Unit tests (scoring, security, knowledge, delivery)
├── Dockerfile            # Multi-stage Python 3.11-slim
├── docker-compose.yml    # API + Worker + Redis + Prometheus
└── requirements.txt      # 14 dependencies
```

**2,900 lines of production-grade Python code. 14 dependencies. Zero external requirements for local development.**

---

## What Makes This Different

| Feature | Typical Approach | ABO |
|---|---|---|
| **Integrations** | Fail without credentials | Safe simulation — runs end-to-end with zero keys |
| **Tool Calls** | Scattered HTTP calls | Single `execute_tool()` chokepoint with automatic audit |
| **Agent Intelligence** | Hardcoded logic OR full ML | Rule-based with clear interfaces, designed to swap in LLMs |
| **Business Scope** | Single workflow | Full lifecycle: Lead → Close → Onboard → Deliver → Invoice |
| **Human Override** | Afterthought | First-class approval queue in data model |
| **Observability** | Logs only | Prometheus + structured JSON + health/readiness probes |
| **Dependencies** | 50+ packages | 14 packages total |

---

## License

MIT

---

**Built by [Cubiczan Technologies](https://www.cubiczan.com)**
