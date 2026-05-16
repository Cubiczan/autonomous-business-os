# CHP State Machine — autonomous-business-os

## Protocol: Consensus Hardening Protocol (CHP) v1.0
## Domain: Finance (CFO Accuracy)
## Applied: 2026-05-16

### States
- EXPLORING: Initial decision exploration with foundation disclosure
- PROVISIONAL: Foundation score ≥100, devil's advocate complete
- PROVISIONAL_LOCK: Ready for third-party validation
- LOCKED: Third-party CONFIRM received, decision committed
- CONVERGED: Cross-agent agreement achieved
- UNRESOLVED: Forced at round 5 if no convergence
- REQUIRES_HUMAN_VERIFICATION: CFO accuracy guard tripped
- REFRAME_REQUIRED: Foundation score <100
- HALT: R0 gate fatal or context parity significant

### Phase Progression
FOUNDATION (Phase 0) → SPEC (Phase 1) → IMPLEMENTATION (Phase 2)
Phase transitions occur at round boundaries: FOUNDATION→SPEC at round 1, SPEC→IMPLEMENTATION at round 3.

### State Transitions
- EXPLORING → PROVISIONAL: Foundation score ≥ 100, CFO accuracy guard passed (no open vulnerabilities/blind spots)
- EXPLORING → REQUIRES_HUMAN_VERIFICATION: Foundation score < 100
- PROVISIONAL → PROVISIONAL_LOCK: Devil's advocate complete, no outstanding financial blind spots
- PROVISIONAL_LOCK → LOCKED: Third-party CONFIRM received with financial accuracy validation
- PROVISIONAL_LOCK → EXPLORING: Third-party REJECT with financial correction criteria
- LOCKED → CONVERGED: Cross-agent financial consensus achieved
- Any → HALT: CFO accuracy guard tripped with material financial risk
- Any → UNRESOLVED: Forced at round 5 if no financial convergence

### R0 Gate (Session Entry)
All four checks must PASS:
- Solvable: The decision can be resolved within the domain's constraints
- Scoped: Clear scope boundaries defined in dossier
- Valid: Current state and goal state are specified
- Worth_it: Stakes justify the governance overhead

### Foundation Score Thresholds
- General: ≥70 PASS, <70 REFRAME
- Finance/CFO: ≥100 (CFOAccuracyPolicy), <100 REQUIRES_HUMAN_VERIFICATION
- Blockchain/DeFi: ≥85 (elevated due to immutable tx risk)

### Adversary Schedule
- Phase 0, Round 0: Mandatory devil's advocate from FoundationDisclosure + FoundationAttack
- Phase 2, Round 3: Implementation drift check devil's advocate
- Council Spawn: high_stakes=True AND confidence <85 → 3-model cross-review

### Third-Party Validation
- PROVISIONAL_LOCK → CONFIRM → LOCKED
- PROVISIONAL_LOCK → REJECT → EXPLORING (with flip_criteria)
