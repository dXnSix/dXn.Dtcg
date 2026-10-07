---
agentx:
  kind: decision
  scope: decisions/runtime/resolution
  status: current
  created: 2026-10-07T12:00:00+02:00
  updated: 2026-10-07T12:00:00+02:00
  topics:
    - inputs
    - modifiers
    - nested-resolvers
  supersedes: []
  superseded_by: []
---
## Decision: Match inputs case-insensitively and validate them against the union of reached resolvers

### Context: Resolver 5.1 recommends case-insensitive inputs and nested resolvers are unspecified; explicit selections failed when a nested resolver or preview did not declare a modifier; [epic `dtcg-2025-10-conformance`](../../../../tasks/dtcg-2025-10-conformance/design.json), design `input-semantics`.

### Alternatives considered: Exact-only matching; requiring every resolver to declare every selected modifier.

### Reasoning: Each input matches exactly first, then by lowercase name (ambiguity lists candidates). Unknown keys are validated only once discovery is ready, against the modifiers any reached resolver declares (previews and inspection include the supplied source). Each resolver applies its own matching inputs or defaults; graphs without modifiers ignore inputs. `selection_from_json` parses JSON inputs (`InvalidInput`).

### Trade-offs accepted: A misspelled input is reported only after all documents are loaded; an input understood by any nested resolver is accepted for all.

### Supersedes: None.
