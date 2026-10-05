---
agentx:
  kind: index
  scope: decisions
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-05T18:00:00+02:00
  topics:
    - decisions
    - dtcg
  supersedes: []
  superseded_by: []
---
# RayX.Dtcg Decision Index

Durable architecture choices for the DTCG runtime. Current source remains the first evidence to inspect. Decisions dated before 2026-10-05 were made while the crates lived in RayX (epic `dtcg-runtime`, [tasks](../../tasks/dtcg-runtime/design.json)) and were promoted here by the [transplant decision](repository/agentx-memory/2026-10-05-repository-agentx-memory-taxonomy-transplant.md). Why the crates live in a sibling repository is RayX's [sibling repositories decision](../../../../RayX/.agentx/memory/decisions/runtime/workspace/2026-10-05-runtime-workspace-sibling-repositories.md).

## Domains

- runtime
  - [Resolve from an I/O-free string catalog with explicit missing-document requests](runtime/resolution/2026-07-26-runtime-resolution-io-free-string-catalog.md)
  - [Overlay whole tokens by path, then resolve aliases on the final graph](runtime/resolution/2026-07-26-runtime-resolution-whole-token-overlay.md)
  - [Project narrow typed views through derives and FromDtcgValue](runtime/projection/2026-07-26-runtime-projection-narrow-typed-projection.md)

- repository
  - [Crate-area taxonomy transplanted from RayX](repository/agentx-memory/2026-10-05-repository-agentx-memory-taxonomy-transplant.md)
