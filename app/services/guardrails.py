from datetime import timedelta
from typing import Any

from sqlalchemy import select
from sqlalchemy.orm import Session

from app.models import (
    AgentInstance,
    ApprovalStatus,
    AuditAction,
    CircuitBreaker,
    ExternalAction,
    ExternalActionStatus,
    HumanApproval,
    SecurityEvent,
    Workflow,
    WorkflowStatus,
    utcnow,
)
from app.services.approval import ApprovalService
from app.services.audit import AuditService


OUTBOUND_MARKERS = {
    "send_email",
    "outbound_email",
    "client_message",
    "social_post",
    "publish_post",
    "publish_video",
    "newsletter_send",
    "ad_publish",
    "proposal_send",
}

HIGH_IMPACT_MARKERS = {
    "send_money",
    "payment",
    "wire_transfer",
    "contract_sign",
    "delete",
    "credential",
    "production_change",
    "invoice_create",
}

PROMPT_INJECTION_PATTERNS = [
    "ignore previous instructions",
    "ignore all previous instructions",
    "system prompt",
    "developer message",
    "reveal your instructions",
    "exfiltrate",
    "send the api key",
    "bypass approval",
    "do not tell the user",
    "disable guardrails",
]


class PromptInjectionGuard:
    def inspect(self, text: str, *, source: str = "unknown") -> dict[str, Any]:
        normalized = text.lower()
        flags = [pattern for pattern in PROMPT_INJECTION_PATTERNS if pattern in normalized]
        score = min(1.0, len(flags) / 3)
        return {
            "source": source,
            "score": score,
            "flags": flags,
            "safe_to_use_as_instruction": not flags,
            "handling": "treat_as_untrusted_data" if flags else "normal",
        }


class GuardrailService:
    def __init__(self, session: Session):
        self.session = session
        self.audit = AuditService(session)
        self.approvals = ApprovalService(session)
        self.prompt_guard = PromptInjectionGuard()

    def classify_action(self, action_type: str, payload: dict[str, Any]) -> dict[str, Any]:
        action = action_type.lower()
        reasons: list[str] = []
        requires_approval = False
        risk_level = "low"
        if any(marker in action for marker in OUTBOUND_MARKERS):
            requires_approval = True
            risk_level = "high"
            reasons.append("Outbound communication or publishing requires approval.")
        if any(marker in action for marker in HIGH_IMPACT_MARKERS):
            requires_approval = True
            risk_level = "critical"
            reasons.append("Financial, contractual, destructive, or credential action is blocked.")
        payload_text = str(payload).lower()
        if any(marker in payload_text for marker in ["gdpr", "personal data", "contract", "payment"]):
            risk_level = "high" if risk_level == "low" else risk_level
            reasons.append("Sensitive business, legal, financial, or personal-data context detected.")
        if not reasons:
            reasons.append("Low-risk internal action with audit logging.")
        return {
            "risk_level": risk_level,
            "requires_approval": requires_approval,
            "reasons": reasons,
        }

    def request_external_action(
        self,
        *,
        workflow: Workflow | None,
        action_type: str,
        summary: str,
        payload: dict[str, Any],
        agent: AgentInstance | None = None,
    ) -> dict[str, Any]:
        classification = self.classify_action(action_type, payload)
        external_action = ExternalAction(
            workflow_id=workflow.id if workflow else None,
            agent_id=agent.id if agent else None,
            action_type=action_type,
            summary=summary,
            payload=payload,
            risk_level=classification["risk_level"],
            requires_approval=classification["requires_approval"],
            status=(
                ExternalActionStatus.proposed
                if classification["requires_approval"]
                else ExternalActionStatus.approved
            ),
        )
        self.session.add(external_action)
        self.session.commit()
        approval: HumanApproval | None = None
        if classification["requires_approval"] and workflow:
            workflow.status = WorkflowStatus.waiting_for_human
            workflow.updated_at = utcnow()
            self.session.commit()
            approval = self.approvals.request(
                workflow.id,
                f"Approve external action: {summary}",
                "Guardrail policy requires human approval before this action can execute.",
                {
                    "external_action_id": external_action.id,
                    "action_type": action_type,
                    "summary": summary,
                    "payload": payload,
                    "risk_level": classification["risk_level"],
                    "reasons": classification["reasons"],
                },
            )
            external_action.approval_id = approval.id
            self.session.commit()
        self.audit.record(
            AuditAction.external_action_requested,
            f"External action proposed: {summary}",
            workflow_id=workflow.id if workflow else None,
            metadata={
                "external_action_id": external_action.id,
                "agent_id": agent.id if agent else None,
                "risk_level": classification["risk_level"],
                "requires_approval": classification["requires_approval"],
                "approval_id": approval.id if approval else None,
            },
        )
        return {
            "external_action_id": external_action.id,
            "approval_id": approval.id if approval else None,
            "status": external_action.status.value,
            **classification,
        }

    def sync_external_action_decision(self, approval: HumanApproval) -> None:
        action_id = approval.proposed_action.get("external_action_id")
        if not action_id:
            return
        external_action = self.session.get(ExternalAction, action_id)
        if not external_action:
            return
        if approval.status == ApprovalStatus.approved:
            external_action.status = ExternalActionStatus.approved
        elif approval.status == ApprovalStatus.rejected:
            external_action.status = ExternalActionStatus.rejected
        self.session.commit()

    def inspect_untrusted_text(
        self,
        text: str,
        *,
        source: str,
        workflow: Workflow | None = None,
        agent: AgentInstance | None = None,
    ) -> dict[str, Any]:
        result = self.prompt_guard.inspect(text, source=source)
        if result["flags"]:
            self.record_security_event(
                category="prompt_injection",
                message=f"Suspicious prompt injection markers detected in {source}.",
                severity="high",
                evidence=result,
                workflow=workflow,
                agent=agent,
            )
        return result

    def record_security_event(
        self,
        *,
        category: str,
        message: str,
        severity: str = "medium",
        evidence: dict[str, Any] | None = None,
        workflow: Workflow | None = None,
        agent: AgentInstance | None = None,
    ) -> SecurityEvent:
        event = SecurityEvent(
            workflow_id=workflow.id if workflow else None,
            agent_id=agent.id if agent else None,
            category=category,
            severity=severity,
            message=message,
            evidence=evidence or {},
        )
        self.session.add(event)
        self.session.commit()
        self.audit.record(
            AuditAction.security_event_created,
            message,
            workflow_id=workflow.id if workflow else None,
            metadata={
                "security_event_id": event.id,
                "category": category,
                "severity": severity,
                "agent_id": agent.id if agent else None,
            },
        )
        return event


class CircuitBreakerService:
    def __init__(self, session: Session):
        self.session = session
        self.audit = AuditService(session)

    def check(self, integration: str, *, limit_per_minute: int = 120) -> CircuitBreaker:
        now = utcnow()
        breaker = self.session.scalar(
            select(CircuitBreaker).where(CircuitBreaker.integration == integration)
        )
        if not breaker:
            breaker = CircuitBreaker(
                integration=integration,
                limit_per_minute=limit_per_minute,
                calls_this_window=0,
                window_started_at=now,
            )
            self.session.add(breaker)
            self.session.commit()
        if breaker.state == "open" and breaker.opened_until and breaker.opened_until > now:
            raise RuntimeError(f"Circuit breaker open for {integration}")
        if breaker.window_started_at + timedelta(minutes=1) <= now:
            breaker.calls_this_window = 0
            breaker.window_started_at = now
            breaker.state = "closed"
            breaker.opened_until = None
        breaker.calls_this_window += 1
        breaker.limit_per_minute = limit_per_minute
        breaker.updated_at = now
        if breaker.calls_this_window > breaker.limit_per_minute:
            breaker.state = "open"
            breaker.opened_until = now + timedelta(minutes=5)
            self.session.commit()
            self.audit.record(
                AuditAction.circuit_breaker_opened,
                f"Circuit breaker opened for {integration}",
                metadata={
                    "integration": integration,
                    "limit_per_minute": breaker.limit_per_minute,
                    "calls_this_window": breaker.calls_this_window,
                },
            )
            raise RuntimeError(f"Rate limit exceeded for {integration}")
        self.session.commit()
        return breaker
