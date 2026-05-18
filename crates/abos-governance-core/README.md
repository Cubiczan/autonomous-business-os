# ABOS Governance Core

`abos-governance-core` is the native Rust sidecar for Autonomous Business OS governance
decisions. It keeps high-impact policy logic deterministic and testable while the Python FastAPI
application remains the integration and dashboard surface.

The crate covers:

- approval classification for outbound, financial, legal, destructive, credential, and production actions,
- prompt-injection marker inspection for untrusted content,
- rate-limit and circuit-breaker state transitions,
- canonical audit-event hashing,
- HMAC signed ledger entries for release and compliance trails.

Signing material is provided at runtime by the caller, normally from `ABOS_LEDGER_SIGNING_KEY`.
No credentials are stored in the crate.
