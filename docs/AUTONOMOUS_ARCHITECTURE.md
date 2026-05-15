# Fully Autonomous Self-Expanding Business System

## Stack Recommendation

The implemented foundation uses the existing production-oriented stack already in this repo:

- FastAPI control plane for APIs, webhooks, and Mission Control.
- SQLAlchemy with SQLite for local development; Postgres is the production target.
- APScheduler for local scheduled loops; Celery/Redis or Temporal is the production upgrade path.
- Jinja dashboard for mobile-friendly Mission Control.
- Prometheus-compatible metrics, structured logs, approval queues, and append-only audit records.
- Integration adapters that simulate safely until credentials are configured.

For the Hostinger Ubuntu/GPU server deployment, run this behind Docker Compose with Postgres,
Redis, reverse proxy TLS, and a secrets manager such as Vault, Infisical, Doppler, or SOPS-backed
environment injection. Agents should never receive raw API keys directly.

LangGraph is the recommended agent execution layer once LLM planning is enabled. The repo now keeps
the business operating system independent from any one model provider: the department factory,
approval engine, audit log, and tool policies remain deterministic even when the reasoning model is
swapped.

## Company Layer

The permanent company layer is seeded at startup by `DepartmentFactory.ensure_company_layer()`:

- Master Orchestrator
- CEO Assistant
- Strategy Agent
- Intelligence Agent
- CFO Agent
- Legal & Compliance Agent
- Security Agent
- Idea Engine

Company agents have no department id. They report to Mission Control and can see company-wide state.
Department agents are isolated under their department id and memory namespace unless the master layer
intentionally shares context upward.

## Self-Spawning Departments

Plain-language department creation is exposed through:

```text
POST /agents/departments
```

Example:

```json
{
  "description": "Create a Content department for YouTube",
  "requested_by": "shyam",
  "run_immediately": true
}
```

The spawning engine automatically:

1. Infers the department type.
2. Defines purpose, goals, operating rules, and revenue signals.
3. Creates the department record.
4. Spawns the standard department team:
   Department CEO, Sales & Outreach, Content Creator, Researcher & Analyst, Operations, Customer Success.
5. Adds specialist agents based on department type.
6. Assigns tools, memory namespaces, schedules, trust levels, and skills.
7. Creates daily, weekly, and type-specific schedules.
8. Starts an initial `department_operation` workflow.

Content departments receive video, social scheduling, and newsletter specialists. Sales departments
receive cold email, CRM, and pipeline specialists. New packs can be added in
`app/services/departments.py`.

## Autonomous Operating Loop

Department schedules create durable `department_operation` workflows. The dynamic department agent:

- Loads the department and its agents.
- Marks agents running.
- Inspects untrusted context for prompt-injection markers.
- Produces department-specific outputs.
- Updates health, revenue signals, and recent output.
- Proposes external actions through the guardrail layer.
- Returns agents to idle or waiting-for-approval state.

This makes departments autonomous for internal planning, research, drafting, analysis, and reporting.
External consequences are approval-gated by policy.

## Skill Management

The runtime skill registry lives in `app/services/skills.py`.

Capabilities:

- Core skills are seeded automatically.
- Skills are SQL records with manifests, versions, scopes, status, and assignments.
- Skills can be company-wide, department-scoped, or agent-specific.
- Agents receive relevant skills during department creation.
- Plain-language skill creation is exposed through:

```text
POST /agents/skills
```

Dropped-in skill manifests are loaded from:

```text
storage/skills/*.json
```

Refresh them at runtime with:

```text
POST /agents/skills/refresh
```

Mission Control shows installed skills and assignment counts.

## Mission Control

Human-facing dashboard:

```text
/admin/mission
```

Machine-readable board:

```text
GET /agents/mission
```

Mission Control includes:

- Active company agents and department agents.
- Department health, revenue signals, and last output.
- Task queue and workflow status.
- Approval inbox.
- Recent external actions.
- Alert feed.
- Skill registry.

## Security And Approval Layer

Security is implemented in `app/services/guardrails.py`.

Non-negotiable policies:

- Outbound email, client messages, social publishing, newsletters, ads, and proposals require approval.
- Financial, contractual, destructive, credential, and production-change actions require approval or block.
- External actions are persisted before execution.
- Human approvals update the corresponding external-action status.
- Tool calls pass through a circuit breaker before execution.
- Prompt-injection markers in untrusted content create security events.
- Pause and kill endpoints stop departments immediately:

```text
POST /agents/departments/{department_id}/pause
POST /agents/departments/{department_id}/kill
```

Agents are assigned trust levels:

- `autonomous`: can perform internal, reversible work with audit logging.
- `supervised`: can run workflows but escalates ambiguous or high-impact steps.
- `approval_required`: cannot execute outbound or external side-effect actions directly.

## Deployment Notes

For production:

- Replace SQLite with Postgres.
- Move secrets to Vault/Infisical/Doppler/SOPS, not `.env` on disk.
- Put Redis/Celery or Temporal behind scheduled and long-running workflows.
- Require authentication on `/admin/*`.
- Use HTTPS, firewall rules, and IP allowlists for webhook ingress.
- Add provider-specific publish/send executors that only run after approval status is approved.
- Keep GitHub/Codeberg publishing in a separate code repo, never mixed with confidential data-room files.

The existing `tools/push_remotes.ps1` supports GitHub and Codeberg pushes with PAT prompts.
