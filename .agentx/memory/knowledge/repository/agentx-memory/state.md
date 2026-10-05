---
agentx:
  kind: state
  scope: knowledge/repository/agentx-memory
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-05T18:00:00+02:00
  topics:
    - agentx-memory
    - taxonomy
    - transplant
  supersedes: []
  superseded_by: []
---
# AgentX Memory State

## Purpose And Usage

Read before adding domains, moving memory, or resuming an epic from `.agentx/tasks/`.

## Current Technical Shape

Two knowledge domains: `runtime` (subdomains `resolution`, `projection`, `source-inspection`, following the crate's module areas) and `repository` (`packaging`, `validation`, `agentx-memory`). Decisions mirror those domains. `.agentx/tasks/dtcg-runtime/` is the completed epic that created the crates inside RayX (schema v4, 4 sprints, done 2026-07-26); its JSON is historical, untouched by the transplant, and refers to RayX paths and commit hashes.

## Backlinks

- 2026-10-05 current [Knowledge index](../../INDEX.md)
- 2026-10-05 current [Decision index](../../../decisions/INDEX.md)
- [dtcg-runtime epic design](../../../../tasks/dtcg-runtime/design.json)
- 2026-10-05 current [taxonomy transplant decision](../../../decisions/repository/agentx-memory/2026-10-05-repository-agentx-memory-taxonomy-transplant.md)
