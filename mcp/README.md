# `@cubiczan/autonomous-business-os-mcp`

Stdio MCP pipe for **Cubiczan** Autonomous Business OS.

CHP is the lock. MCP is the pipe. This server wraps the existing Rust governance sidecar and the Python approval / workflow layer so Cursor or Claude Code can call the product without driving the REST admin API by hand.

Spend / capital gates stay in [`@cubiczan/chp-mcp`](https://www.npmjs.com/package/@cubiczan/chp-mcp). This package does **not** expose `evaluate_spend_gate` or `approve_spend`.

## How the pieces fit

```text
MCP client (Cursor / Claude / …)
        │  tools/call
        ▼
┌──────────────────────────────────┐
│  MCP server (transport / pipe)   │  ← you are here
│  classify_action                 │
│  inspect_text / sign_event       │
│  list_approvals / approve / reject
│  finance_operations              │
│  lead_qualification              │
└─────────────┬────────────────────┘
              │ wraps (does not rebuild)
     ┌────────┴─────────┐
     ▼                  ▼
 crates/abos-governance-core     FastAPI product
 classify-action                 ApprovalService
 inspect-text                    MasterOrchestrator
 sign-event                      finance_operations
                                 lead_qualification
```

## Does `uvicorn` need to be running?

**No** in the default `inprocess` mode. The stdio server imports the FastAPI services and talks to the same SQLite (or `DATABASE_URL`) as the app. Zero-credential simulation is used when Stripe / HubSpot keys are absent.

**Yes** only when you set `ABOS_MCP_MODE=http`. Then the server calls `http://localhost:8000/agents/*` and `uvicorn app.main:app` must already be running.

## Install

Packaging is prepared for npm. This checkout is the source of truth; do not assume the scoped package is published yet.

### From a local checkout (recommended)

```bash
python -m pip install -e ".[mcp]"
cargo build -p abos-governance-core
```

### Cursor / Claude Desktop

`.cursor/mcp.json` (or Claude Desktop `mcpServers`):

```json
{
  "mcpServers": {
    "abos": {
      "command": "python",
      "args": ["-m", "app.mcp"],
      "env": {
        "ABOS_LEDGER_SIGNING_KEY": "change-me"
      }
    }
  }
}
```

After the npm package is published, the same shape as `@cubiczan/chp-mcp`:

```json
{
  "mcpServers": {
    "abos": {
      "command": "npx",
      "args": ["-y", "@cubiczan/autonomous-business-os-mcp"],
      "env": {
        "ABOS_REPO_ROOT": "/absolute/path/to/autonomous-business-os",
        "ABOS_LEDGER_SIGNING_KEY": "change-me"
      }
    }
  }
}
```

`npx` launches `python -m app.mcp` against that checkout. Set `ABOS_PYTHON` if `python3` is not on `PATH`.

### Claude Code

From the repo root:

```bash
claude mcp add abos -- python -m app.mcp
```

After publish:

```bash
claude mcp add abos -- npx -y @cubiczan/autonomous-business-os-mcp
```

## HTTP mode (optional)

```bash
# terminal 1
uvicorn app.main:app --reload

# terminal 2 / MCP env
ABOS_MCP_MODE=http
ABOS_BASE_URL=http://localhost:8000
ADMIN_API_KEY=changeme-insecure-default-do-not-use-in-prod
```

`uvicorn` must already be running in this mode.

## Tools

| Tool | Maps to | Purpose |
|------|---------|---------|
| `classify_action` | `abos-governance-core classify-action` | Risk / approval classification |
| `inspect_text` | `abos-governance-core inspect-text` | Prompt-injection inspection |
| `sign_event` | `abos-governance-core sign-event` | HMAC ledger signature |
| `list_approvals` | Python `HumanApproval` queue | List HITL items |
| `approve` | `ApprovalService.decide` | Approve HITL |
| `reject` | `ApprovalService.decide` | Reject HITL |
| `finance_operations` | `FinanceOperationsAgent` | Invoice workflow (simulates Stripe when unset) |
| `lead_qualification` | `LeadQualificationAgent` | Lead workflow (simulates HubSpot when unset) |
| `abos_info` | local metadata | Mode, brand, CHP pointer |

### Example — classify an outbound action

```jsonc
// tools/call classify_action
{ "action_type": "send_email", "payload": { "to": "client@example.com" } }
```

### Example — finance in simulation mode

```jsonc
// tools/call finance_operations
{
  "customer_id": "cus_sim_1",
  "amount_cents": 250000,
  "description": "Monthly retainer",
  "customer_email": "billing@example.com"
}
```

Invoice creation is a high-impact action, so the product opens a HITL approval. Use `list_approvals` then `approve` / `reject`.

## Related

| Package / repo | Role |
|----------------|------|
| [`@cubiczan/chp-mcp`](https://www.npmjs.com/package/@cubiczan/chp-mcp) | CHP Profile B spend / capital gates (the lock) |
| [`@cubiczan/chp`](https://www.npmjs.com/package/@cubiczan/chp) | Profile B library |
| This package | Pipe into Autonomous Business OS |

## Licence

MIT. Built by Cubiczan.
