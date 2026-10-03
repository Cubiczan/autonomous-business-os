/-
  Workflow.lean — Lean 4 (core only) formal model of the workflow, task and
  approval state machines in autonomous-business-os.

  Sources (read in full before modelling):
    * rust/src/state_machine.rs — the ENFORCED machines:
      `WorkflowStateMachine::can_transition` (ll. 41–58) + `transition`
      (ll. 68–76), the convenience methods `mark_running` (81–85),
      `mark_completed` (88–97), `mark_failed` (100–104),
      `mark_waiting_for_human` (107–109), `mark_cancelled` (112–128 —
      a separate path that bypasses `can_transition`), `retry` (131–140);
      `TaskStateMachine::can_transition` (159–171);
      `ApprovalStateMachine::can_transition` / `decide` (190–230).
    * rust/src/types.rs — the status enums (ll. 15–22, 60–66, 70–74) and a
      SECOND, declarative transition table `valid_transitions` (ll. 26–45)
      which has no call sites anywhere and DISAGREES with the enforced
      machine (see the `diverge_*` theorems below).
    * rust/src/approval.rs — `ApprovalService` (decide at ll. 113–137).
    * app/services/workflows.py, app/services/approval.py, app/models.py —
      the Python implementation, which performs NO transition validation
      at all; it is documented in NOTES.md, not modelled (there is no
      nontrivial relation to model: every assignment succeeds).

  The model is the Rust enforced machine, as `Exec = Step ∪ Cancel`.
  Relations are given as Boolean tables exactly mirroring the Rust `match`
  arms, so every finite fact below is decidable by `decide`/`rfl`.
  Timestamps, results/errors payloads and the attempts counter are
  abstracted away (the counter's guard is analysed in NOTES.md).
-/

namespace AutonomousBusinessOS

/-! ## Generic reachability (reflexive–transitive closure, snoc form) -/

section Reachability

variable {α : Type} {R : α → α → Prop}

/-- Reflexive–transitive closure of `R`: execution traces. A `Reach R a b`
    derivation is a finite sequence of legal `R` steps from `a` to `b`. -/
inductive Reach (R : α → α → Prop) : α → α → Prop where
  | refl (a : α) : Reach R a a
  | snoc {a b c : α} : Reach R a b → R b c → Reach R a c

/-- **No state skipping**: any trace from `a` to a *different* state `b`
    begins with a single legal step out of `a`. There is no way to reach a
    state except through the transition relation, one step at a time. -/
theorem Reach.head_decomp (h : Reach R a b) :
    a = b ∨ ∃ c, R a c ∧ Reach R c b := by
  induction h with
  | refl => exact Or.inl rfl
  | snoc h hr ih =>
    rcases ih with e | ⟨c, hc, hcb⟩
    · subst e
      exact Or.inr ⟨_, hr, Reach.refl _⟩
    · exact Or.inr ⟨c, hc, Reach.snoc hcb hr⟩

/-- A state with no outgoing `R` steps is **absorbing**: once reached, a
    trace stays there forever. -/
theorem Reach.absorbing {t : α} (hno : ∀ s, ¬ R t s) (h : Reach R t s) :
    s = t := by
  induction h with
  | refl => rfl
  | snoc h hr ih => subst ih; exact absurd hr (hno _)

end Reachability

/-! ## The workflow machine (Rust, enforced) -/

/-- `WorkflowStatus`, rust/src/types.rs ll. 15–22. Initial state is
    `pending` (`Workflow::new`, types.rs l. 194). -/
inductive WState where
  | pending | running | waitingForHuman | completed | failed | cancelled
  deriving DecidableEq, Repr

/-- `WorkflowStateMachine::can_transition`, state_machine.rs ll. 41–58,
    transcribed arm by arm. NOTE: this table has NO `→ cancelled` edges. -/
def stepB : WState → WState → Bool
  | .pending, .running => true
  | .running, .completed => true
  | .running, .failed => true
  | .running, .waitingForHuman => true
  | .waitingForHuman, .running => true
  | .waitingForHuman, .completed => true
  | .waitingForHuman, .failed => true
  | .failed, .pending => true
  | _, _ => false

/-- The cancellation edges, which live ONLY in `mark_cancelled`
    (state_machine.rs ll. 112–128): any state except `completed` /
    `cancelled` may be cancelled, via a code path that never consults
    `can_transition`. -/
def cancelB : WState → WState → Bool
  | .pending, .cancelled => true
  | .running, .cancelled => true
  | .waitingForHuman, .cancelled => true
  | .failed, .cancelled => true
  | _, _ => false

/-- Steps of the generic `transition()` primitive. -/
abbrev Step (a b : WState) : Prop := stepB a b = true

/-- Steps of the `mark_cancelled` path. -/
abbrev Cancel (a b : WState) : Prop := cancelB a b = true

/-- The full behaviour of `WorkflowStateMachine`'s public API:
    `transition()` steps plus `mark_cancelled` steps. -/
abbrev Exec (a b : WState) : Prop := (stepB a b || cancelB a b) = true

/-- The second, declarative table: `WorkflowStatus::valid_transitions`,
    types.rs ll. 26–45. It has no call sites in the crate; it is modelled
    only to state the divergence theorems below. -/
def stepTB : WState → WState → Bool
  | .pending, .running => true
  | .pending, .cancelled => true
  | .running, .waitingForHuman => true
  | .running, .completed => true
  | .running, .failed => true
  | .running, .cancelled => true
  | .waitingForHuman, .running => true
  | .waitingForHuman, .completed => true
  | .waitingForHuman, .failed => true
  | .waitingForHuman, .cancelled => true
  | .failed, .running => true
  | _, _ => false

abbrev StepT (a b : WState) : Prop := stepTB a b = true

/-! ### Divergence: the two Rust tables disagree (machine-checked) -/

/-- `types.rs` sends a failed workflow straight back to `running`;
    the enforced machine does not — it must pass through `pending`
    (the `retry` path). -/
theorem diverge_failed_successor :
    StepT .failed .running ∧ ¬ Exec .failed .running :=
  ⟨rfl, by decide⟩

/-- The enforced machine can cancel a failed workflow (`mark_cancelled`);
    the `types.rs` table forbids it. -/
theorem diverge_failed_cancel :
    Exec .failed .cancelled ∧ ¬ StepT .failed .cancelled :=
  ⟨rfl, by decide⟩

/-- `types.rs` advertises `pending → cancelled` as an ordinary transition,
    but `can_transition` rejects it: the generic `transition()` primitive
    can never cancel anything — cancellation exists only in the separate
    `mark_cancelled` path (which is why it appears in `Exec`). -/
theorem diverge_pending_cancel :
    ¬ Step .pending .cancelled ∧ Exec .pending .cancelled :=
  ⟨by decide, rfl⟩

/-! ### Shape of the enforced machine -/

/-- Only `running` can enter `waitingForHuman`: the human gate can only be
    reached from an actively executing workflow. -/
theorem waiting_in_gate : Exec a .waitingForHuman → a = .running := by
  cases a <;> simp [Exec, stepB, cancelB] at *

theorem completed_in_char :
    Exec a .completed → a = .running ∨ a = .waitingForHuman := by
  cases a <;> simp [Exec, stepB, cancelB] at *

theorem pending_in_char : Exec a .pending → a = .failed := by
  cases a <;> simp [Exec, stepB, cancelB] at *

theorem running_in_char :
    Exec a .running → a = .pending ∨ a = .waitingForHuman := by
  cases a <;> simp [Exec, stepB, cancelB] at *

theorem pending_out_char :
    Exec .pending b → b = .running ∨ b = .cancelled := by
  cases b <;> simp [Exec, stepB, cancelB] at *

theorem failed_out_char :
    Exec .failed b → b = .pending ∨ b = .cancelled := by
  cases b <;> simp [Exec, stepB, cancelB] at *

/-- Exits from `waitingForHuman` are exactly these four — and every one of
    them is an ordinary `transition()`/`mark_cancelled` step. No approval
    event exists anywhere in this relation: the code never consults an
    `ApprovalService` before a waiting workflow resumes, completes, or
    fails (see NOTES.md, finding F4). -/
theorem waiting_out_char :
    Exec .waitingForHuman b →
      b = .running ∨ b = .completed ∨ b = .failed ∨ b = .cancelled := by
  cases b <;> simp [Exec, stepB, cancelB] at *

/-! ### Terminal states are absorbing -/

theorem not_exec_completed : ∀ s, ¬ Exec .completed s := by
  intro s; cases s <;> decide

theorem not_exec_cancelled : ∀ s, ¬ Exec .cancelled s := by
  intro s; cases s <;> decide

theorem completed_absorbing : Reach Exec .completed s → s = .completed :=
  Reach.absorbing not_exec_completed

theorem cancelled_absorbing : Reach Exec .cancelled s → s = .cancelled :=
  Reach.absorbing not_exec_cancelled

/-! ### No skipping: completion is only reachable through `running` -/

/-- Invariant: anything reachable from the initial state `pending` is
    either still `pending` or has already passed through `running`. -/
theorem pending_cases (h : Reach Exec .pending s) :
    s = .pending ∨ Reach Exec .running s := by
  induction h with
  | refl => exact Or.inl rfl
  | snoc h hs ih =>
    rcases ih with e | hr
    · subst e
      rcases pending_out_char hs with e2 | e2
      · subst e2; exact Or.inr (Reach.refl _)
      · subst e2; exact Or.inr (Reach.snoc (Reach.refl _) rfl)
    · exact Or.inr (Reach.snoc hr hs)

/-- A workflow cannot jump `pending → completed`: every completed workflow
    has genuinely executed (passed through `running`). -/
theorem running_unavoidable_completed :
    Reach Exec .pending .completed → Reach Exec .running .completed := by
  intro h
  rcases pending_cases h with e | hr
  · cases e
  · exact hr

/-- Likewise the human gate cannot be reached without executing first. -/
theorem running_unavoidable_waiting :
    Reach Exec .pending .waitingForHuman →
      Reach Exec .running .waitingForHuman := by
  intro h
  rcases pending_cases h with e | hr
  · cases e
  · exact hr

/-! ## The task machine (Rust, enforced) -/

/-- `TaskStatus`, types.rs ll. 60–66. Initial state is `queued`. -/
inductive TState where
  | queued | running | completed | failed | escalated
  deriving DecidableEq, Repr

/-- `TaskStateMachine::can_transition`, state_machine.rs ll. 159–171. -/
def taskStepB : TState → TState → Bool
  | .queued, .running => true
  | .running, .completed => true
  | .running, .failed => true
  | .running, .escalated => true
  | .escalated, .running => true
  | _, _ => false

abbrev TaskStep (a b : TState) : Prop := taskStepB a b = true

theorem task_escalated_in : TaskStep a .escalated → a = .running := by
  cases a <;> simp [TaskStep, taskStepB] at *

theorem task_running_in :
    TaskStep a .running → a = .queued ∨ a = .escalated := by
  cases a <;> simp [TaskStep, taskStepB] at *

theorem task_completed_in : TaskStep a .completed → a = .running := by
  cases a <;> simp [TaskStep, taskStepB] at *

theorem not_taskstep_completed : ∀ s, ¬ TaskStep .completed s := by
  intro s; cases s <;> decide

theorem not_taskstep_failed : ∀ s, ¬ TaskStep .failed s := by
  intro s; cases s <;> decide

theorem task_completed_absorbing :
    Reach TaskStep .completed s → s = .completed :=
  Reach.absorbing not_taskstep_completed

/-- Unlike workflows, a failed task has NO outgoing transition at all:
    there is no task-level retry (contrast `failed_out_char`). -/
theorem task_failed_absorbing : Reach TaskStep .failed s → s = .failed :=
  Reach.absorbing not_taskstep_failed

/-! ## The approval machine (Rust, enforced) -/

/-- `ApprovalStatus`, types.rs ll. 70–74. Initial state is `open`. -/
inductive AState where
  | open | approved | rejected
  deriving DecidableEq, Repr

/-- The human decision event: `ApprovalDecision`, types.rs ll. 78–81,
    as consumed by `ApprovalStateMachine::decide`
    (state_machine.rs ll. 203–230). -/
inductive AEvent where
  | approve | reject
  deriving DecidableEq, Repr

/-- The labelled approval relation: the ONLY steps are the two `decide`
    edges out of `open` (`can_transition`, state_machine.rs ll. 190–196). -/
def apprB : AEvent → AState → AState → Bool
  | .approve, .open, .approved => true
  | .reject, .open, .rejected => true
  | _, _, _ => false

abbrev ApprStep (e : AEvent) (a b : AState) : Prop := apprB e a b = true

/-- Unlabelled approval steps. -/
abbrev ApprU (a b : AState) : Prop := ∃ e, ApprStep e a b

/-- **Approval gate**: `approved` can be entered only by the `approve`
    decision event, and only from `open`. There is no other path — no
    self-approval, no approval of an already-decided request. -/
theorem approved_gate :
    ApprStep e a .approved → e = .approve ∧ a = .open := by
  cases e <;> cases a <;> simp [ApprStep, apprB] at *

theorem rejected_gate :
    ApprStep e a .rejected → e = .reject ∧ a = .open := by
  cases e <;> cases a <;> simp [ApprStep, apprB] at *

theorem apprU_into_approved : ApprU a .approved → a = .open := by
  rintro ⟨e, h⟩
  exact (approved_gate h).2

theorem apprU_into_rejected : ApprU a .rejected → a = .open := by
  rintro ⟨e, h⟩
  exact (rejected_gate h).2

theorem not_apprU_from_approved : ∀ s, ¬ ApprU .approved s := by
  rintro s ⟨e, h⟩
  cases e <;> cases s <;> simp [ApprStep, apprB] at h

theorem not_apprU_from_rejected : ∀ s, ¬ ApprU .rejected s := by
  rintro s ⟨e, h⟩
  cases e <;> cases s <;> simp [ApprStep, apprB] at h

theorem approved_absorbing : Reach ApprU .approved s → s = .approved :=
  Reach.absorbing not_apprU_from_approved

theorem rejected_absorbing : Reach ApprU .rejected s → s = .rejected :=
  Reach.absorbing not_apprU_from_rejected

/-- **Decisions are irreversible**: an approved approval can never become
    rejected, nor vice versa (the `AlreadyDecided` error in the code). -/
theorem approval_irreversible : ¬ Reach ApprU .approved .rejected := by
  intro h
  have h' := approved_absorbing h
  cases h'

theorem approval_irreversible' : ¬ Reach ApprU .rejected .approved := by
  intro h
  have h' := rejected_absorbing h
  cases h'

end AutonomousBusinessOS
