---
agentx:
  kind: state
  scope: knowledge/runtime
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-05T18:00:00+02:00
  topics:
    - runtime
    - dtcg-runtime
    - public-api
  supersedes: []
  superseded_by: []
---
# Runtime State

## Purpose And Usage

Read before changing the public API or deciding which module owns a behavior.

## Current Technical Shape

`dtcg_runtime` resolves DTCG 2025.10 token documents and resolvers from already-loaded JSON strings. Its only dependencies are `serde`, `serde_json` (with `preserve_order`), `sha2`, `thiserror` and the derive crate; nothing touches the filesystem, network, an async runtime, RayX, gpux or GPUI. `DtcgRuntime` (`runtime.rs`) is the facade: it owns a `DocumentCatalog`, an optional root `ResourceId`, `RuntimeLimits` and a stateless `TypeRegistry`, and exposes resolution, projection and source-inspection entry points. `DtcgError` (`error.rs`) is the single error type.

Consumers: RayX `rayx_theme` (theme project loading and the generated `Theme` projection), RayX's Test Suite DTCG page, and RayX.TokenEditor (inspection, preview, affected resolution).

## Backlinks

- [Public API](../../../../crates/dtcg_runtime/src/lib.rs)
- [DtcgRuntime facade](../../../../crates/dtcg_runtime/src/runtime.rs)
- [Errors](../../../../crates/dtcg_runtime/src/error.rs)
- 2026-10-05 current [Domain index](INDEX.md)
