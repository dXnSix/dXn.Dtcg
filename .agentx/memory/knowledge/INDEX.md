---
agentx:
  kind: index
  scope: knowledge
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-05T18:00:00+02:00
  topics:
    - dtcg
    - taxonomy
  supersedes: []
  superseded_by: []
---
# RayX.Dtcg Knowledge Index

Current source, tests and manifests establish behavior. This tree records durable routing, rules and rationale for the DTCG 2025.10 runtime crates. Decisions live in the [decision index](../decisions/INDEX.md).

## Domains

- [runtime](runtime/INDEX.md): the `dtcg_runtime` and `dtcg_runtime_derive` crates: resolution, typed projection, source inspection.
- [repository](repository/INDEX.md): workspace packaging and crates.io publishing, validation, RayX co-development, AgentX governance.

## Source Anchors

- [Workspace manifest](../../../Cargo.toml)
- [README](../../../README.md)
- [dtcg_runtime public API](../../../crates/dtcg_runtime/src/lib.rs)
- [Derive macros](../../../crates/dtcg_runtime_derive/src/lib.rs)
