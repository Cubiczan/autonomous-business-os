# abos-mcp

Stdio MCP pipe for Cubiczan's Autonomous Business OS. CHP remains the lock; this capability is only the transport into existing governance and HITL code.

## ADDED Requirements

### Requirement: Stdio MCP server starts

The system SHALL expose a local stdio MCP server that implements `initialize` and `tools/list` without requiring a live Stripe or HubSpot credential.

#### Scenario: tools/list returns ABOS tools

- **WHEN** an MCP client starts `python -m app.mcp` over stdio
- **THEN** `tools/list` returns `classify_action`, `inspect_text`, `sign_event`, `list_approvals`, `approve`, `reject`, `finance_operations`, and `lead_qualification`
- **AND** the list SHALL NOT include `evaluate_spend_gate` or `approve_spend`

### Requirement: Governance tools wrap the Rust sidecar

The system SHALL implement `classify_action`, `inspect_text`, and `sign_event` by invoking `crates/abos-governance-core` rather than a parallel policy engine.

#### Scenario: classify_action uses the sidecar

- **WHEN** a client calls `classify_action` with action type `send_email`
- **THEN** the result is the sidecar JSON classification with `requires_approval` true and a high risk level

### Requirement: HITL and domain tools wrap product code

The system SHALL implement approval list/approve/reject and at least `finance_operations` plus `lead_qualification` by calling existing FastAPI services or the same `/agents/*` routes. Default mode SHALL run in-process and SHALL NOT require `uvicorn`. HTTP mode MAY be used when `uvicorn` is already running.

#### Scenario: finance_operations in simulation mode

- **WHEN** a client calls `finance_operations` with no Stripe key configured
- **THEN** the product finance agent runs in zero-credential simulation
- **AND** a human approval is created for the high-impact invoice action

### Requirement: Install docs match the Cubiczan MCP pattern

The project SHALL document Cursor `mcp.json` and `claude mcp add` install, prepare package metadata for `@cubiczan/autonomous-business-os-mcp`, and spell the brand Cubiczan.

#### Scenario: operator installs from a checkout

- **WHEN** an operator follows the MCP README
- **THEN** they can add the server with Cursor `mcp.json` or `claude mcp add`
- **AND** the docs state that default in-process mode does not need `uvicorn`
- **AND** the docs state that HTTP mode requires `uvicorn` already running
