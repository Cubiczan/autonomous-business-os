# Adversarial Challenge Templates — autonomous-business-os

## Phase 0: Foundation Challenge
When a new decision enters CHP, the adversary MUST address:
1. Why is the proposed direction wrong? (vulnerability_strike)
2. What is the system not seeing? (invalidation_conditions)
3. What is the false consensus risk?

## Domain-Specific Challenges (Finance (CFO Accuracy))
1. What financial reporting errors could this decision introduce? Consider GAAP/IFRS compliance risks.
2. How would an incorrect revenue recognition assumption cascade through the financial statements?
3. What audit trail gaps exist that could trigger a material weakness finding?
4. If the CFO accuracy floor is breached, what is the blast radius for stakeholder trust?
5. What is the false precision risk — are we treating estimates as facts?

## Round 3: Implementation Drift Check
1. Does the implementation match the locked spec acceptance criteria?
2. Are operational handoffs and owner capacity accounted for?
3. Is evidence quality sufficient for the decision domain?

## Council Spawn Triggers
When confidence <85% on high-stakes decisions:
- Attacker Model 1: Challenge foundational assumptions
- Attacker Model 2: Challenge operational feasibility
- Synthesizer: Resolve contradictions and produce final recommendation
