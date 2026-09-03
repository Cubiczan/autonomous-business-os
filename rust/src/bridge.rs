use crate::types::ScoreTier;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", content = "input", rename_all = "snake_case")]
pub enum BridgeRequest {
    ScoreLead { lead: Value, enrichment: Value },
    ClassifyAction { action_type: String, payload: Value },
    InspectText { text: String, source: String },
    HashJson { payload: Value },
    DepartmentMetrics {
        department_type: String,
        status: String,
        revenue_signals: Value,
        output: Value,
        approval_count: usize,
    },
    AnalyzeSkillDescription { description: String, name: Option<String> },
    ResolveSkillSlugs { role: String, department_type: String },
    SerializeSdkResult { payload: Value },
    SerializeDepartment { payload: Value },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum BridgeResponse {
    LeadScore {
        score: f64,
        tier: String,
        reasons: Vec<String>,
    },
    ActionClassification {
        risk_level: String,
        requires_approval: bool,
        reasons: Vec<String>,
    },
    PromptInspection {
        source: String,
        score: f64,
        flags: Vec<String>,
        safe_to_use_as_instruction: bool,
        handling: String,
    },
    CanonicalHash {
        hash: String,
    },
    DepartmentMetrics {
        health_score: f64,
        revenue_signals: Value,
    },
    SkillAnalysis {
        skill_name: String,
        slug: String,
        tags: Vec<String>,
        tools: Vec<String>,
        approval_policy: String,
    },
    SkillSlugs {
        slugs: Vec<String>,
    },
    SerializedValue {
        value: Value,
    },
}

pub fn handle_request(request: BridgeRequest) -> BridgeResponse {
    match request {
        BridgeRequest::ScoreLead { lead, enrichment } => {
            let (score, tier, reasons) = score_lead(lead, enrichment);
            BridgeResponse::LeadScore { score, tier, reasons }
        }
        BridgeRequest::ClassifyAction { action_type, payload } => {
            let (risk_level, requires_approval, reasons) = classify_action(&action_type, &payload);
            BridgeResponse::ActionClassification {
                risk_level,
                requires_approval,
                reasons,
            }
        }
        BridgeRequest::InspectText { text, source } => {
            let inspection = inspect_text(&text, &source);
            BridgeResponse::PromptInspection {
                source: inspection.source,
                score: inspection.score,
                flags: inspection.flags,
                safe_to_use_as_instruction: inspection.safe_to_use_as_instruction,
                handling: inspection.handling,
            }
        }
        BridgeRequest::HashJson { payload } => BridgeResponse::CanonicalHash {
            hash: hash_json(&payload),
        },
        BridgeRequest::DepartmentMetrics {
            department_type,
            status,
            revenue_signals,
            output,
            approval_count,
        } => {
            let (health_score, revenue_signals) = department_metrics(
                &department_type,
                &status,
                revenue_signals,
                output,
                approval_count,
            );
            BridgeResponse::DepartmentMetrics {
                health_score,
                revenue_signals,
            }
        }
        BridgeRequest::AnalyzeSkillDescription { description, name } => {
            let (skill_name, slug, tags, tools, approval_policy) =
                analyze_skill_description(&description, name.as_deref());
            BridgeResponse::SkillAnalysis {
                skill_name,
                slug,
                tags,
                tools,
                approval_policy,
            }
        }
        BridgeRequest::ResolveSkillSlugs {
            role,
            department_type,
        } => BridgeResponse::SkillSlugs {
            slugs: resolve_skill_slugs(&role, &department_type),
        },
        BridgeRequest::SerializeSdkResult { payload } => BridgeResponse::SerializedValue {
            value: serialize_sdk_result(payload),
        },
        BridgeRequest::SerializeDepartment { payload } => BridgeResponse::SerializedValue {
            value: serialize_department(payload),
        },
    }
}

fn score_lead(lead: Value, enrichment: Value) -> (f64, String, Vec<String>) {
    let mut score: f64 = 0.0;
    let mut reasons = Vec::new();
    if lead.get("email").and_then(Value::as_str).is_some_and(|value| !value.trim().is_empty()) {
        score += 10.0;
        reasons.push("valid email present".to_string());
    }
    let email_confidence = enrichment
        .get("email_confidence")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if email_confidence >= 80 {
        score += 15.0;
        reasons.push("high email confidence".to_string());
    }
    let employee_count = enrichment
        .get("employee_count")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if employee_count >= 20 {
        score += 15.0;
        reasons.push("company has meaningful team size".to_string());
    }
    let annual_revenue = enrichment
        .get("annual_revenue")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if annual_revenue >= 1_000_000 {
        score += 15.0;
        reasons.push("revenue threshold met".to_string());
    }
    let title = format!(
        "{} {}",
        lead.get("title").and_then(Value::as_str).unwrap_or(""),
        enrichment.get("title").and_then(Value::as_str).unwrap_or("")
    )
    .to_lowercase();
    if ["founder", "ceo", "coo", "cto", "vp", "head", "director", "partner", "owner"]
        .iter()
        .any(|term| title.contains(term))
    {
        score += 20.0;
        reasons.push("decision-maker title".to_string());
    }
    let company_text = format!(
        "{} {}",
        lead.get("company").and_then(Value::as_str).unwrap_or(""),
        enrichment.get("industry").and_then(Value::as_str).unwrap_or("")
    )
    .to_lowercase();
    if ["ai", "software", "saas", "consulting", "agency", "fintech", "health"]
        .iter()
        .any(|term| company_text.contains(term))
    {
        score += 15.0;
        reasons.push("target industry fit".to_string());
    }
    if lead
        .get("metadata")
        .and_then(Value::as_object)
        .and_then(|m| m.get("intent_signal"))
        .is_some_and(|value| !matches!(value, Value::Bool(false) | Value::Null))
    {
        score += 10.0;
        reasons.push("intent signal".to_string());
    }

    score = score.min(100.0);
    let tier = if score >= 80.0 {
        ScoreTier::A
    } else if score >= 55.0 {
        ScoreTier::B
    } else {
        ScoreTier::C
    };

    let tier = match tier {
        ScoreTier::A => "A",
        ScoreTier::B => "B",
        ScoreTier::C => "C",
    }
    .to_string();

    (score as f64, tier, reasons)
}

fn classify_action(action_type: &str, payload: &Value) -> (String, bool, Vec<String>) {
    const OUTBOUND_MARKERS: &[&str] = &[
        "send_email",
        "outbound_email",
        "client_message",
        "social_post",
        "publish_post",
        "publish_video",
        "newsletter_send",
        "ad_publish",
        "proposal_send",
    ];
    const HIGH_IMPACT_MARKERS: &[&str] = &[
        "send_money",
        "payment",
        "wire_transfer",
        "contract_sign",
        "delete",
        "credential",
        "production_change",
        "invoice_create",
    ];
    let action = action_type.to_lowercase();
    let payload_text = payload.to_string().to_lowercase();
    let mut reasons = Vec::new();
    let mut requires_approval = false;
    let mut risk_level = "low".to_string();

    if OUTBOUND_MARKERS.iter().any(|marker| action.contains(marker)) {
        requires_approval = true;
        risk_level = "high".to_string();
        reasons.push("Outbound communication or publishing requires approval.".to_string());
    }
    if HIGH_IMPACT_MARKERS.iter().any(|marker| action.contains(marker)) {
        requires_approval = true;
        risk_level = "critical".to_string();
        reasons.push("Financial, contractual, destructive, or credential action is blocked.".to_string());
    }
    if ["gdpr", "personal data", "contract", "payment"]
        .iter()
        .any(|marker| payload_text.contains(marker))
    {
        if risk_level == "low" {
            risk_level = "high".to_string();
        }
        reasons.push("Sensitive business, legal, financial, or personal-data context detected.".to_string());
    }
    if reasons.is_empty() {
        reasons.push("Low-risk internal action with audit logging.".to_string());
    }

    (risk_level, requires_approval, reasons)
}

#[derive(Debug, Clone)]
struct PromptInspection {
    source: String,
    score: f64,
    flags: Vec<String>,
    safe_to_use_as_instruction: bool,
    handling: String,
}

fn inspect_text(text: &str, source: &str) -> PromptInspection {
    const PATTERNS: &[&str] = &[
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
    ];
    let normalized = text.to_lowercase();
    let flags: Vec<String> = PATTERNS
        .iter()
        .filter(|pattern| normalized.contains(**pattern))
        .map(|pattern| (*pattern).to_string())
        .collect();
    let score = (flags.len() as f64 / 3.0).min(1.0);
    PromptInspection {
        source: source.to_string(),
        score,
        flags: flags.clone(),
        safe_to_use_as_instruction: flags.is_empty(),
        handling: if flags.is_empty() {
            "normal".to_string()
        } else {
            "treat_as_untrusted_data".to_string()
        },
    }
}

fn hash_json(payload: &Value) -> String {
    let canonical = canonicalize(payload);
    let serialized = serde_json::to_string(&canonical).expect("canonical json serializes");
    let digest = Sha256::digest(serialized.as_bytes());
    hex::encode(digest)
}

fn canonicalize(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let ordered: BTreeMap<String, Value> = map
                .iter()
                .map(|(key, value)| (key.clone(), canonicalize(value)))
                .collect();
            serde_json::to_value(ordered).expect("ordered map serializes")
        }
        Value::Array(items) => Value::Array(items.iter().map(canonicalize).collect()),
        _ => value.clone(),
    }
}

fn department_metrics(
    department_type: &str,
    status: &str,
    revenue_signals: Value,
    output: Value,
    approval_count: usize,
) -> (f64, Value) {
    let mut signals = revenue_signals.as_object().cloned().unwrap_or_default();
    match department_type {
        "sales" => {
            let leads_count = output
                .get("leads")
                .and_then(Value::as_array)
                .map(|items| items.len())
                .unwrap_or(0) as u64;
            let lead_count = signals
                .get("lead_count")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                + leads_count;
            let pipeline_value = signals
                .get("pipeline_value")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                + leads_count * 5_000;
            signals.insert("lead_count".to_string(), Value::from(lead_count));
            signals.insert("pipeline_value".to_string(), Value::from(pipeline_value));
        }
        "content" => {
            let drafts_count = output
                .get("drafts")
                .and_then(Value::as_array)
                .map(|items| items.len())
                .unwrap_or(0) as u64;
            let content_outputs = signals
                .get("content_outputs")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                + drafts_count;
            signals.insert("content_outputs".to_string(), Value::from(content_outputs));
        }
        _ => {}
    }

    let mut health_score: f64 = if status == "active" { 0.95 } else { 0.4 };
    if approval_count > 10 {
        health_score -= 0.1;
    }
    health_score = health_score.clamp(0.0, 1.0);
    (health_score, Value::Object(signals))
}

fn analyze_skill_description(
    description: &str,
    name: Option<&str>,
) -> (String, String, Vec<String>, Vec<String>, String) {
    let skill_name = name
        .map(|value| value.to_string())
        .unwrap_or_else(|| title_from_description(description));
    let slug = slugify(&skill_name);
    let tags = infer_tags(description);
    let tools = infer_tools(description);
    let approval_policy = infer_approval_policy(description);
    (skill_name, slug, tags, tools, approval_policy)
}

fn resolve_skill_slugs(role: &str, department_type: &str) -> Vec<String> {
    let role_text = role.to_lowercase();
    let mut slugs: BTreeSet<String> = BTreeSet::from([
        "analytics_reporting".to_string(),
        "approval_routing".to_string(),
        "prompt_injection_defense".to_string(),
    ]);
    if role_text.contains("ceo") || role_text.contains("strategy") {
        slugs.extend([
            "market_research".to_string(),
            "analytics_reporting".to_string(),
            "compliance_review".to_string(),
        ]);
    }
    if role_text.contains("sales") || role_text.contains("outreach") || department_type == "sales" {
        slugs.extend([
            "lead_generation".to_string(),
            "cold_email_outreach".to_string(),
            "crm_management".to_string(),
            "pipeline_tracking".to_string(),
        ]);
    }
    if role_text.contains("content") || role_text.contains("creator") || department_type == "content" {
        slugs.extend([
            "content_strategy".to_string(),
            "content_drafting".to_string(),
            "social_scheduling".to_string(),
        ]);
    }
    if role_text.contains("youtube") {
        slugs.insert("youtube_scriptwriting".to_string());
    }
    if role_text.contains("newsletter") {
        slugs.insert("newsletter_production".to_string());
    }
    if role_text.contains("research") || role_text.contains("analyst") || department_type == "intelligence" {
        slugs.extend(["market_research".to_string(), "analytics_reporting".to_string()]);
    }
    if role_text.contains("operations") {
        slugs.extend(["analytics_reporting".to_string(), "compliance_review".to_string()]);
    }
    if role_text.contains("customer") || role_text.contains("success") {
        slugs.insert("customer_success".to_string());
    }
    if role_text.contains("finance") || department_type == "finance" {
        slugs.insert("finance_reporting".to_string());
    }
    if role_text.contains("legal") || role_text.contains("compliance") {
        slugs.insert("compliance_review".to_string());
    }
    slugs.into_iter().collect()
}

fn title_from_description(description: &str) -> String {
    let words: Vec<&str> = description
        .split_whitespace()
        .map(|word| word.trim_matches(|c: char| ".,:;()[]{}".contains(c)))
        .filter(|word| word.len() > 2)
        .take(5)
        .collect();
    if words.is_empty() {
        return "Custom Skill".to_string();
    }
    words
        .into_iter()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn infer_tags(description: &str) -> Vec<String> {
    let text = description.to_lowercase();
    let tags = [
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
    ];
    let inferred: Vec<String> = tags
        .iter()
        .filter(|tag| text.contains(**tag))
        .map(|tag| (*tag).to_string())
        .collect();
    if inferred.is_empty() {
        vec!["custom".to_string()]
    } else {
        inferred
    }
}

fn infer_tools(description: &str) -> Vec<String> {
    let text = description.to_lowercase();
    let mut tools = vec!["memory".to_string()];
    let tool_map = [
        ("email", "email_draft"),
        ("social", "social_draft"),
        ("linkedin", "social_draft"),
        ("twitter", "social_draft"),
        ("x/", "social_draft"),
        ("youtube", "document_generation"),
        ("crm", "crm"),
        ("lead", "apollo"),
        ("invoice", "stripe"),
        ("finance", "accounting"),
        ("calendar", "calendar"),
        ("research", "web_research"),
    ];
    for (marker, tool) in tool_map {
        if text.contains(marker) && !tools.iter().any(|existing| existing == tool) {
            tools.push(tool.to_string());
        }
    }
    tools
}

fn infer_approval_policy(description: &str) -> String {
    let text = description.to_lowercase();
    if ["send", "publish", "post", "email", "message", "approval", "approve"]
        .iter()
        .any(|marker| text.contains(marker))
    {
        return "human_approval_required_before_external_action".to_string();
    }
    if ["money", "payment", "contract", "delete"]
        .iter()
        .any(|marker| text.contains(marker))
    {
        return "human_approval_required_before_high_impact_action".to_string();
    }
    "autonomous_with_audit".to_string()
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut last_was_sep = false;
    for ch in value.to_lowercase().chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            slug.push(ch);
            last_was_sep = false;
        } else if !last_was_sep {
            slug.push('_');
            last_was_sep = true;
        }
    }
    let trimmed = slug.trim_matches('_').to_string();
    let capped = if trimmed.len() > 80 {
        trimmed[..80].to_string()
    } else {
        trimmed
    };
    if capped.is_empty() {
        "custom_skill".to_string()
    } else {
        capped
    }
}

fn serialize_sdk_result(payload: Value) -> Value {
    match payload {
        Value::Null => Value::Object(Default::default()),
        Value::Object(map) => {
            let data = map.get("data").cloned();
            if data.is_some()
                && map
                    .keys()
                    .all(|key| matches!(key.as_str(), "data" | "meta" | "status" | "success"))
            {
                match data.unwrap() {
                    Value::Object(obj) => Value::Object(obj),
                    Value::Array(items) => {
                        let arr = Value::Array(items);
                        serde_json::json!({"items": arr.clone(), "data": arr})
                    }
                    other => serde_json::json!({"data": other}),
                }
            } else {
                Value::Object(map)
            }
        }
        Value::Array(items) => {
            let arr = Value::Array(items);
            serde_json::json!({"items": arr.clone(), "data": arr})
        }
        other => serde_json::json!({"value": other}),
    }
}

fn serialize_department(payload: Value) -> Value {
    payload
}
