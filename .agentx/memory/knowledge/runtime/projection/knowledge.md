---
agentx:
  kind: knowledge
  scope: knowledge/runtime/projection
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-05T18:00:00+02:00
  topics:
    - projection
    - derive
    - type-registry
  supersedes: []
  superseded_by: []
---
# Projection Knowledge

- The original `dtcg-runtime` design planned a converter registry keyed by Rust `TypeId` plus effective DTCG `$type`, and a `rest` field collecting unclaimed groups. What shipped is simpler: `TypeRegistry` forwards to the `FromDtcgValue` trait, foreign types use `#[dtcg(with = "fn")]`, and `rest` is a `Default` placeholder.
- `project_at` and `with` exist for RayX's generated `rayx_theme` model, which projects one shared struct at several roots and converts gpux types it cannot implement `FromDtcgValue` for.
