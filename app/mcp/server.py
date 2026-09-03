"""Stdio MCP server for Autonomous Business OS.

Default mode talks to the product in-process. Set ``ABOS_MCP_MODE=http`` only
when ``uvicorn`` is already serving the FastAPI app.
"""

from __future__ import annotations

import logging
import os
import sys
from typing import Any

from mcp.server.mcpserver import MCPServer

from app.mcp import governance, product

logging.basicConfig(stream=sys.stderr, level=logging.INFO)
logger = logging.getLogger("abos.mcp")

BRAND = "Cubiczan"
PACKAGE_NAME = "@cubiczan/autonomous-business-os-mcp"
CHP_MCP = "@cubiczan/chp-mcp"

EXPECTED_TOOLS = (
    "classify_action",
    "inspect_text",
    "sign_event",
    "list_approvals",
    "approve",
    "reject",
    "finance_operations",
    "lead_qualification",
    "abos_info",
)


def create_server() -> MCPServer:
    server = MCPServer(
        name="autonomous-business-os-mcp",
        instructions=(
            "Cubiczan Autonomous Business OS MCP. CHP is the lock; this server is "
            "the pipe into existing governance and HITL product code. Use "
            f"{CHP_MCP} for spend-gate / capital consensus tools. Do not treat "
            "this server as a replacement for CHP Profile B."
        ),
    )

    @server.tool(
        name="classify_action",
        description=(
            "Classify an external action with the Rust governance sidecar "
            "(abos-governance-core classify-action). Returns risk_level, "
            "requires_approval, status, and reasons."
        ),
    )
    def classify_action(action_type: str, payload: dict[str, Any] | None = None) -> dict[str, Any]:
        return governance.classify_action(action_type, payload)

    @server.tool(
        name="inspect_text",
        description=(
            "Inspect untrusted text for prompt-injection markers via the Rust "
            "sidecar (abos-governance-core inspect-text)."
        ),
    )
    def inspect_text(source: str, text: str) -> dict[str, Any]:
        return governance.inspect_text(source, text)

    @server.tool(
        name="sign_event",
        description=(
            "HMAC-sign an audit event with the Rust sidecar (abos-governance-core "
            "sign-event). Requires ABOS_LEDGER_SIGNING_KEY (or secret_env)."
        ),
    )
    def sign_event(
        event: dict[str, Any],
        key_id: str = "local",
        secret_env: str = "ABOS_LEDGER_SIGNING_KEY",
    ) -> dict[str, Any]:
        return governance.sign_event(event, key_id=key_id, secret_env=secret_env)

    @server.tool(
        name="list_approvals",
        description=(
            "List human-in-the-loop approvals from the Python approval queue. "
            "Optional status filter: open, approved, rejected."
        ),
    )
    def list_approvals(status: str | None = None, limit: int = 50) -> dict[str, Any]:
        return product.list_approvals(status=status, limit=limit)

    @server.tool(
        name="approve",
        description="Approve an open HITL item. Wraps ApprovalService.decide.",
    )
    def approve(approval_id: str, decided_by: str, decision_note: str | None = None) -> dict[str, Any]:
        return product.decide_approval(approval_id, "approved", decided_by, decision_note)

    @server.tool(
        name="reject",
        description="Reject an open HITL item. Wraps ApprovalService.decide.",
    )
    def reject(approval_id: str, decided_by: str, decision_note: str | None = None) -> dict[str, Any]:
        return product.decide_approval(approval_id, "rejected", decided_by, decision_note)

    @server.tool(
        name="finance_operations",
        description=(
            "Run the existing finance_operations workflow. Uses Stripe simulation "
            "when STRIPE_API_KEY is unset. High-impact invoice creation creates a HITL approval."
        ),
    )
    def finance_operations(
        customer_id: str,
        amount_cents: int,
        description: str,
        customer_email: str | None = None,
        currency: str = "usd",
        due_in_days: int = 14,
    ) -> dict[str, Any]:
        return product.finance_operations(
            customer_id,
            amount_cents,
            description,
            customer_email=customer_email,
            currency=currency,
            due_in_days=due_in_days,
        )

    @server.tool(
        name="lead_qualification",
        description=(
            "Run the existing lead_qualification workflow. Uses Apollo/Hunter/CRM "
            "simulation when those keys are unset."
        ),
    )
    def lead_qualification(
        email: str,
        name: str | None = None,
        company: str | None = None,
        title: str | None = None,
        source: str = "mcp",
    ) -> dict[str, Any]:
        return product.lead_qualification(
            email,
            name=name,
            company=company,
            title=title,
            source=source,
        )

    @server.tool(
        name="abos_info",
        description=(
            "Report MCP mode, brand, and the split with CHP. Spend gates are not "
            f"implemented here; use {CHP_MCP}."
        ),
    )
    def abos_info() -> dict[str, Any]:
        return {
            "brand": BRAND,
            "package": PACKAGE_NAME,
            "mode": product.product_mode(),
            "uvicorn_required": product.product_mode() == "http",
            "chp": "CHP is the lock; MCP is the pipe.",
            "spend_gates": f"Use {CHP_MCP}. This server does not expose evaluate_spend_gate.",
            "tools": list(EXPECTED_TOOLS),
            "base_url": os.environ.get("ABOS_BASE_URL", "http://localhost:8000"),
        }

    return server


def main() -> None:
    if product.product_mode() == "http":
        logger.info("abos_mcp_http_mode base_url=%s", os.environ.get("ABOS_BASE_URL", "http://localhost:8000"))
    else:
        product.ensure_product_ready()
        logger.info("abos_mcp_inprocess_mode uvicorn_not_required=true")
    create_server().run(transport="stdio")


if __name__ == "__main__":
    main()
