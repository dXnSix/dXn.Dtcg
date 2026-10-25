---
agentx:
  kind: decision
  scope: decisions/repository/agentx-memory
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-25T16:00:00+02:00
  topics:
    - agentx-memory
    - taxonomy
  supersedes: []
  superseded_by: []
---
## Decision: Organize dXn.Dtcg's AgentX memory by crate area

### Context: The repository needs its own memory that routes agents to the source it describes, with the runtime's design decisions recorded next to it.

### Alternatives considered: One flat knowledge file; a taxonomy by document type; a taxonomy derived from the crates' module boundaries.

### Reasoning: Memory belongs with the source it describes. The runtime crate has three stable areas (resolution, projection, source inspection), which are the `runtime` subdomains; `repository` holds packaging, validation and memory governance. Each runtime decision was checked against current source when recorded (the shipped `TypeRegistry` and `rest` differ from the original design).

### Trade-offs accepted: No `ui`, `platform`, `examples` or `tooling` domains, because this repository has no UI, platform code, apps or build tooling.

### Supersedes: None.
