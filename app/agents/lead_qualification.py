from typing import Any

from sqlalchemy.orm import Session

from app.agents.base import BaseAgent
from app.config import get_settings
from app.integrations.apollo import ApolloClient
from app.integrations.crm import CRMClient
from app.integrations.email import EmailClient
from app.integrations.hunter import HunterClient
from app.models import Lead, Workflow, WorkflowStatus
from app.services.approval import ApprovalService
from app.services.memory import MemoryService
from app.services.scoring import LeadScoringService


class LeadQualificationAgent(BaseAgent):
    name = "lead_qualification_agent"

    def __init__(self, session: Session):
        super().__init__(session)
        settings = get_settings()
        crm_token = (
            settings.hubspot_access_token
            if settings.crm_provider == "hubspot"
            else settings.salesforce_access_token
        )
        self.apollo = ApolloClient(settings.apollo_api_key)
        self.hunter = HunterClient(settings.hunter_api_key)
        self.crm = CRMClient(settings.crm_provider, crm_token)
        self.email = EmailClient(
            settings.smtp_host,
            settings.smtp_port,
            settings.smtp_username,
            settings.smtp_password,
            settings.smtp_from,
        )
        self.scoring = LeadScoringService()
        self.memory = MemoryService(session)
        self.approvals = ApprovalService(session)

    def run(self, workflow: Workflow) -> dict[str, Any]:
        lead_payload = workflow.payload
        email = lead_payload["email"]

        apollo = self.execute_tool(
            workflow,
            "apollo_enrich_person",
            {"email": email},
            lambda: self.apollo.enrich_person(email),
        )
        hunter = self.execute_tool(
            workflow,
            "hunter_verify_email",
            {"email": email},
            lambda: self.hunter.verify_email(email),
        )

        enrichment = {**apollo.get("data", {}), **hunter.get("data", {})}
        scoring = self.scoring.score(lead_payload, enrichment)
        outreach = self._generate_outreach(lead_payload, scoring)

        crm_result = self.execute_tool(
            workflow,
            "crm_upsert_lead",
            {"email": email, "score": scoring["score"]},
            lambda: self.crm.upsert_lead({**lead_payload, "score": scoring["score"]}),
        )
        draft = self.execute_tool(
            workflow,
            "draft_outreach",
            {"email": email, "tier": scoring["tier"]},
            lambda: self.email.draft_outreach(email, outreach["subject"], outreach["body"]),
        )

        lead = Lead(
            email=email,
            name=lead_payload.get("name") or enrichment.get("name") or "",
            company=lead_payload.get("company") or enrichment.get("company") or "",
            source=lead_payload.get("source", workflow.source),
            score=scoring["score"],
            enrichment=enrichment,
            outreach=outreach,
        )
        self.session.add(lead)
        self.session.commit()
        self.memory.set(
            "leads",
            email,
            {
                "lead_id": lead.id,
                "score": scoring["score"],
                "tier": scoring["tier"],
                "crm": crm_result,
            },
            text=f"{lead.name} at {lead.company} scored {scoring['score']}.",
        )

        approval = None
        if scoring["tier"] == "A":
            workflow.status = WorkflowStatus.waiting_for_human
            self.session.commit()
            approval = self.approvals.request(
                workflow.id,
                f"Approve outreach to {email}",
                "Tier A lead outreach should be reviewed before sending.",
                {"email": email, "outreach": outreach, "lead_id": lead.id},
            )

        return {
            "lead_id": lead.id,
            "score": scoring,
            "enrichment": enrichment,
            "outreach": outreach,
            "crm": crm_result,
            "draft": draft,
            "approval_id": approval.id if approval else None,
        }

    def _generate_outreach(self, lead: dict[str, Any], scoring: dict[str, Any]) -> dict[str, str]:
        name = lead.get("name") or "there"
        company = lead.get("company") or "your team"
        subject = f"Idea for {company}"
        body = (
            f"Hi {name},\n\n"
            f"I noticed {company} looks like a strong fit for operational automation. "
            f"Based on the signals we saw ({', '.join(scoring['reasons'])}), there may be "
            "a practical opportunity to reduce manual sales, onboarding, delivery, and finance work.\n\n"
            "Worth a quick conversation this week?\n"
        )
        return {"subject": subject, "body": body}
