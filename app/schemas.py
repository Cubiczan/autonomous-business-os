from typing import Any, Literal

from pydantic import BaseModel, EmailStr, Field


class LeadIngestRequest(BaseModel):
    source: str = "api"
    email: EmailStr
    name: str | None = None
    company: str | None = None
    title: str | None = None
    website: str | None = None
    metadata: dict[str, Any] = Field(default_factory=dict)


class ContractSignedRequest(BaseModel):
    client_name: str
    client_email: EmailStr | None = None
    contract_id: str
    project_type: str = "implementation"
    start_date: str | None = None
    metadata: dict[str, Any] = Field(default_factory=dict)


class DeliveryStatusRequest(BaseModel):
    project_id: str
    client_name: str
    milestone: str
    completion_pct: float = Field(ge=0, le=100)
    budget_used_pct: float = Field(ge=0, le=200)
    days_since_client_contact: int = Field(ge=0)
    metadata: dict[str, Any] = Field(default_factory=dict)


class InvoiceRequest(BaseModel):
    customer_id: str
    customer_email: EmailStr | None = None
    amount_cents: int = Field(gt=0)
    currency: str = "usd"
    description: str
    due_in_days: int = Field(default=14, ge=0)
    metadata: dict[str, Any] = Field(default_factory=dict)


class KnowledgeQueryRequest(BaseModel):
    question: str
    channel_id: str | None = None
    requester: str | None = None
    metadata: dict[str, Any] = Field(default_factory=dict)


class WorkflowCreateRequest(BaseModel):
    kind: Literal[
        "lead_qualification",
        "client_onboarding",
        "delivery_monitoring",
        "finance_operations",
        "knowledge_communication",
    ]
    title: str
    source: str = "api"
    payload: dict[str, Any] = Field(default_factory=dict)


class ApprovalDecisionRequest(BaseModel):
    status: Literal["approved", "rejected"]
    decided_by: str
    decision_note: str | None = None
