"""Invoke the Rust governance sidecar without reimplementing policy."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[2]


class GovernanceError(RuntimeError):
    """Raised when the Rust sidecar cannot be resolved or returns an error."""


def resolve_governance_command() -> tuple[list[str], Path]:
    """Return (argv prefix, cwd) for ``abos-governance-core``."""
    configured = os.environ.get("ABOS_GOVERNANCE_BIN", "").strip()
    if configured:
        return [configured], Path(configured).resolve().parent

    for candidate in (
        REPO_ROOT / "target" / "release" / "abos-governance-core",
        REPO_ROOT / "target" / "debug" / "abos-governance-core",
    ):
        if candidate.exists():
            return [str(candidate)], REPO_ROOT

    cargo = shutil.which("cargo")
    if cargo:
        return [cargo, "run", "--quiet", "-p", "abos-governance-core", "--"], REPO_ROOT

    raise GovernanceError(
        "abos-governance-core binary not found. Build it with "
        "`cargo build -p abos-governance-core` or set ABOS_GOVERNANCE_BIN."
    )


def _run_sidecar(args: list[str], *, extra_env: dict[str, str] | None = None) -> dict[str, Any]:
    command, cwd = resolve_governance_command()
    env = os.environ.copy()
    if extra_env:
        env.update(extra_env)
    completed = subprocess.run(
        [*command, *args],
        cwd=cwd,
        capture_output=True,
        text=True,
        env=env,
        check=False,
    )
    if completed.returncode != 0:
        detail = (completed.stderr or completed.stdout or "").strip()
        raise GovernanceError(detail or f"governance sidecar exited {completed.returncode}")
    stdout = completed.stdout.strip()
    if not stdout:
        raise GovernanceError("governance sidecar returned empty output")
    return json.loads(stdout)


def classify_action(action_type: str, payload: dict[str, Any] | None = None) -> dict[str, Any]:
    raw_payload = json.dumps(payload or {})
    return _run_sidecar(["classify-action", action_type, raw_payload])


def inspect_text(source: str, text: str) -> dict[str, Any]:
    return _run_sidecar(["inspect-text", source, text])


def sign_event(
    event: dict[str, Any],
    *,
    key_id: str = "local",
    secret_env: str = "ABOS_LEDGER_SIGNING_KEY",
) -> dict[str, Any]:
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False, encoding="utf-8") as handle:
        json.dump(event, handle)
        event_path = handle.name
    try:
        return _run_sidecar(["sign-event", event_path, key_id, secret_env])
    finally:
        Path(event_path).unlink(missing_ok=True)
