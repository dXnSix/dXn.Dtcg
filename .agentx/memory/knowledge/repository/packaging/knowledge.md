---
agentx:
  kind: knowledge
  scope: knowledge/repository/packaging
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-05T18:00:00+02:00
  topics:
    - packaging
    - crates-io
  supersedes: []
  superseded_by: []
---
# Packaging Knowledge

- Publish `dtcg_runtime_derive` before `dtcg_runtime`: the runtime's normal dependency on the derive crate must resolve from the registry.
