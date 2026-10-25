---
agentx:
  kind: state
  scope: knowledge/repository/agentx-memory
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-25T16:00:00+02:00
  topics:
    - agentx-memory
    - taxonomy
  supersedes: []
  superseded_by: []
---
# AgentX Memory State

## Purpose And Usage

Read before adding domains or moving memory.

## Current Technical Shape

Two knowledge domains: `runtime` (subdomains `resolution`, `projection`, `source-inspection`, following the crate's module areas) and `repository` (`packaging`, `validation`, `agentx-memory`). Decisions mirror those domains. `.agentx/tasks/` holds no epics: finished epics were removed on 2026-10-25, and their durable decisions live in `decisions/`.

## Backlinks

- 2026-10-05 current [Knowledge index](../../INDEX.md)
- 2026-10-05 current [Decision index](../../../decisions/INDEX.md)
- 2026-10-05 current [crate-area taxonomy decision](../../../decisions/repository/agentx-memory/2026-10-05-repository-agentx-memory-crate-area-taxonomy.md)
