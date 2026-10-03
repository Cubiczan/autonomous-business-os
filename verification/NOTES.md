# Workflow.lean — source mapping and review notes

Model of the workflow / task / approval state machines in
`autonomous-business-os`, in `Workflow.lean` (same directory).
Compiles clean with `~/.elan/bin/lean Workflow.lean` (Lean 4.34.1, core
only); `#print axioms` on the headline theorems shows only `propext`.
The model is the **Rust enforced machine** (`Exec = Step ∪ Cancel`), as
the Python implementation has no transition relation to model (F2).

## What was modelled

| Machine | States | Relation in the model | Source |
|---|---|---|---|
| Workflow | `pending, running, waitingForHuman, completed, failed, cancelled` | `stepB` = `can_transition`; `cancelB` = `mark_cancelled` edges; `Exec = stepB ‖ cancelB` | `rust/src/state_machine.rs` ll. 41–58, 112–128 |
| Workflow (declarative copy) | same | `stepTB` = `valid_transitions` — modelled only for the divergence theorems | `rust/src/types.rs` ll. 26–45 |
| Task | `queued, running, completed, failed, escalated` | `taskStepB` | `rust/src/state_machine.rs` ll. 159–171 |
| Approval | `open, approved, rejected` (+ events `approve/reject`) | `apprB` | `rust/src/state_machine.rs` ll. 190–196, 203–230 |

Initial states: workflow `pending` (`Workflow::new`, `types.rs` l. 194),
task `queued`, approval `open` — identical in Python
(`app/models.py` ll. 20–40, 119–121). Abstracted away: timestamps, result
payloads, and the attempts counter (analysed in F4 instead).

## Theorem → source mapping

| Theorem(s) | Property | Source |
|---|---|---|
| `Reach.head_decomp` | No state skipping: every non-trivial trace starts with one legal step; reachability exists only via the relation | generic; instantiated for all three machines |
| `Reach.absorbing` | A state with no outgoing steps traps every trace | generic |
| `diverge_failed_successor` | `types.rs`: `failed → running`; enforced: ¬ `failed → running` | `types.rs` l. 44 vs `state_machine.rs` ll. 52, 131–140 |
| `diverge_failed_cancel` | Enforced: `failed → cancelled` ✓; `types.rs`: ✗ | `state_machine.rs` ll. 112–128 vs `types.rs` l. 44 |
| `diverge_pending_cancel` | `can_transition`: ¬ `pending → cancelled`; the step exists only via `mark_cancelled` ∈ `Exec` | `state_machine.rs` ll. 44, 112–128 vs `types.rs` l. 28 |
| `waiting_in_gate` | `waitingForHuman` is enterable only from `running` | ll. 41–58 + 112–128 |
| `completed_in_char`, `pending_in_char`, `running_in_char`, `pending_out_char`, `failed_out_char` | Exact in/out neighbourhoods of each state | ll. 41–58 + 112–128 |
| `waiting_out_char` | Exits from `waitingForHuman` are exactly `{running, completed, failed, cancelled}` — all ordinary steps, **no approval event** | ll. 41–58 + 112–128 (see F3) |
| `not_exec_completed`, `not_exec_cancelled`, `completed_absorbing`, `cancelled_absorbing` | Terminal workflow states are absorbing | ll. 56–57 (`Completed => false`, `Cancelled => false`) |
| `pending_cases`, `running_unavoidable_completed`, `running_unavoidable_waiting` | Nothing is completed (or gated) without first executing: every trace from `pending` to `completed`/`waitingForHuman` passes through `running` | initial state `types.rs` l. 194 + full table |
| `task_escalated_in`, `task_running_in`, `task_completed_in` | Task neighbourhoods: escalation/completion only from `running`; `running` only from `queued`/`escalated` | ll. 159–171 |
| `not_taskstep_completed`, `not_taskstep_failed`, `task_completed_absorbing`, `task_failed_absorbing` | Task `completed` **and `failed`** are absorbing — no task retry | ll. 168–169 |
| `approved_gate`, `rejected_gate` | **Approval gate**: a decided state is enterable only via the matching decision event, only from `open` | ll. 190–196, 203–230 |
| `apprU_into_approved`, `apprU_into_rejected` | Same, for unlabelled steps | ll. 190–196 |
| `not_apprU_from_approved`, `not_apprU_from_rejected`, `approved_absorbing`, `rejected_absorbing`, `approval_irreversible`, `approval_irreversible'` | Decisions are irreversible (`AlreadyDecided`) | ll. 198–201, 203–230; `rust/src/approval.rs` ll. 113–137 |

## Findings

### F1 — HEADLINE: Rust disagrees with itself about the workflow machine
`types.rs::valid_transitions` (ll. 26–45) and
`state_machine.rs::can_transition` (ll. 41–58) define **different
machines**, and the divergence theorems above are the machine-checked
proof. (a) A failed workflow's successor is `running` in `types.rs`
(direct re-run, skipping `pending`) but `pending` in the enforced
machine (the retry path). (b) `types.rs` forbids `failed → cancelled`;
`mark_cancelled` allows it. (c) `types.rs` lists `pending → cancelled`
as an ordinary transition, but `can_transition` has no `→ cancelled`
edges at all, so the generic `transition()` rejects every cancellation.
`valid_transitions`/`can_transition_to` have **zero call sites** in
`rust/src` or `crates` (grep-verified) — dead code today, but it is the
table a future reader (or a second implementation) will copy. Delete it
or generate it from `can_transition`.

### F2 — HEADLINE: Rust vs Python is enforced vs unenforced, not two dialects
The state *sets* agree exactly (`app/models.py` ll. 20–40), but Python's
`WorkflowService` performs **no validation whatsoever**: `mark_running`,
`mark_completed`, `mark_failed` assign the status unconditionally
(`app/services/workflows.py` ll. 34–66). Every ordered pair of states is
a legal Python "transition", including `completed → running`
(resurrecting a finished workflow — while still incrementing `attempts`)
and `pending → completed` (the exact skip the Rust machine provably
forbids). Callers also bypass the service with direct assignments
(`app/services/guardrails.py` l. 72, `app/agents/finance_operations.py`
l. 77, `app/agents/lead_qualification.py` l. 101 →
`waiting_for_human`; `app/agents/orchestrator.py` l. 49 → `pending`).
Python has no cancel / retry / waiting service methods at all. Any
property proved about the Rust machine (this file) says nothing about
the Python runtime.

### F3 — HEADLINE: the workflow human gate is not approval-gated
The gate property **is** proved — but only for the approval machine
itself (`approved_gate`: `approved` is reachable solely via an `approve`
event from `open`, irreversibly). For workflows, `waiting_out_char`
shows the exits from `waitingForHuman` are ordinary
`transition()`/`mark_cancelled` steps: **nothing in the workflow machine
consults an approval**. Worse, nothing in the Rust runtime uses the
gate at all: `mark_waiting_for_human` and `WaitingForHuman` have no
references outside `state_machine.rs`/`types.rs`, and
`rust/src/orchestrator.rs`'s run loop (ll. 281–304: `mark_running` →
`mark_completed`/`mark_failed` → `retry`) never enters
`waitingForHuman` and never touches `ApprovalService`. As shipped, the
human-in-the-loop workflow state is unreachable in Rust and ungated on
exit; the approval chain is a standalone service (API layer only).

### F4 — The max-attempts guard is bypassable through the public primitive
`retry()` checks `attempts >= max_attempts` (ll. 131–140), but the guard
is a property of that convenience method, not of the machine:
`can_transition(failed, pending)` is unconditionally true, and the
public `transition()` (ll. 68–76) consults only `can_transition` — so
`transition(wf, Pending)` retries an exhausted workflow, and
`transition(wf, Running)` from `pending` starts a run **without**
incrementing `attempts` (only `mark_running`, ll. 81–85, increments).
The module doc's "(retry, only if attempts < max_attempts)" (l. 30) is
therefore not enforced at the state-machine layer. Current in-crate
callers use the convenience methods, so this is a latent API hazard:
make `transition` private or move the guards into it.

### F5 — Python approval decisions are reversible, flippable, and un-gated
`app/services/approval.py::decide` (ll. 33–56) overwrites `status`,
`decided_by`, and `decided_at` on every call, with no check that the
approval is still `open`. Re-deciding an approval flips
`approved ↔ rejected` (forbidden in Rust: `AlreadyDecided`,
`state_machine.rs` ll. 198–201), and since the target status is a
caller-supplied parameter, it can even be reset to `open`. The one call
site constrains the target only by string mapping — any value other
than `"approved"` becomes `rejected` (`app/api/routes_agents.py`
l. 390), a fail-unsafe default for a human-decision endpoint.

### F6 — Rust `ApprovalService` reports a second decision as "not found"
`ApprovalService::decide` searches only the `pending` list
(`rust/src/approval.rs` ll. 119–122) after decided approvals are moved
to the `decided` list, so approving an already-decided id returns
`ApprovalError::NotFound`, not `AlreadyDecided` — the crate's own test
enshrines this (`approve_already_decided`, approval.rs tests). The
`AlreadyDecided` check at ll. 125–128 is effectively dead: the service
only ever puts `Open` approvals into `pending`. Harmless today, but the
two layers disagree on the error contract.

### F7 — Task/workflow asymmetry on failure is unexplained
Failed workflows retry (`failed → pending`); failed tasks are terminal
(proved: `task_failed_absorbing`) with no retry or escalation exit, and
per F1 the two Rust tables can't even agree on how the workflow retry
re-enters. If tasks are meant to be single-shot, neither
`state_machine.rs` nor `types.rs` says so.

### Agreement points (for the record)
State sets and initial states are identical across Rust and Python;
both Rust tables agree that `completed` and `cancelled` are terminal
and `failed` is not (`types.rs` ll. 52–54); the enforced machine's own
test suite (state_machine.rs, ~80 tests) matches the modelled `stepB` /
`cancelB` / `taskStepB` / `apprB` tables arm for arm.
