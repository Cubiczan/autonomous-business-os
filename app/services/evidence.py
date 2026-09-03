from typing import Any

from sqlalchemy.orm import Session

from app.models import AgentInstance, AuditAction, EvidencePacket, ExternalAction, Workflow
from app.services.audit import AuditService
from app.rust_core import run_abos_core


VGA_ATTRIBUTION = (
    "Governance evidence-packet pattern adapted from Georgios Fradelos, PhD, "
    "Verifiable Governance Architecture (VGA) for Organisations and Teams with Human "
    "and AI Employees, Geneva, January 9, 2026, local source "
    "AI Governance papers/ssrn-6306840.pdf; and Finance-Grade Assurance for "
    "Agentic AI, Geneva, January 11, 2026, local source "
    "AI Governance papers/ssrn-6306980.pdf."
)


class EvidencePacketService:
    def __init__(self, session: Session):
        self.session = session
        self.audit = AuditService(session)

    def record_external_action_packet(
        self,
        *,
        external_action: ExternalAction,
        classification: dict[str, Any],
        workflow: Workflow | None = None,
        agent: AgentInstance | None = None,
    ) -> EvidencePacket:
        payload_hash = self._hash_json(external_action.payload)
        artifacts = {
            "intent": external_action.summary,
            "action_type": external_action.action_type,
            "risk_level": external_action.risk_level,
            "requires_approval": external_action.requires_approval,
            "approval_id": external_action.approval_id,
            "guardrail_reasons": classification.get("reasons", []),
            "payload_hash": payload_hash,
            "workflow": {
                "id": workflow.id if workflow else None,
                "kind": workflow.kind if workflow else None,
                "source": workflow.source if workflow else None,
            },
            "agent": {
                "id": agent.id if agent else None,
                "role": agent.role if agent else None,
                "trust_level": agent.trust_level.value if agent else None,
            },
            "policy": {
                "mode": "fail-close",
                "tool_boundary": "external_action",
                "human_approval_required": external_action.requires_approval,
            },
        }
        status = "waiting_for_approval" if external_action.requires_approval else "approved_to_execute"
        packet = EvidencePacket(
            workflow_id=workflow.id if workflow else None,
            external_action_id=external_action.id,
            agent_id=agent.id if agent else None,
            packet_type="external_action",
            intent=external_action.summary,
            context_hash=self._hash_json(artifacts),
            status=status,
            artifacts=artifacts,
            attribution=VGA_ATTRIBUTION,
        )
        self.session.add(packet)
        self.session.commit()
        self.audit.record(
            AuditAction.evidence_packet_recorded,
            f"Evidence packet recorded: {external_action.summary}",
            workflow_id=workflow.id if workflow else None,
            metadata={
                "evidence_packet_id": packet.id,
                "external_action_id": external_action.id,
                "context_hash": packet.context_hash,
                "status": packet.status,
            },
        )
        return packet

    def _hash_json(self, payload: dict[str, Any]) -> str:
        response = run_abos_core("hash_json", {"payload": payload})
        return response["value"]["hash"]
