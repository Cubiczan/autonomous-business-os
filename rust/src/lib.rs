//! # Autonomous Business OS
//!
//! A modular Rust crate for autonomous business operations including:
//!
//! - **Workflow engine** with enforced state transitions and retry semantics
//! - **Human-in-the-loop approval chains** for governance
//! - **Lead scoring engine** with deterministic, configurable signal evaluation
//! - **Rate limiting** with token-bucket algorithm and circuit breakers
//! - **Agent orchestration** with exponential backoff retry and escalation
//! - **Security** with HMAC signature verification and replay protection
//! - **Audit trail** with append-only immutable event log
//!
//! ## Module layout
//!
//! | Module | Description |
//! |--------|-------------|
//! | `types` | Core domain types (Workflow, Task, Lead, etc.) |
//! | `state_machine` | Transition guards and state machines |
//! | `audit` | Append-only audit trail |
//! | `approval` | Human-in-the-loop approval service |
//! | `scoring` | Lead scoring engine |
//! | `security` | HMAC verification and replay protection |
//! | `rate_limit` | Token-bucket rate limiter and circuit breakers |
//! | `orchestrator` | Agent dispatch with retry and escalation |
//! | `pipeline` | Full BusinessOS façade tying all subsystems together |

pub mod approval;
pub mod audit;
pub mod orchestrator;
pub mod pipeline;
pub mod rate_limit;
pub mod scoring;
pub mod security;
pub mod state_machine;
pub mod types;

pub use pipeline::BusinessOS;
