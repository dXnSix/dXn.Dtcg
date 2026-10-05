---
agentx:
  kind: rules
  scope: knowledge/repository/packaging
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-05T18:00:00+02:00
  topics:
    - packaging
    - dependencies
  supersedes: []
  superseded_by: []
---
# Packaging Rules

- Keep the derive crate's dev-dependency on `dtcg_runtime` path-only (no `version`); adding a version reintroduces the publish cycle.
- Declare every dependency once in `[workspace.dependencies]` and inherit package metadata from `[workspace.package]`.
- Keep `cargo package --workspace` passing; it is part of the pre-commit list and CI.
