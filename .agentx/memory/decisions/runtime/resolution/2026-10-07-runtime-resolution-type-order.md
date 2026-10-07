---
agentx:
  kind: decision
  scope: decisions/runtime/resolution
  status: current
  created: 2026-10-07T12:00:00+02:00
  updated: 2026-10-07T12:00:00+02:00
  topics:
    - types
    - aliases
    - conformance
  supersedes: []
  superseded_by: []
---
## Decision: Determine a token type from its own type, then its whole-reference target, then its group

### Context: The runtime let an inherited group `$type` win over an alias target type, against Format 5.2.2; [epic `dtcg-2025-10-conformance`](../../../../tasks/dtcg-2025-10-conformance/design.json), design `type-order`.

### Alternatives considered: Keep group-first inheritance; check value shapes instead of type names.

### Reasoning: The effective type is own `$type`, else the type of the token a whole reference targets (curly alias, single-member `$ref` to `.../$value`, or token-level `$ref`), else the nearest group type after `$extends`, else an error. A token with both its own type and a whole reference must match the target type by name (`reference type mismatch`), as corpus fixtures require.

### Trade-offs accepted: Documents that relied on a group type overriding an alias target type now take the target type or fail with a mismatch.

### Supersedes: None.
