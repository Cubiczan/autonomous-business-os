# Security

## Secret Management

- Never commit `.env`, customer exports, credentials, transcripts, or invoices.
- Store credentials in AWS Secrets Manager, GCP Secret Manager, Railway variables, or the
  equivalent platform secret store.
- Rotate API keys after any local sharing, staging incident, or staff transition.
- Prefer OAuth apps with least-privilege scopes over full-access tokens.

## Webhook Verification

Slack, Stripe, and DocuSign webhook routes include verification hooks. Configure the matching
secret variables in production. Providers with stronger signature schemes should be upgraded
from shared-secret verification to full HMAC verification before handling regulated data.

## Operator Access

The API routes require `x-admin-api-key`. The sample admin dashboard is intended to sit behind
SSO or a private network in production. Add an identity-aware proxy before exposing it publicly.

## Data Handling

- Treat CRM records, contracts, Slack messages, invoices, and meeting transcripts as sensitive.
- Store only what is required for workflow execution and audit.
- Mask PII in logs before adding external log sinks.
- Define retention rules for `memory_entries`, `audit_logs`, and `agent_tasks`.

## Deployment Controls

- Use separate environments for development, staging, and production.
- Use different OAuth apps and webhook secrets per environment.
- Enable database backups and restore tests.
- Require code review before changing approval thresholds or automated-send policies.
