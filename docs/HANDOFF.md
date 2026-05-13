# Handoff

## What Is Included

- FastAPI backend and admin dashboard.
- Master orchestrator and five business agents.
- Integration adapter layer for required tools.
- Durable SQL state and background scheduler.
- Human approval queue and escalation records.
- Docker, Compose, Railway, AWS, and GCP deployment templates.
- Security, architecture, deployment, runbook, and support docs.

## What Needs Environment-Specific Setup

- Production database and Redis.
- API keys and OAuth apps for selected providers.
- Real CRM schema mapping.
- Slack app installation and event subscriptions.
- Stripe webhook endpoint and signing secret.
- Calendar provider service account or OAuth installation.
- SSO or private-network protection for `/admin`.

## Recommended First Production Hardening Sprint

- Add Alembic migrations.
- Add SSO and role-based permissions to the admin dashboard.
- Add provider-specific webhook HMAC verification where the placeholder shared-secret check is
  not sufficient.
- Replace simulated integration paths with staging credentials and contract tests.
- Add a queue system such as Celery, RQ, or Cloud Tasks for high-volume workloads.
- Add PII redaction to external log sinks.
