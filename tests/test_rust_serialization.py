from app.rust_core import serialize_department, serialize_sdk_result


def test_serialize_sdk_result_normalizes_list_payload() -> None:
    result = serialize_sdk_result([{"id": "in_123"}])

    assert result["items"] == [{"id": "in_123"}]
    assert result["data"] == [{"id": "in_123"}]


def test_serialize_sdk_result_passes_through_wrapped_data() -> None:
    result = serialize_sdk_result({"data": {"id": "abc"}, "status": "ok"})

    assert result == {"id": "abc"}


def test_serialize_department_round_trips_payload() -> None:
    payload = {
        "id": "dep_1",
        "name": "Growth",
        "department_type": "sales",
        "purpose": "Drive pipeline",
        "goals": ["book meetings"],
        "operating_rules": ["approval first"],
        "status": "active",
        "health_score": 0.95,
        "revenue_signals": {"lead_count": 2},
        "last_output": {"summary": "ok"},
        "agents": [{"id": "a1", "name": "Agent", "role": "sales", "skills": ["crm"]}],
        "schedules": [{"id": "s1", "name": "Daily", "enabled": True}],
    }

    assert serialize_department(payload) == payload
