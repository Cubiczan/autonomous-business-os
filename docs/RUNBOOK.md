# Runbook

## Local Startup

1. Copy `.env.example` to `.env`.
2. Set `ADMIN_API_KEY` and any integration credentials.
3. Run `python -m pip install -r requirements.txt`.
4. Start the API with `uvicorn app.main:app --reload`.
5. Open `/admin`, `/docs`, `/health`, and `/metrics`.

## Production Startup

1. Provision Postgres and Redis.
2. Store secrets in the platform secret manager.
3. Deploy the Docker image.
4. Run one API process and at least one worker process.
5. Configure third-party webhooks to point to `/webhooks/*`.
6. Set monitoring alerts on `/health`, error logs, retry counts, and unresolved escalations.

## Common Incidents

### Webhook Requests Failing

- Check webhook secret configuration.
- Confirm the third-party provider points at the deployed base URL.
- Inspect `/admin/audit` for rejected signature or payload errors.

### Workflows Stuck Pending

- Verify the API scheduler or worker process is running.
- Check database connectivity with `/ready`.
- Inspect logs for repeated agent exceptions.

### Too Many Approvals

- Review scoring thresholds and approval policies in the relevant agent.
- Use `/admin/approvals` to drain the queue.

### Third-Party API Failures

- Confirm tokens are present and have the required scopes.
- Check rate limits in the provider console.
- Inspect `agent_tasks` output and audit metadata for the failing provider.

## Rollback

1. Keep the previous container image tagged.
2. Revert traffic to the previous image.
3. Do not roll back the database blindly; preserve audit and workflow state.
4. If a migration is added later, require a tested down migration before production rollout.
