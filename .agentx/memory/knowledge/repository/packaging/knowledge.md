---
agentx:
  kind: knowledge
  scope: knowledge/repository/packaging
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-23T22:00:00+02:00
  topics:
    - packaging
    - crates-io
  supersedes: []
  superseded_by: []
---
# Packaging Knowledge

- Publish `dxn_dtcg_derive` before `dxn_dtcg`: the runtime's normal dependency on the derive crate must resolve from the registry.
