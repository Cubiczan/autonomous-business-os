from functools import lru_cache
from pathlib import Path

from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    app_name: str = "Autonomous Business Operating System"
    environment: str = "development"
    app_base_url: str = "http://localhost:8000"
    database_url: str = "sqlite:///./storage/business_os.sqlite3"
    redis_url: str = "redis://localhost:6379/0"
    # SECURITY WARNING: secret_key and admin_api_key MUST be overridden with
    # cryptographically strong random values before any non-local deployment.
    # Generate with: python -c "import secrets; print(secrets.token_urlsafe(48))"
    secret_key: str = Field(default="changeme-insecure-default-do-not-use-in-prod", repr=False)
    admin_api_key: str = Field(default="changeme-insecure-default-do-not-use-in-prod", repr=False)
    log_level: str = "INFO"

    slack_signing_secret: str | None = Field(default=None, repr=False)
    stripe_webhook_secret: str | None = Field(default=None, repr=False)
    docusign_webhook_secret: str | None = Field(default=None, repr=False)

    apollo_api_key: str | None = Field(default=None, repr=False)
    hunter_api_key: str | None = Field(default=None, repr=False)
    hubspot_access_token: str | None = Field(default=None, repr=False)
    salesforce_access_token: str | None = Field(default=None, repr=False)
    crm_provider: str = "hubspot"

    smtp_host: str | None = None
    smtp_port: int = 587
    smtp_username: str | None = None
    smtp_password: str | None = Field(default=None, repr=False)
    smtp_from: str | None = None
    gmail_client_id: str | None = Field(default=None, repr=False)
    gmail_client_secret: str | None = Field(default=None, repr=False)
    outlook_client_id: str | None = Field(default=None, repr=False)
    outlook_client_secret: str | None = Field(default=None, repr=False)

    docusign_access_token: str | None = Field(default=None, repr=False)
    notion_token: str | None = Field(default=None, repr=False)
    notion_database_id: str | None = None
    linear_api_key: str | None = Field(default=None, repr=False)
    jira_base_url: str | None = None
    jira_email: str | None = None
    jira_api_token: str | None = Field(default=None, repr=False)
    slack_bot_token: str | None = Field(default=None, repr=False)
    calendar_provider: str = "google"
    google_calendar_credentials_json: str | None = Field(default=None, repr=False)
    microsoft_graph_token: str | None = Field(default=None, repr=False)

    stripe_api_key: str | None = Field(default=None, repr=False)
    accounting_provider: str = "quickbooks"

    airbyte_client_id: str | None = Field(default=None, repr=False)
    airbyte_client_secret: str | None = Field(default=None, repr=False)
    airbyte_workspace_name: str = "default"
    airbyte_organization_id: str | None = None

    linear_team_id: str | None = None
    jira_project_key: str | None = None
    jira_issue_type: str = "Task"
    quickbooks_access_token: str | None = Field(default=None, repr=False)
    xero_access_token: str | None = Field(default=None, repr=False)

    sentry_dsn: str | None = Field(default=None, repr=False)
    otel_exporter_otlp_endpoint: str | None = None

    llama_cloud_api_key: str | None = Field(default=None, repr=False)
    llama_cloud_knowledge_pipeline_id: str | None = None
    llama_cloud_dense_top_k: int = 12
    llama_cloud_sparse_top_k: int = 12
    llama_cloud_rerank_top_n: int = 5

    model_config = SettingsConfigDict(env_file=".env", env_file_encoding="utf-8", extra="ignore")

    @property
    def storage_dir(self) -> Path:
        return Path("storage")


@lru_cache
def get_settings() -> Settings:
    return Settings()
