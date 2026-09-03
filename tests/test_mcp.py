"""Stdio MCP pipe: tools/list, Rust classify-action, and simulated finance HITL."""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[1]


@pytest.fixture(scope="session")
def governance_bin() -> str:
    completed = subprocess.run(
        ["cargo", "build", "-p", "abos-governance-core"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    if completed.returncode != 0:
        pytest.skip(f"Rust sidecar build failed: {completed.stderr[-400:]}")
    binary = REPO_ROOT / "target" / "debug" / "abos-governance-core"
    if not binary.exists():
        pytest.skip("abos-governance-core binary was not produced")
    return str(binary)


def test_classify_action_hits_rust_sidecar(governance_bin: str) -> None:
    from app.mcp.governance import classify_action

    os.environ["ABOS_GOVERNANCE_BIN"] = governance_bin
    result = classify_action("send_email", {"to": "client@example.com"})
    assert result["requires_approval"] is True
    assert result["risk_level"] == "high"
    assert result["status"] == "proposed"


def test_finance_operations_simulation_opens_approval(db_session) -> None:
    from app.mcp.product import decide_approval, finance_operations, list_approvals

    result = finance_operations(
        "cus_sim_1",
        250000,
        "Monthly retainer",
        customer_email="billing@example.com",
        session=db_session,
    )
    assert result["mode"] == "inprocess"
    assert result["status"] == "waiting_for_human"
    invoice_action = result["result"]["invoice_action"]
    assert invoice_action["requires_approval"] is True
    approval_id = invoice_action["approval_id"]
    assert approval_id

    listed = list_approvals(status="open", session=db_session)
    ids = {item["id"] for item in listed["approvals"]}
    assert approval_id in ids

    decided = decide_approval(approval_id, "approved", "operator", "sim review", session=db_session)
    assert decided["approval"]["status"] == "approved"


def _tool_text(result) -> dict:
    structured = getattr(result, "structuredContent", None) or getattr(result, "data", None)
    if isinstance(structured, dict):
        return structured
    chunks = []
    for item in getattr(result, "content", []) or []:
        text = getattr(item, "text", None)
        if text:
            chunks.append(text)
    payload = "".join(chunks).strip()
    if payload.startswith("{") or payload.startswith("["):
        parsed = json.loads(payload)
        return parsed if isinstance(parsed, dict) else {"value": parsed}
    return {"text": payload}


def test_stdio_tools_list_classify_and_finance(governance_bin: str, tmp_path: Path) -> None:
    pytest.importorskip("mcp")
    from mcp.client import Client
    from mcp.client.stdio import StdioServerParameters

    db_path = tmp_path / "abos-mcp.sqlite3"
    env = {
        "PATH": os.environ.get("PATH", ""),
        "HOME": os.environ.get("HOME", ""),
        "PYTHONPATH": str(REPO_ROOT),
        "DATABASE_URL": f"sqlite:///{db_path}",
        "ABOS_GOVERNANCE_BIN": governance_bin,
        "ABOS_MCP_MODE": "inprocess",
        "ABOS_LEDGER_SIGNING_KEY": "test-ledger-key",
        "ADMIN_API_KEY": "changeme-insecure-default-do-not-use-in-prod",
        "STRIPE_API_KEY": "",
        "HUBSPOT_ACCESS_TOKEN": "",
    }
    params = StdioServerParameters(
        command=sys.executable,
        args=["-m", "app.mcp"],
        env=env,
        cwd=str(REPO_ROOT),
    )

    async def _exercise() -> None:
        async with Client(params) as client:
            listed = await client.list_tools()
            names = {tool.name for tool in listed.tools}
            for required in (
                "classify_action",
                "inspect_text",
                "sign_event",
                "list_approvals",
                "approve",
                "reject",
                "finance_operations",
                "lead_qualification",
            ):
                assert required in names
            assert "evaluate_spend_gate" not in names
            assert "approve_spend" not in names

            classified = await client.call_tool(
                "classify_action",
                {"action_type": "send_email", "payload": {"to": "client@example.com"}},
            )
            assert classified.isError is not True
            classification = _tool_text(classified)
            assert classification["requires_approval"] is True
            assert classification["risk_level"] == "high"

            finance = await client.call_tool(
                "finance_operations",
                {
                    "customer_id": "cus_sim_mcp",
                    "amount_cents": 125000,
                    "description": "Simulated invoice",
                    "customer_email": "billing@example.com",
                },
            )
            assert finance.isError is not True
            finance_body = _tool_text(finance)
            assert finance_body["status"] == "waiting_for_human"
            assert finance_body["result"]["invoice_action"]["requires_approval"] is True

            approvals = await client.call_tool("list_approvals", {"status": "open"})
            assert approvals.isError is not True
            inbox = _tool_text(approvals)
            assert inbox["approvals"]
            assert inbox["approvals"][0]["id"] == finance_body["result"]["invoice_action"]["approval_id"]

    import asyncio

    asyncio.run(_exercise())


def test_stdio_entrypoint_is_importable() -> None:
    from app.mcp.server import EXPECTED_TOOLS, create_server

    server = create_server()
    assert server.name == "autonomous-business-os-mcp"
    assert "classify_action" in EXPECTED_TOOLS
    assert "evaluate_spend_gate" not in EXPECTED_TOOLS
