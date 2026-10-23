---
agentx:
  kind: decision
  scope: decisions/repository/agentx-memory
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-23T22:00:00+02:00
  topics:
    - agentx-memory
    - taxonomy
    - transplant
    - sibling-repositories
  supersedes: []
  superseded_by: []
---
## Decision: Give dXn.Dtcg its own AgentX memory with a crate-area taxonomy, transplanted from RayX

### Context: The DTCG runtime crates moved from RayX `crates/rayx/` into this repository on 2026-10-05. Their memory stayed in RayX: runtime facts were scattered across RayX's `ui/theming-components`, `runtime/workspace` and `tooling/validation` state files, the completed `dtcg-runtime` epic stayed in RayX `.agentx/tasks/`, and the epic's design decisions had never been promoted into memory.

### Alternatives considered: Keep the memory in RayX and link to it; copy RayX's theming subdomain; derive a taxonomy from this repository's own module boundaries.

### Reasoning: Memory belongs with the source it describes. The crate has three stable areas (resolution, projection, source inspection), which become the `runtime` subdomains; `repository` holds packaging, validation and memory governance. Moved: the runtime facts from RayX knowledge (affected-resolution restore and `nodes_visited` accounting, the `keep_last_definitions` measurement, inspection/preview/relocation behavior, `project_at`/`with`), the `dtcg-runtime` epic directory, and DES-001 to DES-004 of its design, promoted as three decisions and checked against current source (the shipped `TypeRegistry` and `rest` differ from the design). RayX keeps its theme decisions and theming epics, including the superseded theme-model crate split that mentions the old RayX DTCG parser, because their subject is RayX's theme.

### Trade-offs accepted: The transplanted epic JSON was not edited, so its paths and hashes refer to RayX. RayX keeps one-line pointers and a [transplant record](../../../../../../../RayX/RayX.Old/.agentx/memory/decisions/tooling/agentx-memory/2026-10-05-tooling-agentx-memory-dtcg-memory-transplant.md). Excluded categories: no `ui`, `platform`, `examples` or `tooling` domains, because this repository has no UI, platform code, apps or build tooling.

### Supersedes: None.
