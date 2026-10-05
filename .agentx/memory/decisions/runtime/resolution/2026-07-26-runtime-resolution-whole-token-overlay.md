---
agentx:
  kind: decision
  scope: decisions/runtime/resolution
  status: current
  created: 2026-07-26T16:30:00+02:00
  updated: 2026-10-05T18:00:00+02:00
  topics:
    - resolution
    - overlay
    - aliases
  supersedes: []
  superseded_by: []
---
## Decision: Overlay whole tokens by path, then resolve aliases on the final graph

### Context: DTCG 2025.10 resolvers combine sets and modifier contexts in `resolutionOrder`; the earlier RayX compiler merged overrides field by field (epic `dtcg-runtime`, DES-002).

### Alternatives considered: Deep-merge JSON objects or Serde structs; resolve aliases per source before overlay.

### Reasoning: DTCG overlay operates on token identities. Sources flatten in declaration order and a later definition replaces the whole token at its path; effective types and aliases resolve afterwards across the complete active graph, so an alias always sees the winning definition for the selected context. Composite overrides are complete replacement values.

### Trade-offs accepted: A composite override must be a complete value; it is never merged with the definition it replaces. Unprojected tokens must stay in the active graph because projected tokens may alias them.

### Supersedes: None.
