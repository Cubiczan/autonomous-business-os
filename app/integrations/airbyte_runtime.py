"""Bridge sync ABO agents to the async Airbyte Agent SDK."""

from __future__ import annotations

import asyncio
import concurrent.futures
from typing import Any

from airbyte_agent_sdk import connect

from app.config import Settings, get_settings


def airbyte_hosted_configured(settings: Settings | None = None) -> bool:
    settings = settings or get_settings()
    return bool(settings.airbyte_client_id and settings.airbyte_client_secret)


def hosted_connect_kwargs(settings: Settings | None = None) -> dict[str, Any]:
    settings = settings or get_settings()
    kwargs: dict[str, Any] = {
        "client_id": settings.airbyte_client_id,
        "client_secret": settings.airbyte_client_secret,
    }
    if settings.airbyte_workspace_name:
        kwargs["workspace_name"] = settings.airbyte_workspace_name
    if settings.airbyte_organization_id:
        kwargs["organization_id"] = settings.airbyte_organization_id
    return kwargs


def run_async(coro: Any) -> Any:
    """Run an async SDK coroutine from synchronous agent code."""
    try:
        asyncio.get_running_loop()
    except RuntimeError:
        return asyncio.run(coro)
    with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
        return pool.submit(asyncio.run, coro).result()


def serialize_sdk_result(result: Any) -> dict[str, Any]:
    if result is None:
        return {}
    if hasattr(result, "model_dump"):
        payload = result.model_dump(mode="json")
        if isinstance(payload, dict):
            data = payload.get("data")
            if data is not None and set(payload.keys()) <= {"data", "meta", "status", "success"}:
                if isinstance(data, dict):
                    return data
                if isinstance(data, list):
                    return {"items": data, "data": data}
                return {"data": data}
        return payload if isinstance(payload, dict) else {"value": payload}
    if hasattr(result, "data"):
        data = result.data
        if hasattr(data, "model_dump"):
            return data.model_dump(mode="json")
        if isinstance(data, list):
            return {"items": data, "data": data}
        if isinstance(data, dict):
            return data
        return {"data": data}
    if isinstance(result, dict):
        return result
    return {"value": result}


async def _execute_with_connector(
    connector: Any,
    entity: str,
    action: str,
    params: dict[str, Any],
) -> dict[str, Any]:
    try:
        result = await connector.execute(entity, action, params=params)
        return serialize_sdk_result(result)
    finally:
        await connector.close()


def execute_operation(
    slug: str,
    entity: str,
    action: str,
    params: dict[str, Any] | None = None,
    *,
    local_auth: Any | None = None,
    connector_cls: type | None = None,
    settings: Settings | None = None,
) -> dict[str, Any]:
    """Execute a connector operation via hosted Airbyte or direct local auth."""
    params = params or {}
    settings = settings or get_settings()

    async def _run() -> dict[str, Any]:
        if local_auth is not None:
            if connector_cls is None:
                raise ValueError("connector_cls is required when local_auth is provided")
            connector = connector_cls(auth_config=local_auth)
            return await _execute_with_connector(connector, entity, action, params)

        if not airbyte_hosted_configured(settings):
            raise RuntimeError(
                f"Airbyte hosted credentials are required to execute {slug}.{entity}.{action}"
            )
        connector = connect(slug, **hosted_connect_kwargs(settings))
        return await _execute_with_connector(connector, entity, action, params)

    return run_async(_run())
