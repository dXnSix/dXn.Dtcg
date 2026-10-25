---
agentx:
  kind: index
  scope: knowledge
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-25T16:00:00+02:00
  topics:
    - dtcg
    - taxonomy
  supersedes: []
  superseded_by: []
---
# dXn.Dtcg Knowledge Index

Current source, tests and manifests establish behavior. This tree records durable routing, rules and rationale for the DTCG 2025.10 runtime crates. Decisions live in the [decision index](../decisions/INDEX.md).

## Domains

- [runtime](runtime/INDEX.md): the `dxn_dtcg` and `dxn_dtcg_derive` crates: resolution, typed projection, source inspection.
- [repository](repository/INDEX.md): workspace packaging and git-tag releases, validation, AgentX governance.

## Source Anchors

- [Workspace manifest](../../../Cargo.toml)
- [README](../../../README.md)
- [dxn_dtcg public API](../../../crates/dxn_dtcg/src/lib.rs)
- [Derive macros](../../../crates/dxn_dtcg_derive/src/lib.rs)
