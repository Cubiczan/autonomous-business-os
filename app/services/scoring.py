from typing import Any

from app.rust_core import run_abos_core


class LeadScoringService:
    """Deterministic lead scoring that can be replaced by a model-backed scorer."""

    ideal_titles = {
        "founder",
        "ceo",
        "coo",
        "cto",
        "vp",
        "head",
        "director",
        "partner",
        "owner",
    }
    target_company_terms = {"ai", "software", "saas", "consulting", "agency", "fintech", "health"}

    def score(self, lead: dict[str, Any], enrichment: dict[str, Any]) -> dict[str, Any]:
        payload = run_abos_core("score_lead", {"lead": lead, "enrichment": enrichment})
        return payload["value"]
