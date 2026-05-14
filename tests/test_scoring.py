from app.services.scoring import LeadScoringService


def test_scores_decision_maker_with_company_fit_highly() -> None:
    scorer = LeadScoringService()

    result = scorer.score(
        {
            "email": "founder@example.com",
            "title": "Founder",
            "company": "Example AI",
            "metadata": {"intent_signal": True},
        },
        {"email_confidence": 92, "employee_count": 50, "annual_revenue": 2_000_000},
    )

    assert result["score"] >= 80
    assert result["tier"] == "A"
    assert "decision-maker title" in result["reasons"]


def test_scores_sparse_lead_as_c_tier() -> None:
    scorer = LeadScoringService()

    result = scorer.score({"email": "person@example.com"}, {"email_confidence": 50})

    assert result["score"] < 55
    assert result["tier"] == "C"
