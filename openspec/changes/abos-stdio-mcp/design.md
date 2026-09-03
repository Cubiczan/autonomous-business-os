# Design: Autonomous Business OS stdio MCP

## Context

Autonomous Business OS is a FastAPI app with SQL-backed approvals and domain agents, plus a Rust sidecar at `crates/abos-governance-core`. `@cubiczan/chp-mcp` is the Cubiczan pattern for a stdio MCP: Cursor `mcp.json`, `claude mcp add`, npm package name. This change follows that install shape without copying spend-gate tools.

## Goals

- MCP tools must hit real product code (Rust CLI and Python services), not a parallel OS.
- Default local mode must run without `uvicorn` and without live Stripe/HubSpot keys.
- Packaging is prepared for `@cubiczan/autonomous-business-os-mcp` but not published.

## Non-goals

- Rebuilding agents, integrations, or the admin dashboard.
- Publishing to npm or PyPI.
- Reimplementing CHP Profile B `evaluate_spend_gate` / `approve_spend`.

## Architecture

```text
MCP client (Cursor / Claude Code)
        │  stdio tools/call
        ▼
┌──────────────────────────────────┐
│  MCP server (pipe)               │
│  python -m app.mcp               │
│  or npx @cubiczan/autonomous-    │
│     business-os-mcp              │
└─────────────┬────────────────────┘
              │
     ┌────────┴─────────┐
     ▼                  ▼
 Rust sidecar      Python product
 classify-action   ApprovalService
 inspect-text      MasterOrchestrator
 sign-event        finance_operations
                   lead_qualification
```

CHP is the lock for capital/spend consensus. This server is only the pipe into ABOS governance and HITL.

### Modes

| Mode | Env | uvicorn | Use |
|------|-----|---------|-----|
| `inprocess` (default) | `ABOS_MCP_MODE=inprocess` | Not required | Direct SQLAlchemy + orchestrator. Shares `DATABASE_URL` with the app. |
| `http` | `ABOS_MCP_MODE=http`, `ABOS_BASE_URL`, `ADMIN_API_KEY` | Must already be running | Thin client over existing `/agents/*` routes. |

### Governance resolution

1. `ABOS_GOVERNANCE_BIN` if set.
2. `target/release/abos-governance-core` or `target/debug/abos-governance-core`.
3. `cargo run -p abos-governance-core --`.

Signing uses `ABOS_LEDGER_SIGNING_KEY` at runtime. No keys in source.

## Tool surface

| Tool | Maps to | Notes |
|------|---------|-------|
| `classify_action` | `abos-governance-core classify-action` | Rust sidecar |
| `inspect_text` | `abos-governance-core inspect-text` | Rust sidecar |
| `sign_event` | `abos-governance-core sign-event` | Rust sidecar |
| `list_approvals` | `HumanApproval` query / mission inbox | HITL |
| `approve` | `ApprovalService.decide` + guardrail sync | HITL |
| `reject` | `ApprovalService.decide` + guardrail sync | HITL |
| `finance_operations` | `FinanceOperationsAgent` | Simulation when Stripe unset |
| `lead_qualification` | `LeadQualificationAgent` | Simulation when HubSpot unset |
| `abos_info` | local metadata | Points spend gates at `@cubiczan/chp-mcp` |

## Risks

- Stdio servers must not write logs to stdout. Route diagnostics to stderr.
- SQLite + concurrent `uvicorn` and in-process MCP can contend; document sharing `DATABASE_URL` or using HTTP mode.
- `cargo run` is slow; prefer a built binary in tests.
