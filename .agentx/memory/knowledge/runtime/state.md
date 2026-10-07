---
agentx:
  kind: state
  scope: knowledge/runtime
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-07T04:00:00+02:00
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

`dtcg_runtime` resolves DTCG 2025.10 token documents and resolvers from already-loaded JSON strings. Its only dependencies are `serde`, `serde_json` (with `preserve_order`), `sha2`, `thiserror` and the derive crate; nothing touches the filesystem, network, an async runtime, RayX, gpux or GPUI. `DtcgRuntime` (`runtime.rs`) is the facade: it owns a `DocumentCatalog`, an optional root `ResourceId`, `RuntimeLimits` and a stateless `TypeRegistry`, and exposes resolution, projection and source-inspection entry points. `DtcgError` (`error.rs`) is the single error type; it is `#[non_exhaustive]`, `Clone` and `PartialEq`. Missing alias and `$extends` targets raise `UnresolvedReference { resource, path, target, message }`; `Projection` is reserved for projection. `ResolvedToken`, `RuntimeLimits` (built with `with_*` setters), `ModifierAxis`, `DocumentRequest`, `ValueValidationError` and `DtcgDiagnosticCode` are also `#[non_exhaustive]`. `DtcgRuntime`'s `Debug` prints ids and limits, never document bodies.

Value validation is strict DTCG 2025.10 by default; `RuntimeOptions { limits, extensions }` with `ValueExtensions::rayx()` opts into the RayX shapes (dimension typography `lineHeight`, `dev.rayx.blur`), which RayX and RayX.TokenEditor must enable (`DtcgRuntime::with_options`, `from_json_with_options`, `from_envelope_with_options`, `validate_value_with`).

Conformance is measured against the official DTCG 2025.10 corpus vendored byte-identically under `crates/dtcg_runtime/tests/fixtures/dtcg-test-suite/2025.10/` (excluded from the package); `tests/official_test_suite.rs` ratchets the failing fixtures against its `KNOWN_GAPS` list.

Consumers: RayX `rayx_theme` (theme project loading and the generated `Theme` projection), RayX's Test Suite DTCG page, and RayX.TokenEditor (inspection, preview, affected resolution).

## Backlinks

- [Public API](../../../../crates/dtcg_runtime/src/lib.rs)
- [DtcgRuntime facade](../../../../crates/dtcg_runtime/src/runtime.rs)
- [Errors](../../../../crates/dtcg_runtime/src/error.rs)
- 2026-10-05 current [Domain index](INDEX.md)
