from typing import Any

from sqlalchemy.orm import Session

from app.agents.base import BaseAgent
from app.config import get_settings
from app.integrations.accounting import AccountingClient
from app.integrations.email import EmailClient
from app.integrations.stripe_client import StripeClient
from app.models import Workflow, WorkflowStatus
from app.services.approval import ApprovalService
from app.services.guardrails import GuardrailService
from app.services.memory import MemoryService


class FinanceOperationsAgent(BaseAgent):
    name = "finance_operations_agent"

    def __init__(self, session: Session):
        super().__init__(session)
        settings = get_settings()
        accounting_token = (
            settings.quickbooks_access_token
            if settings.accounting_provider == "quickbooks"
            else settings.xero_access_token
        )
        self.stripe = StripeClient(settings.stripe_api_key)
        self.accounting = AccountingClient(settings.accounting_provider, accounting_token)
        self.email = EmailClient(
            settings.smtp_host,
            settings.smtp_port,
            settings.smtp_username,
            settings.smtp_password,
            settings.smtp_from,
        )
        self.memory = MemoryService(session)
        self.approvals = ApprovalService(session)
        self.guardrails = GuardrailService(session)

    def run(self, workflow: Workflow) -> dict[str, Any]:
        payload = workflow.payload
        invoice_action = self.guardrails.request_external_action(
            workflow=workflow,
            action_type="invoice_create",
            summary=f"Create invoice for {payload.get('customer_id', 'customer')}",
            payload=payload,
        )
        if invoice_action["requires_approval"]:
            return {
                "invoice": None,
                "invoice_action": invoice_action,
                "summary": "Invoice creation is waiting for human approval.",
            }
        invoice = self.execute_tool(
            workflow,
            "stripe_create_invoice",
            payload,
            lambda: self.stripe.create_invoice(payload),
        )
        overdue = self.execute_tool(
            workflow,
            "stripe_list_overdue_invoices",
            {},
            self.stripe.list_overdue_invoices,
        )
        reconciliation = self.execute_tool(
            workflow,
            "accounting_reconcile_payments",
            {},
            self.accounting.reconcile_payments,
        )
        summary = self._weekly_summary(invoice, overdue, reconciliation)
        self.memory.set("finance", "weekly_summary", summary, text=summary["summary"])

        approval = None
        if overdue.get("data", {}).get("items"):
            workflow.status = WorkflowStatus.waiting_for_human
            self.session.commit()
            approval = self.approvals.request(
                workflow.id,
                "Approve overdue payment follow-up",
                "One or more overdue payment reminders are ready to send.",
                {"overdue": overdue.get("data", {}).get("items")},
            )

        return {
            "invoice": invoice,
            "overdue": overdue,
            "reconciliation": reconciliation,
            "summary": summary,
            "approval_id": approval.id if approval else None,
        }

    def _weekly_summary(
        self,
        invoice: dict[str, Any],
        overdue: dict[str, Any],
        reconciliation: dict[str, Any],
    ) -> dict[str, Any]:
        overdue_items = overdue.get("data", {}).get("items", [])
        return {
            "summary": (
                f"Invoice workflow completed. {len(overdue_items)} overdue invoices found. "
                f"Reconciliation result: {reconciliation.get('data', {})}."
            ),
            "overdue_count": len(overdue_items),
            "invoice_simulated": invoice.get("simulated", False),
        }
