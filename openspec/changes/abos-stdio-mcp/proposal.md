# Change: Autonomous Business OS stdio MCP

## Why

Cursor and Claude Code can only drive Autonomous Business OS today by calling the REST admin API by hand. Operators already have a Rust governance sidecar (`classify-action`, `inspect-text`, `sign-event`) and a Python approval queue plus domain workflows. Those need a local stdio pipe so an MCP client can invoke real product code without rebuilding the OS.

CHP remains the lock. This MCP is the pipe. Spend-gate tools stay in `@cubiczan/chp-mcp` and are not duplicated here.

## What Changes

- Add a local stdio MCP server that wraps existing FastAPI services and `crates/abos-governance-core`.
- Expose governance tools: `classify_action`, `inspect_text`, `sign_event`.
- Expose HITL tools: `list_approvals`, `approve`, `reject`.
- Expose domain workflows already in the product: `finance_operations` and `lead_qualification`.
- Default to in-process product calls (no live Stripe/HubSpot keys; zero-credential simulation). Optional HTTP mode talks to a running `uvicorn`.
- Prepare npm packaging for `@cubiczan/autonomous-business-os-mcp` (do not publish from this change).
- Document Cursor `.cursor/mcp.json` and `claude mcp add` install. Brand is Cubiczan.

## Capabilities

- `abos-mcp`: stdio MCP transport and tool surface for Autonomous Business OS.

## Impact

- New Python package module `app/mcp`.
- New npm package metadata under `mcp/`.
- Tests cover `tools/list`, `classify_action` against the Rust sidecar, and one finance/approval call in simulation mode.
- No change to CHP Profile B spend-gate behavior.
