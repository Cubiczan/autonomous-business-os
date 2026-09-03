"""Stdio MCP pipe for Autonomous Business OS.

CHP is the lock. This package is the pipe into existing FastAPI services and
``crates/abos-governance-core``. Spend-gate tools live in ``@cubiczan/chp-mcp``.
"""

from app.mcp.server import create_server, main

__all__ = ["create_server", "main"]
