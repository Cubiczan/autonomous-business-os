from typing import Any


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
        score = 0.0
        reasons: list[str] = []

        if lead.get("email"):
            score += 10
            reasons.append("valid email present")
        if enrichment.get("email_confidence", 0) >= 80:
            score += 15
            reasons.append("high email confidence")
        if enrichment.get("employee_count", 0) >= 20:
            score += 15
            reasons.append("company has meaningful team size")
        if enrichment.get("annual_revenue", 0) >= 1_000_000:
            score += 15
            reasons.append("revenue threshold met")

        title = (lead.get("title") or enrichment.get("title") or "").lower()
        if any(term in title for term in self.ideal_titles):
            score += 20
            reasons.append("decision-maker title")

        company_text = f"{lead.get('company', '')} {enrichment.get('industry', '')}".lower()
        if any(term in company_text for term in self.target_company_terms):
            score += 15
            reasons.append("target industry fit")

        if lead.get("metadata", {}).get("intent_signal"):
            score += 10
            reasons.append("intent signal")

        score = min(score, 100.0)
        return {
            "score": score,
            "tier": "A" if score >= 80 else "B" if score >= 55 else "C",
            "reasons": reasons,
        }
