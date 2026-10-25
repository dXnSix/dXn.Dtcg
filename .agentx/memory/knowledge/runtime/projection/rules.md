---
agentx:
  kind: rules
  scope: knowledge/runtime/projection
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-25T16:00:00+02:00
  topics:
    - projection
    - derive
    - values
  supersedes: []
  superseded_by: []
---
# Projection Rules

- Keep projection narrow: a struct requests only its fields, extra active tokens are tolerated, and unprojected tokens stay available as alias targets.
- Convert consumer-owned types through `#[dtcg(with = "fn")]` in the consumer; do not add consumer-specific `FromDtcgValue` impls here.
- Keep every value shape beyond DTCG 2025.10 pinned by `tests/value_extension_types.rs`.
