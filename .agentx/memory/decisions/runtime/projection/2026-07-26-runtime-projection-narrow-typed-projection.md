---
agentx:
  kind: decision
  scope: decisions/runtime/projection
  status: current
  created: 2026-07-26T16:30:00+02:00
  updated: 2026-10-25T16:00:00+02:00
  topics:
    - projection
    - derive
    - type-registry
  supersedes: []
  superseded_by: []
---
## Decision: Project narrow typed views through derives and FromDtcgValue

### Context: Consumers need typed Rust structs for a subset of the active token graph, and Serde cannot pick a conversion by DTCG semantic type.

### Alternatives considered: Serde deserialization of the resolved JSON tree; a converter registry keyed by Rust `TypeId` plus effective `$type` (the original design); consumer-specific converters inside the runtime.

### Reasoning: `#[derive(DtcgTokens)]`/`#[derive(DtcgComponentTokens)]` generate per-field projection: a missing requested token fails, extra tokens are tolerated, and unprojected tokens stay available as alias targets. The shipped `TypeRegistry` forwards to the `FromDtcgValue` trait instead of a `TypeId` map, which is simpler and statically checked. Consumers convert types they do not own with `#[dtcg(with = "fn")]`, and `project_at` projects one struct type at several roots (added 2026-09-23 for generated models that repeat one shape).

### Trade-offs accepted: Converter choice is per Rust type, not per `(type, $type)` pair; a field needing a different interpretation uses `with`. `#[dtcg(rest)]` is a `Default` placeholder, not the planned collector of unclaimed groups.

### Supersedes: None.
