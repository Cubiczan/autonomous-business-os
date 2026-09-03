from __future__ import annotations

import json
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "rust" / "Cargo.toml"
DEBUG_BIN = ROOT / "rust" / "target" / "debug" / "abos-core"
RELEASE_BIN = ROOT / "rust" / "target" / "release" / "abos-core"


def run_abos_core(command: str, payload: dict) -> dict:
    request = json.dumps({"command": command, "input": payload})
    if RELEASE_BIN.exists():
        proc = subprocess.run([str(RELEASE_BIN)], input=request, text=True, capture_output=True, check=True)
        return json.loads(proc.stdout)
    if DEBUG_BIN.exists():
        proc = subprocess.run([str(DEBUG_BIN)], input=request, text=True, capture_output=True, check=True)
        return json.loads(proc.stdout)

    proc = subprocess.run(
        ["cargo", "run", "--quiet", "--manifest-path", str(MANIFEST), "--bin", "abos-core"],
        input=request,
        text=True,
        capture_output=True,
        check=True,
    )
    return json.loads(proc.stdout)


def department_metrics(payload: dict) -> dict:
    response = run_abos_core("department_metrics", payload)
    return response["value"]


def analyze_skill_description(payload: dict) -> dict:
    response = run_abos_core("analyze_skill_description", payload)
    return response["value"]


def resolve_skill_slugs(payload: dict) -> list[str]:
    response = run_abos_core("resolve_skill_slugs", payload)
    value = response["value"]
    return list(value["slugs"] if isinstance(value, dict) else value)


def serialize_sdk_result(payload: dict | list | str | int | float | bool | None) -> dict:
    response = run_abos_core("serialize_sdk_result", {"payload": payload})
    value = response["value"]
    if isinstance(value, dict) and "value" in value:
        value = value["value"]
    return value if isinstance(value, dict) else {"value": value}


def serialize_department(payload: dict) -> dict:
    response = run_abos_core("serialize_department", {"payload": payload})
    value = response["value"]
    if isinstance(value, dict) and "value" in value:
        value = value["value"]
    return value if isinstance(value, dict) else {"value": value}
