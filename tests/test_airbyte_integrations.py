from unittest.mock import patch

from app.integrations.crm import CRMClient
from app.integrations.notion import NotionClient
from app.integrations.slack import SlackClient
from app.integrations.stripe_client import StripeClient
from app.integrations.task_management import TaskManagementClient


def test_slack_simulated_without_credentials() -> None:
    client = SlackClient(token=None)
    result = client.post_message("#general", "hello")
    assert result.simulated is True
    assert result.data["channel"] == "#general"


def test_notion_simulated_without_database_id() -> None:
    client = NotionClient(token="ntn_test", database_id=None)
    result = client.create_page("Client onboarding", {"Client": {"title": []}})
    assert result.simulated is True


@patch("app.integrations.slack.execute_operation")
def test_slack_uses_airbyte_sdk(mock_execute) -> None:
    mock_execute.return_value = {"ok": True, "ts": "123.456"}
    client = SlackClient(token="xoxb-test")
    with patch("app.integrations.slack.airbyte_hosted_configured", return_value=False):
        result = client.post_message("#client-onboarding", "Onboarding started")
    assert result.simulated is False
    mock_execute.assert_called_once()
    assert mock_execute.call_args.args[:3] == ("slack", "messages", "create")


@patch("app.integrations.stripe_client.execute_operation")
def test_stripe_list_overdue_maps_invoice_items(mock_execute) -> None:
    mock_execute.return_value = {
        "data": [
            {"id": "in_123", "amount_due": 125000, "customer": "cus_abc", "status": "open"},
        ]
    }
    client = StripeClient(token="sk_test_123")
    with patch("app.integrations.stripe_client.airbyte_hosted_configured", return_value=False):
        result = client.list_overdue_invoices()
    assert result.simulated is False
    assert result.data["items"][0]["invoice_id"] == "in_123"
    assert result.data["items"][0]["amount_cents"] == 125000


@patch("app.integrations.crm.execute_operation")
def test_hubspot_upsert_returns_existing_contact(mock_execute) -> None:
    mock_execute.return_value = {"results": [{"id": "1", "properties": {"email": "a@b.com"}}]}
    client = CRMClient("hubspot", "pat-test")
    with patch("app.integrations.crm.airbyte_hosted_configured", return_value=False):
        result = client.upsert_lead({"email": "a@b.com", "name": "Ada", "company": "Acme"})
    assert result.simulated is False
    assert result.data["id"] == "1"


@patch("app.integrations.crm.execute_operation")
def test_hubspot_upsert_simulates_when_contact_missing(mock_execute) -> None:
    mock_execute.return_value = {"results": []}
    client = CRMClient("hubspot", "pat-test")
    with patch("app.integrations.crm.airbyte_hosted_configured", return_value=False):
        result = client.upsert_lead({"email": "new@b.com", "name": "New", "company": "Acme"})
    assert result.simulated is True
    assert result.data["create_unsupported"] is True


@patch("app.integrations.task_management.execute_operation")
def test_linear_create_task_uses_team_id_from_settings(mock_execute) -> None:
    mock_execute.return_value = {"id": "issue_1", "title": "Task"}
    client = TaskManagementClient("linear", "lin_test", None, None)
    with patch("app.integrations.task_management.airbyte_hosted_configured", return_value=False):
        with patch.object(client._settings, "linear_team_id", "team_abc"):
            result = client.create_task("Kickoff", "Prepare agenda", {"client": "Acme"})
    assert result.simulated is False
    params = mock_execute.call_args.args[3]
    assert params["team_id"] == "team_abc"
