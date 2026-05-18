import re
from pathlib import Path
from typing import Any

from sqlalchemy import select
from sqlalchemy.orm import Session

from app.config import get_settings
from app.models import (
    AgentInstance,
    AgentSkillAssignment,
    AuditAction,
    Skill,
    SkillScope,
    SkillStatus,
    utcnow,
)
from app.services.audit import AuditService


CORE_SKILLS: list[dict[str, Any]] = [
    {
        "slug": "market_research",
        "name": "Market Research",
        "description": "Scan markets, competitors, customer segments, and trend signals.",
        "tags": ["research", "strategy", "intelligence"],
        "tool_permissions": ["web_research", "rag_search", "competitive_analysis"],
    },
    {
        "slug": "lead_generation",
        "name": "Lead Generation",
        "description": "Find and qualify prospect accounts and contacts.",
        "tags": ["sales", "prospecting", "crm"],
        "tool_permissions": ["apollo", "hunter", "crm"],
    },
    {
        "slug": "cold_email_outreach",
        "name": "Cold Email Outreach",
        "description": "Draft compliant prospecting emails and follow-up sequences.",
        "tags": ["sales", "outreach", "email"],
        "tool_permissions": ["email_draft"],
        "approval_policy": "all_outbound_requires_human_approval",
    },
    {
        "slug": "pipeline_tracking",
        "name": "Pipeline Tracking",
        "description": "Track stages, blockers, revenue signals, and next actions in the CRM.",
        "tags": ["sales", "crm", "analytics"],
        "tool_permissions": ["crm", "analytics"],
    },
    {
        "slug": "content_strategy",
        "name": "Content Strategy",
        "description": "Plan channel-specific calendars, angles, campaigns, and messaging pillars.",
        "tags": ["content", "marketing", "strategy"],
        "tool_permissions": ["calendar", "analytics"],
    },
    {
        "slug": "content_drafting",
        "name": "Content Drafting",
        "description": "Write posts, articles, threads, ads, scripts, newsletters, and landing copy.",
        "tags": ["content", "copywriting", "marketing"],
        "tool_permissions": ["document_generation"],
        "approval_policy": "publishing_requires_human_approval",
    },
    {
        "slug": "social_scheduling",
        "name": "Social Scheduling",
        "description": "Prepare social posts for LinkedIn, X, Instagram, Facebook, and blogs.",
        "tags": ["content", "publishing", "social"],
        "tool_permissions": ["social_draft", "calendar"],
        "approval_policy": "publishing_requires_human_approval",
    },
    {
        "slug": "youtube_scriptwriting",
        "name": "YouTube Scriptwriting",
        "description": "Create hooks, outlines, scripts, descriptions, chapters, and thumbnail briefs.",
        "tags": ["content", "youtube", "video"],
        "tool_permissions": ["document_generation", "analytics"],
    },
    {
        "slug": "newsletter_production",
        "name": "Newsletter Production",
        "description": "Draft subject lines, newsletter issues, segments, and email sequences.",
        "tags": ["content", "email", "newsletter"],
        "tool_permissions": ["email_draft"],
        "approval_policy": "all_outbound_requires_human_approval",
    },
    {
        "slug": "crm_management",
        "name": "CRM Management",
        "description": "Update accounts, contacts, deal stages, notes, and follow-up tasks.",
        "tags": ["sales", "ops", "crm"],
        "tool_permissions": ["crm"],
    },
    {
        "slug": "analytics_reporting",
        "name": "Analytics Reporting",
        "description": "Analyze performance data and produce daily, weekly, and monthly reports.",
        "tags": ["analytics", "reporting", "ops"],
        "tool_permissions": ["analytics", "rag_search"],
    },
    {
        "slug": "customer_success",
        "name": "Customer Success",
        "description": "Track client health, follow-ups, satisfaction signals, and renewals.",
        "tags": ["success", "client", "retention"],
        "tool_permissions": ["crm", "email_draft", "calendar"],
        "approval_policy": "client_messages_require_human_approval",
    },
    {
        "slug": "finance_reporting",
        "name": "Finance Reporting",
        "description": "Monitor revenue, costs, invoices, collections, and financial anomalies.",
        "tags": ["finance", "reporting", "risk"],
        "tool_permissions": ["stripe", "accounting"],
    },
    {
        "slug": "compliance_review",
        "name": "Compliance Review",
        "description": "Review data handling, GDPR risk, contracts, claims, and approvals.",
        "tags": ["legal", "compliance", "risk"],
        "tool_permissions": ["policy_check", "audit_log"],
    },
    {
        "slug": "approval_routing",
        "name": "Approval Routing",
        "description": "Route outbound, financial, contract, and destructive actions to humans.",
        "tags": ["security", "approval", "governance"],
        "tool_permissions": ["approval_queue", "audit_log"],
    },
    {
        "slug": "prompt_injection_defense",
        "name": "Prompt Injection Defense",
        "description": "Detect untrusted instructions in web pages, emails, and documents.",
        "tags": ["security", "llm", "guardrails"],
        "tool_permissions": ["security_event"],
    },
]


def slugify(value: str) -> str:
    slug = re.sub(r"[^a-z0-9]+", "_", value.lower()).strip("_")
    return slug[:80] or "custom_skill"


class SkillRegistry:
    def __init__(self, session: Session):
        self.session = session
        self.audit = AuditService(session)

    def seed_core_skills(self) -> list[Skill]:
        skills: list[Skill] = []
        for manifest in CORE_SKILLS:
            skill = self.session.scalar(
                select(Skill).where(
                    Skill.slug == manifest["slug"],
                    Skill.scope == SkillScope.company,
                    Skill.department_id.is_(None),
                    Skill.agent_id.is_(None),
                )
            )
            if not skill:
                skill = Skill(
                    name=manifest["name"],
                    slug=manifest["slug"],
                    description=manifest["description"],
                    scope=SkillScope.company,
                    manifest=manifest,
                )
                self.session.add(skill)
                self.session.commit()
                self.audit.record(
                    AuditAction.skill_registered,
                    f"Core skill registered: {skill.name}",
                    metadata={"skill_id": skill.id, "slug": skill.slug},
                )
            skills.append(skill)
        return skills

    def create_from_description(
        self,
        description: str,
        *,
        name: str | None = None,
        scope: SkillScope = SkillScope.company,
        department_id: str | None = None,
        agent_id: str | None = None,
    ) -> Skill:
        skill_name = name or self._title_from_description(description)
        slug = slugify(skill_name)
        manifest = {
            "slug": slug,
            "name": skill_name,
            "description": description,
            "tags": self._infer_tags(description),
            "tool_permissions": self._infer_tools(description),
            "approval_policy": self._infer_approval_policy(description),
            "runtime_contract": {
                "input": "Plain-language task, memory context, allowed tools, and trust level.",
                "output": "Structured result with proposed actions, citations, risks, and next run.",
            },
            "created_from_plain_language": True,
        }
        skill = Skill(
            name=skill_name,
            slug=slug,
            description=description,
            scope=scope,
            department_id=department_id,
            agent_id=agent_id,
            manifest=manifest,
        )
        self.session.add(skill)
        self.session.commit()
        self.audit.record(
            AuditAction.skill_registered,
            f"Skill registered from plain language: {skill.name}",
            metadata={
                "skill_id": skill.id,
                "scope": scope.value,
                "department_id": department_id,
                "agent_id": agent_id,
            },
        )
        return skill

    def refresh_from_manifest_directory(self) -> list[Skill]:
        settings = get_settings()
        directory = settings.storage_dir / "skills"
        directory.mkdir(parents=True, exist_ok=True)
        registered: list[Skill] = []
        for path in sorted(directory.glob("*.json")):
            skill = self.register_manifest_file(path)
            registered.append(skill)
        return registered

    def register_manifest_file(self, path: Path) -> Skill:
        import json

        manifest = json.loads(path.read_text(encoding="utf-8"))
        slug = slugify(manifest.get("slug") or manifest.get("name") or path.stem)
        existing = self.session.scalar(
            select(Skill).where(
                Skill.slug == slug,
                Skill.department_id == manifest.get("department_id"),
                Skill.agent_id == manifest.get("agent_id"),
            )
        )
        if existing:
            existing.name = manifest.get("name", existing.name)
            existing.description = manifest.get("description", existing.description)
            existing.version = manifest.get("version", existing.version)
            existing.manifest = {**existing.manifest, **manifest, "slug": slug}
            existing.updated_at = utcnow()
            skill = existing
        else:
            skill = Skill(
                name=manifest.get("name", slug.replace("_", " ").title()),
                slug=slug,
                version=manifest.get("version", "1.0.0"),
                description=manifest.get("description", ""),
                scope=SkillScope(manifest.get("scope", SkillScope.company.value)),
                department_id=manifest.get("department_id"),
                agent_id=manifest.get("agent_id"),
                manifest={**manifest, "slug": slug},
            )
            self.session.add(skill)
        self.session.commit()
        self.audit.record(
            AuditAction.skill_registered,
            f"Skill manifest loaded: {skill.name}",
            metadata={"skill_id": skill.id, "path": str(path)},
        )
        return skill

    def list_active(self) -> list[Skill]:
        self.seed_core_skills()
        return list(
            self.session.scalars(
                select(Skill).where(Skill.status == SkillStatus.active).order_by(Skill.name)
            )
        )

    def assign_relevant_skills(self, agent: AgentInstance, department_type: str) -> list[Skill]:
        self.seed_core_skills()
        slugs = self._skill_slugs_for_agent(agent.role, department_type)
        skills = list(
            self.session.scalars(
                select(Skill).where(Skill.status == SkillStatus.active, Skill.slug.in_(slugs))
            )
        )
        assigned: list[Skill] = []
        for skill in skills:
            existing = self.session.scalar(
                select(AgentSkillAssignment).where(
                    AgentSkillAssignment.agent_id == agent.id,
                    AgentSkillAssignment.skill_id == skill.id,
                )
            )
            if existing:
                assigned.append(skill)
                continue
            self.session.add(AgentSkillAssignment(agent_id=agent.id, skill_id=skill.id))
            self.session.commit()
            self.audit.record(
                AuditAction.skill_assigned,
                f"Assigned {skill.name} to {agent.name}",
                metadata={"agent_id": agent.id, "skill_id": skill.id},
            )
            assigned.append(skill)
        return assigned

    def resolve_agent_skills(self, agent_id: str) -> list[Skill]:
        return list(
            self.session.scalars(
                select(Skill)
                .join(AgentSkillAssignment)
                .where(
                    AgentSkillAssignment.agent_id == agent_id,
                    Skill.status == SkillStatus.active,
                )
                .order_by(Skill.name)
            )
        )

    def _title_from_description(self, description: str) -> str:
        words = [word.strip(".,:;()[]{}") for word in description.split() if len(word) > 2]
        return " ".join(words[:5]).title() or "Custom Skill"

    def _infer_tags(self, description: str) -> list[str]:
        text = description.lower()
        tags = []
        for tag in [
            "sales",
            "content",
            "email",
            "social",
            "finance",
            "legal",
            "security",
            "research",
            "analytics",
            "customer",
            "youtube",
            "newsletter",
        ]:
            if tag in text:
                tags.append(tag)
        return tags or ["custom"]

    def _infer_tools(self, description: str) -> list[str]:
        text = description.lower()
        tools = ["memory"]
        tool_map = {
            "email": "email_draft",
            "social": "social_draft",
            "linkedin": "social_draft",
            "twitter": "social_draft",
            "x/": "social_draft",
            "youtube": "document_generation",
            "crm": "crm",
            "lead": "apollo",
            "invoice": "stripe",
            "finance": "accounting",
            "calendar": "calendar",
            "research": "web_research",
        }
        for marker, tool in tool_map.items():
            if marker in text and tool not in tools:
                tools.append(tool)
        return tools

    def _infer_approval_policy(self, description: str) -> str:
        text = description.lower()
        if any(
            marker in text
            for marker in ["send", "publish", "post", "email", "message", "approval", "approve"]
        ):
            return "human_approval_required_before_external_action"
        if any(marker in text for marker in ["money", "payment", "contract", "delete"]):
            return "human_approval_required_before_high_impact_action"
        return "autonomous_with_audit"

    def _skill_slugs_for_agent(self, role: str, department_type: str) -> list[str]:
        role_text = role.lower()
        slugs = {"analytics_reporting", "approval_routing", "prompt_injection_defense"}
        if "ceo" in role_text or "strategy" in role_text:
            slugs.update({"market_research", "analytics_reporting", "compliance_review"})
        if "sales" in role_text or "outreach" in role_text or department_type == "sales":
            slugs.update({"lead_generation", "cold_email_outreach", "crm_management", "pipeline_tracking"})
        if "content" in role_text or "creator" in role_text or department_type == "content":
            slugs.update({"content_strategy", "content_drafting", "social_scheduling"})
        if "youtube" in role_text:
            slugs.add("youtube_scriptwriting")
        if "newsletter" in role_text:
            slugs.add("newsletter_production")
        if "research" in role_text or "analyst" in role_text or department_type == "intelligence":
            slugs.update({"market_research", "analytics_reporting"})
        if "operations" in role_text:
            slugs.update({"analytics_reporting", "compliance_review"})
        if "customer" in role_text or "success" in role_text:
            slugs.add("customer_success")
        if "finance" in role_text or department_type == "finance":
            slugs.add("finance_reporting")
        if "legal" in role_text or "compliance" in role_text:
            slugs.add("compliance_review")
        return sorted(slugs)
