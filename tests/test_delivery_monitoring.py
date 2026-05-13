from app.agents.delivery_monitoring import DeliveryMonitoringAgent


def test_delivery_agent_detects_budget_and_contact_risks() -> None:
    agent = object.__new__(DeliveryMonitoringAgent)

    risks = agent._detect_risks(
        {
            "completion_pct": 35,
            "budget_used_pct": 70,
            "days_since_client_contact": 9,
            "metadata": {"days_until_deadline": 20},
        }
    )

    assert {risk["type"] for risk in risks} == {"budget_drift", "communication_gap"}


def test_delivery_agent_detects_schedule_delay() -> None:
    agent = object.__new__(DeliveryMonitoringAgent)

    risks = agent._detect_risks(
        {
            "completion_pct": 30,
            "budget_used_pct": 35,
            "days_since_client_contact": 1,
            "metadata": {"days_until_deadline": 10},
        }
    )

    assert any(risk["type"] == "schedule_delay" for risk in risks)
