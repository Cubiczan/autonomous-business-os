from typing import Any

from sqlalchemy import select
from sqlalchemy.orm import Session

from app.agents.base import BaseAgent
from app.models import AgentInstance, AgentRuntimeStatus, Department, DepartmentStatus, Workflow
from app.services.guardrails import GuardrailService
from app.services.memory import MemoryService


class DynamicDepartmentAgent(BaseAgent):
    name = "dynamic_department_agent"

    def __init__(self, session: Session):
        super().__init__(session)
        self.guardrails = GuardrailService(session)
        self.memory = MemoryService(session)

    def run(self, workflow: Workflow) -> dict[str, Any]:
        department_id = workflow.payload["department_id"]
        department = self.session.get(Department, department_id)
        if not department:
            raise ValueError(f"Department not found: {department_id}")
        if department.status != DepartmentStatus.active:
            return {
                "department_id": department.id,
                "status": department.status.value,
                "message": "Department is not active; operating loop skipped.",
            }

        agents = self.session.scalars(
            select(AgentInstance).where(AgentInstance.department_id == department.id)
        ).all()
        for agent in agents:
            agent.status = AgentRuntimeStatus.running
            agent.current_task = f"{workflow.payload.get('mode', 'daily')} operating loop"
        self.session.commit()

        inspection = self.guardrails.inspect_untrusted_text(
            str(workflow.payload.get("external_context", "")),
            source=f"workflow:{workflow.id}:external_context",
            workflow=workflow,
        )
        output = self._generate_department_output(department, workflow.payload, inspection)
        approvals = self._propose_external_actions(department, workflow, output)
        department.last_output = output
        department.revenue_signals = self._revenue_signals(department, output)
        department.health_score = self._health_score(department, approvals)
        for agent in agents:
            agent.status = (
                AgentRuntimeStatus.waiting_for_approval if approvals else AgentRuntimeStatus.idle
            )
            agent.current_task = None
            agent.last_output = {"workflow_id": workflow.id, "summary": output["summary"]}
        self.session.commit()
        self.memory.set(
            f"department:{department.id}",
            f"workflow:{workflow.id}",
            output,
            text=output["summary"],
        )
        return {
            "department_id": department.id,
            "department": department.name,
            "mode": workflow.payload.get("mode", "daily"),
            "output": output,
            "approval_requests": approvals,
        }

    def _generate_department_output(
        self,
        department: Department,
        payload: dict[str, Any],
        inspection: dict[str, Any],
    ) -> dict[str, Any]:
        mode = payload.get("mode", "daily")
        if department.department_type == "content":
            channels = payload.get(
                "channels",
                ["linkedin", "x", "instagram", "facebook", "youtube", "newsletter", "blog", "ads"],
            )
            ideas = [
                "Why autonomous departments beat task-level automation",
                "The approval layer that keeps AI business ops safe",
                "How to measure an AI agent team without babysitting it",
            ]
            drafts = [
                {
                    "channel": channel,
                    "title": self._content_title(channel),
                    "body": self._content_body(channel, department.name),
                    "status": "draft_ready_for_approval",
                }
                for channel in channels
            ]
            return {
                "summary": f"{department.name} prepared {len(drafts)} content drafts for approval.",
                "mode": mode,
                "ideas": ideas,
                "research_notes": [
                    "Autonomy should be measured by closed-loop completion, not agent count.",
                    "Approval gates are needed at publishing, contracts, payments, and destructive actions.",
                    "Department isolation reduces accidental cross-contamination between business units.",
                ],
                "drafts": drafts,
                "prompt_injection_inspection": inspection,
            }
        if department.department_type == "sales":
            leads = [
                {
                    "company": "Acme Automation",
                    "contact_role": "VP Operations",
                    "score": 84,
                    "reason": "Clear operational workflow complexity and likely automation budget.",
                },
                {
                    "company": "Northstar Clinics",
                    "contact_role": "COO",
                    "score": 77,
                    "reason": "High follow-up volume and customer-success coordination needs.",
                },
            ]
            outreach = [
                {
                    "company": lead["company"],
                    "subject": f"Autonomous operations idea for {lead['company']}",
                    "body": "Drafted outreach sequence ready for review before sending.",
                }
                for lead in leads
            ]
            return {
                "summary": f"{department.name} found {len(leads)} lead candidates and drafted outreach.",
                "mode": mode,
                "leads": leads,
                "outreach": outreach,
                "prompt_injection_inspection": inspection,
            }
        if department.department_type == "intelligence":
            return {
                "summary": f"{department.name} completed market and competitor signal scan.",
                "mode": mode,
                "signals": [
                    {"type": "competitor", "severity": "medium", "note": "New AI ops positioning trend."},
                    {"type": "market", "severity": "low", "note": "Growing demand for approval-first agents."},
                ],
                "prompt_injection_inspection": inspection,
            }
        return {
            "summary": f"{department.name} completed its {mode} operating loop.",
            "mode": mode,
            "actions": ["Reviewed goals", "Updated task queue", "Prepared upward report"],
            "prompt_injection_inspection": inspection,
        }

    def _propose_external_actions(
        self,
        department: Department,
        workflow: Workflow,
        output: dict[str, Any],
    ) -> list[dict[str, Any]]:
        proposals: list[dict[str, Any]] = []
        if department.department_type == "content":
            for draft in output.get("drafts", []):
                proposals.append(
                    self.guardrails.request_external_action(
                        workflow=workflow,
                        action_type=f"publish_post:{draft['channel']}",
                        summary=f"Publish {draft['channel']} content draft",
                        payload=draft,
                    )
                )
        if department.department_type == "sales":
            for draft in output.get("outreach", []):
                proposals.append(
                    self.guardrails.request_external_action(
                        workflow=workflow,
                        action_type="outbound_email:sales_outreach",
                        summary=f"Send sales outreach to {draft['company']}",
                        payload=draft,
                    )
                )
        return proposals

    def _revenue_signals(self, department: Department, output: dict[str, Any]) -> dict[str, Any]:
        signals = dict(department.revenue_signals or {})
        if department.department_type == "sales":
            leads = output.get("leads", [])
            signals["lead_count"] = signals.get("lead_count", 0) + len(leads)
            signals["pipeline_value"] = signals.get("pipeline_value", 0) + len(leads) * 5000
        if department.department_type == "content":
            drafts = output.get("drafts", [])
            signals["content_outputs"] = signals.get("content_outputs", 0) + len(drafts)
        return signals

    def _health_score(self, department: Department, approvals: list[dict[str, Any]]) -> float:
        score = 0.95 if department.status == DepartmentStatus.active else 0.4
        if len(approvals) > 10:
            score -= 0.1
        return max(0.0, min(1.0, score))

    def _content_title(self, channel: str) -> str:
        if channel == "youtube":
            return "How Autonomous AI Departments Actually Work"
        if channel == "newsletter":
            return "This Week in Autonomous Business Ops"
        if channel == "blog":
            return "Designing Self-Spawning AI Departments"
        if channel == "ads":
            return "Stop Babysitting Automations"
        return "Autonomous Business Systems Need Approval Gates"

    def _content_body(self, channel: str, department_name: str) -> str:
        if channel == "youtube":
            return (
                "Hook, outline, full script, description, chapters, and thumbnail brief generated "
                f"by {department_name}. Human approval required before publication."
            )
        if channel == "newsletter":
            return (
                "Newsletter draft covering autonomy metrics, approval queues, audit logs, and "
                "department health signals. Human approval required before send."
            )
        return (
            "Draft explains why autonomous departments need clear goals, scoped memory, approval "
            "gates, audit logs, and kill switches before they should touch real customers."
        )
