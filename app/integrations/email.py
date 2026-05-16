import smtplib
from email.message import EmailMessage

from app.integrations.base import IntegrationResult


class EmailClient:
    provider = "smtp"

    def __init__(
        self,
        host: str | None,
        port: int,
        username: str | None,
        password: str | None,
        from_address: str | None,
    ):
        self.host = host
        self.port = port
        self.username = username
        self.password = password
        self.from_address = from_address

    @property
    def configured(self) -> bool:
        return bool(self.host and self.from_address)

    def draft_outreach(self, recipient: str, subject: str, body: str) -> IntegrationResult:
        return IntegrationResult(
            ok=True,
            provider=self.provider,
            action="draft_outreach",
            data={"to": recipient, "subject": subject, "body": body},
            simulated=True,
        )

    def send(self, recipient: str, subject: str, body: str) -> IntegrationResult:
        if not self.configured:
            return self.draft_outreach(recipient, subject, body)

        message = EmailMessage()
        message["From"] = self.from_address
        message["To"] = recipient
        message["Subject"] = subject
        message.set_content(body)

        with smtplib.SMTP(self.host, self.port, timeout=20) as smtp:
            smtp.starttls()
            if self.username and self.password:
                smtp.login(self.username, self.password)
            smtp.send_message(message)

        return IntegrationResult(
            ok=True,
            provider=self.provider,
            action="send",
            data={"to": recipient, "subject": subject},
        )
