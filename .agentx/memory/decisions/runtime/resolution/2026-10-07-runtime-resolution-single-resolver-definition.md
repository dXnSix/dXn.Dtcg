---
agentx:
  kind: decision
  scope: decisions/runtime/resolution
  status: current
  created: 2026-10-07T12:00:00+02:00
  updated: 2026-10-07T12:00:00+02:00
  topics:
    - resolver
    - validation
    - steps
  supersedes: []
  superseded_by: []
---
## Decision: Give resolver structure one owner and derive every step view from it

### Context: `resolve`, `modifiers()` and input validation read different definitions of the same `$ref` step with sibling overrides; [epic `dtcg-2025-10-conformance`](../../../../tasks/dtcg-2025-10-conformance/design.json), designs `resolver-validation`, `source-reference-bounds` and `source-reference-discovery-units`.

### Alternatives considered: Patch each consumer separately; validate structure only in `resolve`.

### Reasoning: `resolver_definition.rs` owns the allowed-member tables, the `resolutionOrder` and context-count rules, and `effective_steps` (each step with `$ref` sibling overrides applied). Discovery, `modifiers()`, `normalize_selection`, input validation, envelopes and `source_references` all use it, so no API can disagree about a step. `source_references` walks reference objects as the overlaid target and bounds its walk: first walks are free, and repeated walks under different overrides count toward `max_documents` in the units discovery uses (steps and inline sources, separately), so it lists every document `validate_complete` accepts except through dormant definitions.

### Trade-offs accepted: Unknown members in resolvers, sets, modifiers and inline steps are rejected everywhere, which forces consumer fixtures to move editor data into `$defs` or `$extensions`.

### Supersedes: None.
