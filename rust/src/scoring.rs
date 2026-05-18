//! # Lead Scoring Engine
//!
//! Deterministic, data-driven lead scoring with configurable signals,
//! thresholds, and a per-lead breakdown of how the score was computed.

use crate::types::*;

// ── Configuration ────────────────────────────────────────────────────

/// Configurable scoring weights and thresholds.
#[derive(Debug, Clone)]
pub struct ScoringConfig {
    /// Points awarded when the lead has a valid (non-empty) email.
    pub points_valid_email: u32,
    /// Points awarded when `enrichment.email_confidence >= email_confidence_threshold`.
    pub points_high_email_confidence: u32,
    /// Points awarded when `enrichment.employee_count >= min_team_size`.
    pub points_team_size: u32,
    /// Points awarded when `enrichment.annual_revenue >= min_revenue_cents`.
    pub points_revenue: u32,
    /// Points awarded when the contact's title/name contains a decision-maker keyword.
    pub points_decision_maker: u32,
    /// Points awarded when the company or enrichment industry matches a target industry.
    pub points_target_industry: u32,
    /// Points awarded when `enrichment.intent_signal` is truthy.
    pub points_intent_signal: u32,
    /// Minimum total score for Tier A.
    pub tier_a_threshold: u32,
    /// Minimum total score for Tier B (below this is Tier C).
    pub tier_b_threshold: u32,
    /// Email confidence score threshold for the "high confidence" bonus.
    pub email_confidence_threshold: u32,
    /// Minimum employee count for the team-size bonus.
    pub min_team_size: u32,
    /// Minimum annual revenue (in cents) for the revenue bonus.
    pub min_revenue_cents: i64,
}

impl Default for ScoringConfig {
    fn default() -> Self {
        ScoringConfig {
            points_valid_email: 10,
            points_high_email_confidence: 15,
            points_team_size: 15,
            points_revenue: 15,
            points_decision_maker: 20,
            points_target_industry: 15,
            points_intent_signal: 10,
            tier_a_threshold: 80,
            tier_b_threshold: 55,
            email_confidence_threshold: 80,
            min_team_size: 20,
            min_revenue_cents: 1_000_000_00, // $1M in cents
        }
    }
}

// ── Breakdown ────────────────────────────────────────────────────────

/// Detailed per-signal scoring breakdown for a lead.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoringBreakdown {
    pub total: u32,
    pub tier: ScoreTier,
    pub points_email: u32,
    pub points_confidence: u32,
    pub points_team_size: u32,
    pub points_revenue: u32,
    pub points_title: u32,
    pub points_industry: u32,
    pub points_intent: u32,
}

// ── Service ──────────────────────────────────────────────────────────

/// Deterministic lead scoring engine.
pub struct LeadScoringService {
    config: ScoringConfig,
    decision_maker_titles: Vec<String>,
    target_industries: Vec<String>,
}

impl LeadScoringService {
    /// Default decision-maker title keywords (all lowercase).
    const DEFAULT_TITLES: &'static [&'static str] = &[
        "founder",
        "ceo",
        "coo",
        "cto",
        "vp",
        "head",
        "director",
        "partner",
        "owner",
        "president",
        "chief",
    ];

    /// Default target industry keywords (all lowercase).
    const DEFAULT_INDUSTRIES: &'static [&'static str] = &[
        "ai",
        "saas",
        "fintech",
        "technology",
        "software",
        "biotech",
        "cleantech",
        "climate",
        "crypto",
        "blockchain",
        "defi",
    ];

    /// Create a new scoring service with the given config and default
    /// title / industry keyword lists.
    pub fn new(config: ScoringConfig) -> Self {
        LeadScoringService {
            config,
            decision_maker_titles: Self::DEFAULT_TITLES
                .iter()
                .map(|s| s.to_string())
                .collect(),
            target_industries: Self::DEFAULT_INDUSTRIES
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }

    /// Override the decision-maker title keyword list (builder pattern).
    pub fn with_titles(mut self, titles: Vec<String>) -> Self {
        self.decision_maker_titles = titles;
        self
    }

    /// Override the target industry keyword list (builder pattern).
    pub fn with_industries(mut self, industries: Vec<String>) -> Self {
        self.target_industries = industries;
        self
    }

    /// Score a lead and return `(score, tier)`.
    ///
    /// The raw score is capped at 100.
    pub fn score(&self, lead: &Lead) -> (u32, ScoreTier) {
        let breakdown = self.score_breakdown(lead);
        (breakdown.total, breakdown.tier)
    }

    /// Score a lead and return a full [`ScoringBreakdown`].
    pub fn score_breakdown(&self, lead: &Lead) -> ScoringBreakdown {
        let points_email = self.score_email(lead);
        let points_confidence = self.score_email_confidence(lead);
        let points_team_size = self.score_team_size(lead);
        let points_revenue = self.score_revenue(lead);
        let points_title = self.score_decision_maker_title(lead);
        let points_industry = self.score_target_industry(lead);
        let points_intent = self.score_intent_signal(lead);

        let total = points_email
            + points_confidence
            + points_team_size
            + points_revenue
            + points_title
            + points_industry
            + points_intent;

        let total = total.min(100);
        let tier = self.classify_tier(total);

        ScoringBreakdown {
            total,
            tier,
            points_email,
            points_confidence,
            points_team_size,
            points_revenue,
            points_title,
            points_industry,
            points_intent,
        }
    }

    // ── Individual signals ────────────────────────────────────────

    fn score_email(&self, lead: &Lead) -> u32 {
        if !lead.email.trim().is_empty() {
            self.config.points_valid_email
        } else {
            0
        }
    }

    fn score_email_confidence(&self, lead: &Lead) -> u32 {
        let confidence = lead
            .enrichment
            .get("email_confidence")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        if confidence >= self.config.email_confidence_threshold {
            self.config.points_high_email_confidence
        } else {
            0
        }
    }

    fn score_team_size(&self, lead: &Lead) -> u32 {
        let employees = lead
            .enrichment
            .get("employee_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        if employees >= self.config.min_team_size {
            self.config.points_team_size
        } else {
            0
        }
    }

    fn score_revenue(&self, lead: &Lead) -> u32 {
        let revenue = lead
            .enrichment
            .get("annual_revenue")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        if revenue >= self.config.min_revenue_cents {
            self.config.points_revenue
        } else {
            0
        }
    }

    fn score_decision_maker_title(&self, lead: &Lead) -> u32 {
        // Check enrichment.title first, fall back to lead.name
        let title_text = lead
            .enrichment
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let name_text = lead.name.as_deref().unwrap_or("");

        let combined = format!("{} {}", title_text, name_text).to_lowercase();

        if self
            .decision_maker_titles
            .iter()
            .any(|kw| combined.contains(kw.as_str()))
        {
            self.config.points_decision_maker
        } else {
            0
        }
    }

    fn score_target_industry(&self, lead: &Lead) -> u32 {
        let industry_text = lead
            .enrichment
            .get("industry")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let company_text = lead.company.as_deref().unwrap_or("");

        let combined = format!("{} {}", industry_text, company_text).to_lowercase();

        if self
            .target_industries
            .iter()
            .any(|kw| combined.contains(kw.as_str()))
        {
            self.config.points_target_industry
        } else {
            0
        }
    }

    fn score_intent_signal(&self, lead: &Lead) -> u32 {
        let intent = lead.enrichment.get("intent_signal");
        match intent {
            // Boolean true → score
            Some(v) if v.is_boolean() && v.as_bool().unwrap() => self.config.points_intent_signal,
            // Boolean false → no score (explicit, so the !v.is_null() arm doesn't catch it)
            Some(v) if v.is_boolean() => 0,
            // Non-null, non-boolean (e.g. string, number) → score
            Some(v) if !v.is_null() => self.config.points_intent_signal,
            _ => 0,
        }
    }

    fn classify_tier(&self, score: u32) -> ScoreTier {
        if score >= self.config.tier_a_threshold {
            ScoreTier::A
        } else if score >= self.config.tier_b_threshold {
            ScoreTier::B
        } else {
            ScoreTier::C
        }
    }
}

impl Default for LeadScoringService {
    fn default() -> Self {
        Self::new(ScoringConfig::default())
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn svc() -> LeadScoringService {
        LeadScoringService::default()
    }

    fn minimal_lead() -> Lead {
        Lead::new("l-1", "Jane Smith", "jane@example.com", "Acme Corp")
            .with_enrichment(serde_json::Value::Object(serde_json::Map::new()))
    }

    fn lead_with_enrichment(enrichment: serde_json::Value) -> Lead {
        Lead::new("l-1", "Jane Smith", "jane@example.com", "Acme Corp")
            .with_enrichment(enrichment)
    }

    // ── Default config ────────────────────────────────────────────

    #[test]
    fn default_config_values() {
        let cfg = ScoringConfig::default();
        assert_eq!(cfg.points_valid_email, 10);
        assert_eq!(cfg.points_high_email_confidence, 15);
        assert_eq!(cfg.points_team_size, 15);
        assert_eq!(cfg.points_revenue, 15);
        assert_eq!(cfg.points_decision_maker, 20);
        assert_eq!(cfg.points_target_industry, 15);
        assert_eq!(cfg.points_intent_signal, 10);
        assert_eq!(cfg.tier_a_threshold, 80);
        assert_eq!(cfg.tier_b_threshold, 55);
        assert_eq!(cfg.email_confidence_threshold, 80);
        assert_eq!(cfg.min_team_size, 20);
        assert_eq!(cfg.min_revenue_cents, 1_000_000_00);
    }

    // ── Email scoring ─────────────────────────────────────────────

    #[test]
    fn valid_email_scores() {
        let s = svc();
        let b = s.score_breakdown(&minimal_lead());
        assert_eq!(b.points_email, 10);
    }

    #[test]
    fn empty_email_no_score() {
        let s = svc();
        let lead = Lead::new("l-2", "Jane", "", "Acme")
            .with_enrichment(serde_json::json!({}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_email, 0);
    }

    #[test]
    fn whitespace_email_no_score() {
        let s = svc();
        let lead = Lead::new("l-3", "Jane", "   ", "Acme")
            .with_enrichment(serde_json::json!({}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_email, 0);
    }

    // ── Email confidence ──────────────────────────────────────────

    #[test]
    fn high_email_confidence_scores() {
        let s = svc();
        let lead = lead_with_enrichment(serde_json::json!({"email_confidence": 90}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_confidence, 15);
    }

    #[test]
    fn low_email_confidence_no_score() {
        let s = svc();
        let lead = lead_with_enrichment(serde_json::json!({"email_confidence": 50}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_confidence, 0);
    }

    #[test]
    fn exact_confidence_threshold_scores() {
        let s = svc();
        let lead = lead_with_enrichment(serde_json::json!({"email_confidence": 80}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_confidence, 15);
    }

    #[test]
    fn missing_email_confidence_no_score() {
        let s = svc();
        let b = s.score_breakdown(&minimal_lead());
        assert_eq!(b.points_confidence, 0);
    }

    // ── Team size ─────────────────────────────────────────────────

    #[test]
    fn large_team_scores() {
        let s = svc();
        let lead = lead_with_enrichment(serde_json::json!({"employee_count": 50}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_team_size, 15);
    }

    #[test]
    fn small_team_no_score() {
        let s = svc();
        let lead = lead_with_enrichment(serde_json::json!({"employee_count": 10}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_team_size, 0);
    }

    #[test]
    fn exact_team_size_threshold_scores() {
        let s = svc();
        let lead = lead_with_enrichment(serde_json::json!({"employee_count": 20}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_team_size, 15);
    }

    // ── Revenue ───────────────────────────────────────────────────

    #[test]
    fn high_revenue_scores() {
        let s = svc();
        let lead = lead_with_enrichment(serde_json::json!({"annual_revenue": 5_000_000_00}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_revenue, 15);
    }

    #[test]
    fn low_revenue_no_score() {
        let s = svc();
        let lead = lead_with_enrichment(serde_json::json!({"annual_revenue": 500_000_00}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_revenue, 0);
    }

    #[test]
    fn exact_revenue_threshold_scores() {
        let s = svc();
        let lead = lead_with_enrichment(serde_json::json!({"annual_revenue": 1_000_000_00}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_revenue, 15);
    }

    // ── Decision maker title ──────────────────────────────────────

    #[test]
    fn ceo_title_scores() {
        let s = svc();
        let lead = lead_with_enrichment(serde_json::json!({"title": "CEO & Founder"}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_title, 20);
    }

    #[test]
    fn vp_title_scores() {
        let s = svc();
        let lead = lead_with_enrichment(serde_json::json!({"title": "VP of Engineering"}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_title, 20);
    }

    #[test]
    fn director_title_scores() {
        let s = svc();
        let lead = lead_with_enrichment(serde_json::json!({"title": "Director of Sales"}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_title, 20);
    }

    #[test]
    fn no_decision_maker_title_no_score() {
        let s = svc();
        let lead = Lead::new("l-4", "Jane Developer", "jane@example.com", "Acme Corp")
            .with_enrichment(serde_json::json!({}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_title, 0);
    }

    #[test]
    fn case_insensitive_title() {
        let s = svc();
        let lead = lead_with_enrichment(serde_json::json!({"title": "Chief Technology Officer"}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_title, 20);
    }

    #[test]
    fn custom_titles_override() {
        let s = svc().with_titles(vec!["manager".into()]);
        let lead = lead_with_enrichment(serde_json::json!({"title": "Product Manager"}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_title, 20);
    }

    #[test]
    fn decision_maker_in_name() {
        let s = svc();
        let lead = Lead::new("l-5", "Jane CEO-Smith", "jane@example.com", "Acme")
            .with_enrichment(serde_json::json!({}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_title, 20);
    }

    #[test]
    fn name_none_no_title_match() {
        let s = svc();
        let lead = Lead::new("l-6", "", "jane@example.com", "Acme")
            .with_enrichment(serde_json::json!({}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_title, 0);
    }

    // ── Target industry ───────────────────────────────────────────

    #[test]
    fn matching_industry_scores() {
        let s = svc();
        let lead = lead_with_enrichment(serde_json::json!({"industry": "SaaS"}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_industry, 15);
    }

    #[test]
    fn matching_company_industry() {
        let s = svc();
        let lead = Lead::new("l-7", "Jane", "jane@fintech.io", "Fintech Solutions")
            .with_enrichment(serde_json::json!({}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_industry, 15);
    }

    #[test]
    fn no_matching_industry() {
        let s = svc();
        // Use a company name that doesn't contain any target industry substring
        // (e.g. "Retail" contains "ai", so we use "Bakery" instead)
        let lead = Lead::new("l-8", "Jane", "jane@bakery.com", "Bakery Shop")
            .with_enrichment(serde_json::json!({}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_industry, 0);
    }

    #[test]
    fn custom_industries_override() {
        let s = svc().with_industries(vec!["retail".into()]);
        let lead = Lead::new("l-9", "Jane", "jane@retail.com", "Retail Corp")
            .with_enrichment(serde_json::json!({}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_industry, 15);
    }

    #[test]
    fn company_none_no_industry_match_from_company() {
        let s = svc();
        let lead = Lead::new("l-10", "Jane", "jane@retail.com", "")
            .with_enrichment(serde_json::json!({}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_industry, 0);
    }

    // ── Intent signal ─────────────────────────────────────────────

    #[test]
    fn boolean_intent_true_scores() {
        let s = svc();
        let lead = lead_with_enrichment(serde_json::json!({"intent_signal": true}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_intent, 10);
    }

    #[test]
    fn boolean_intent_false_no_score() {
        let s = svc();
        let lead = lead_with_enrichment(serde_json::json!({"intent_signal": false}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_intent, 0);
    }

    #[test]
    fn string_intent_scores() {
        let s = svc();
        let lead = lead_with_enrichment(serde_json::json!({"intent_signal": "pricing_page"}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_intent, 10);
    }

    #[test]
    fn null_intent_no_score() {
        let s = svc();
        let lead = lead_with_enrichment(serde_json::json!({"intent_signal": null}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_intent, 0);
    }

    #[test]
    fn missing_intent_no_score() {
        let s = svc();
        let b = s.score_breakdown(&minimal_lead());
        assert_eq!(b.points_intent, 0);
    }

    // ── Tier classification ───────────────────────────────────────

    #[test]
    fn tier_a_classification() {
        let s = svc();
        // Max possible with default config: 10+15+15+15+20+15+10 = 100
        let lead = lead_with_enrichment(serde_json::json!({
            "email_confidence": 95,
            "employee_count": 100,
            "annual_revenue": 10_000_000_00,
            "title": "CEO",
            "industry": "AI",
            "intent_signal": true
        }));
        let (score, tier) = s.score(&lead);
        assert_eq!(score, 100);
        assert_eq!(tier, ScoreTier::A);
    }

    #[test]
    fn tier_b_classification() {
        let s = svc();
        let lead = lead_with_enrichment(serde_json::json!({
            "email_confidence": 90,
            "employee_count": 50,
            "title": "Director"
        }));
        let (score, tier) = s.score(&lead);
        assert!(score >= 55 && score < 80);
        assert_eq!(tier, ScoreTier::B);
    }

    #[test]
    fn tier_c_classification() {
        let s = svc();
        let b = s.score_breakdown(&minimal_lead());
        assert_eq!(b.tier, ScoreTier::C);
    }

    #[test]
    fn exact_tier_a_threshold() {
        let s = svc();
        assert_eq!(s.classify_tier(80), ScoreTier::A);
    }

    #[test]
    fn exact_tier_b_threshold() {
        let s = svc();
        assert_eq!(s.classify_tier(55), ScoreTier::B);
        assert_eq!(s.classify_tier(54), ScoreTier::C);
    }

    // ── Score cap ─────────────────────────────────────────────────

    #[test]
    fn score_capped_at_100() {
        // Custom config where max possible > 100
        let cfg = ScoringConfig {
            points_valid_email: 30,
            points_high_email_confidence: 30,
            points_team_size: 30,
            points_revenue: 30,
            points_decision_maker: 30,
            points_target_industry: 30,
            points_intent_signal: 30,
            tier_a_threshold: 80,
            tier_b_threshold: 55,
            email_confidence_threshold: 80,
            min_team_size: 20,
            min_revenue_cents: 1_000_000_00,
        };
        let s = LeadScoringService::new(cfg);
        let lead = lead_with_enrichment(serde_json::json!({
            "email_confidence": 90,
            "employee_count": 100,
            "annual_revenue": 5_000_000_00,
            "title": "CEO",
            "industry": "SaaS",
            "intent_signal": true
        }));
        let (score, _) = s.score(&lead);
        assert_eq!(score, 100);
    }

    // ── score() vs score_breakdown() consistency ──────────────────

    #[test]
    fn score_matches_breakdown() {
        let s = svc();
        let lead = lead_with_enrichment(serde_json::json!({
            "email_confidence": 85,
            "employee_count": 30,
            "title": "CTO",
            "industry": "fintech",
            "intent_signal": true
        }));
        let (score, tier) = s.score(&lead);
        let b = s.score_breakdown(&lead);
        assert_eq!(score, b.total);
        assert_eq!(tier, b.tier);
    }

    // ── Builder pattern ───────────────────────────────────────────

    #[test]
    fn builder_chain() {
        let s = LeadScoringService::default()
            .with_titles(vec!["executive".into()])
            .with_industries(vec!["manufacturing".into()]);
        let lead = Lead::new("l-11", "Exec Jane", "jane@manu.com", "Manufacturing Co")
            .with_enrichment(serde_json::json!({"title": "Executive VP"}));
        let b = s.score_breakdown(&lead);
        assert_eq!(b.points_title, 20);
        assert_eq!(b.points_industry, 15);
    }
}
