---
agentx:
  kind: state
  scope: knowledge/runtime/projection
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-07T11:00:00+02:00
  topics:
    - projection
    - derive
    - values
    - extensions
  supersedes: []
  superseded_by: []
---
# Projection State

## Purpose And Usage

Read before changing the derives, the projection traits, value conversion, or the accepted value shapes.

## Current Technical Shape

`DtcgTokens::project_at(tokens, registry, root)` projects a struct rooted at a dotted token path, so one struct type can be projected at several roots; `project` uses the document root, and `join_token_path` joins paths while ignoring empty parts. `#[derive(DtcgTokens)]` and `#[derive(DtcgComponentTokens)]` (which adds `component_id()`) live in `dtcg_runtime_derive` and are re-exported by `dtcg_runtime`.

Struct attributes: `#[dtcg(root = "...")]` and, for components, `#[dtcg(id = "...")]` (defaults to the struct name). Field attributes: `path`, `rename`, `nested` (project a nested `DtcgTokens` at the joined path), `with = "fn"` (a `fn(&ResolvedToken) -> Result<T, DtcgError>` for foreign types, avoiding orphan impls) and `rest` (at most one field, currently initialized with `Default`). Leaves convert through `TypeRegistry::convert::<T>`, a stateless forwarder to `FromDtcgValue` (built-in for `String`, `f64`, `f32`, `bool`). A missing requested token fails with `DtcgError::Projection`, except for `Option<T>` fields (also spelled `core::`/`std::option::Option`), which become `None`; unrequested tokens are ignored but still serve as alias targets. Attributes are parsed strictly: unknown, duplicated or misplaced keys and conflicting combinations (`nested`+`with`, `path`+`rename`, `rest` with anything) are spanned compile errors pinned by `trybuild` snapshots in `crates/dtcg_runtime_derive/tests/ui`. Generated code uses fully qualified paths and unraws field names; generic bounds are added per field (`FromDtcgValue` for leaves, `DtcgTokens` for nested, `Default` for rest).

`dtcg_runtime::values` models every DTCG type (plus the RayX `Blur` and `LineHeight`) with the spec's consumer rules (alpha default 1, clamped gradient positions, font-weight keyword numbers, single-layer shadows); each implements `FromDtcgValue`, and `ResolvedToken::typed(&ValueExtensions)` converts by effective type. `f32` projection rejects numbers outside the finite `f32` range. `ResolvedTokens::to_document` exports a resolution as a DTCG document that re-resolves to the same tokens.

`validate_value(type, value)` checks resolved values strictly against the 13 DTCG 2025.10 types; `validate_value_with(type, value, &ValueExtensions)` also accepts the opt-in RayX shapes: the extension type `dev.rayx.blur` (`BLUR_TYPE`: `{ "kind": "layer" | "background", "radius": <dimension> }`) and a `dimension` typography `lineHeight`.

## Backlinks

- [Projection trait](../../../../../crates/dtcg_runtime/src/dtcg_tokens.rs)
- [Value conversion](../../../../../crates/dtcg_runtime/src/from_dtcg_value.rs)
- [Value validation](../../../../../crates/dtcg_runtime/src/value.rs)
- [Typed values](../../../../../crates/dtcg_runtime/src/values.rs)
- [Derive macros](../../../../../crates/dtcg_runtime_derive/src/lib.rs)
- [RayX export value shapes](../../../../../crates/dtcg_runtime/tests/rayx_export_types.rs)
- 2026-07-26 current [narrow typed projection decision](../../../decisions/runtime/projection/2026-07-26-runtime-projection-narrow-typed-projection.md)
