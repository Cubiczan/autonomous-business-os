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
from app.services.evidence import EvidencePacketService
from app.rust_core import run_abos_core


class PromptInjectionGuard:
    def inspect(self, text: str, *, source: str = "unknown") -> dict[str, Any]:
        payload = run_abos_core("inspect_text", {"text": text, "source": source})
        return payload["value"]


class GuardrailService:
    def __init__(self, session: Session):
        self.session = session
        self.audit = AuditService(session)
        self.approvals = ApprovalService(session)
        self.evidence = EvidencePacketService(session)
        self.prompt_guard = PromptInjectionGuard()

    def classify_action(self, action_type: str, payload: dict[str, Any]) -> dict[str, Any]:
        payload = run_abos_core("classify_action", {"action_type": action_type, "payload": payload})
        return payload["value"]

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
        packet = self.evidence.record_external_action_packet(
            external_action=external_action,
            classification=classification,
            workflow=workflow,
            agent=agent,
        )
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
                "evidence_packet_id": packet.id,
            },
        )
        return {
            "external_action_id": external_action.id,
            "approval_id": approval.id if approval else None,
            "evidence_packet_id": packet.id,
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
