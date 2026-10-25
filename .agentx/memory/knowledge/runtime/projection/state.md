---
agentx:
  kind: state
  scope: knowledge/runtime/projection
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-26T02:00:00+02:00
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

`DtcgTokens::project_at(tokens, registry, root)` projects a struct rooted at a dotted token path, so one struct type can be projected at several roots; `project` uses the document root, and `join_token_path` joins paths while ignoring empty parts. `#[derive(DtcgTokens)]` and `#[derive(DtcgComponentTokens)]` (which adds `component_id()`) live in `dxn_dtcg_derive` and are re-exported by `dxn_dtcg`.

Struct attributes: `#[dtcg(root = "...")]` and, for components, `#[dtcg(id = "...")]` (defaults to the struct name). Field attributes: `path`, `rename`, `nested` (project a nested `DtcgTokens` at the joined path), `with = "fn"` (a `fn(&ResolvedToken) -> Result<T, DtcgError>` for foreign types, avoiding orphan impls) and `rest` (at most one field, currently initialized with `Default`). Leaves convert through `TypeRegistry::convert::<T>`, a stateless forwarder to `FromDtcgValue` (built-in for `String`, `f64`, `f32`, `bool`). A missing requested token fails with `DtcgError::Projection`, except for `Option<T>` fields (also spelled `core::`/`std::option::Option`), which become `None`; unrequested tokens are ignored but still serve as alias targets. Attributes are parsed strictly: unknown, duplicated or misplaced keys and conflicting combinations (`nested`+`with`, `path`+`rename`, `rest` with anything) are spanned compile errors pinned by `trybuild` snapshots in `crates/dxn_dtcg_derive/tests/ui`. Generated code uses fully qualified paths and unraws field names; generic bounds are added per field (`FromDtcgValue` for leaves, `DtcgTokens` for nested, `Default` for rest).

`dxn_dtcg::values` models every DTCG type (plus the extension `Blur` and dimension `LineHeight`) with the spec's consumer rules (alpha default 1, clamped gradient positions, font-weight keyword numbers, single-layer shadows); each implements `FromDtcgValue`, and `ResolvedToken::typed(&ValueExtensions)` converts by effective type. `f32` projection rejects numbers outside the finite `f32` range. `ResolvedTokens::to_document` exports a resolution as a DTCG document that re-resolves to the same tokens. Each model has `to_json`, and the twelve DTCG-type models implement `Serialize`/`Deserialize` through their DTCG JSON form (deserializing validates the type with `ValueExtensions::all()`).

`values::Color` (math in `src/color.rs`, no feature) converts between the 14 DTCG color spaces with CSS Color 4 formulas (XYZ D65 hub, Bradford for D50 spaces, BT.1886 pure 2.4 power for Rec. 2020) and offers `mix`/`mix_in` (premultiplied alpha, shorter hue arc, powerless hues take the other color's), OKLCH `lighten`/`darken`, `with_alpha`, sRGB hex, WCAG luminance/contrast, and `in_gamut`/`clamp_to_gamut` in linear light (sRGB gamut for HSL/HWB; HSL/HWB conversions clip sRGB first; `contrast` uses the colors clipped into sRGB; a converted hue is zeroed only at rounding-noise chroma, while mixing keeps colorjs's powerless thresholds). Results are always valid DTCG colors; references come from colorjs.io 0.7.1 within 1e-5 and in-gamut round trips hold within 1e-9 except components near 0 in A98 RGB/Rec. 2020 (`NOISE` = 1e-14 snapping).

`#[expression_module(specifier = "…")]` (`dxn_dtcg_derive/src/expression_module.rs`, re-exported from `dxn_dtcg::expressions`) turns an inherent `impl` block's `pub fn`s into a `NativeModule` with camelCase exports, TSDoc from doc comments and TypeScript types mapped by spelling; receivers, `async`, generics, trait impls, unknown keys and invalid or reserved export names are spanned compile errors.

`validate_value(type, value)` checks resolved values strictly against the 13 DTCG 2025.10 types; `validate_value_with(type, value, &ValueExtensions)` also accepts the opt-in value extension shapes: the extension type `dxn.dtcg.blur` (`BLUR_TYPE`: `{ "kind": "layer" | "background", "radius": <dimension> }`) and a `dimension` typography `lineHeight`.

## Backlinks

- [Projection trait](../../../../../crates/dxn_dtcg/src/dtcg_tokens.rs)
- [Value conversion](../../../../../crates/dxn_dtcg/src/from_dtcg_value.rs)
- [Value validation](../../../../../crates/dxn_dtcg/src/value.rs)
- [Typed values](../../../../../crates/dxn_dtcg/src/values.rs)
- [Derive macros](../../../../../crates/dxn_dtcg_derive/src/lib.rs)
- [Value extension shapes](../../../../../crates/dxn_dtcg/tests/value_extension_types.rs)
- 2026-07-26 current [narrow typed projection decision](../../../decisions/runtime/projection/2026-07-26-runtime-projection-narrow-typed-projection.md)
