# Changelog

All notable changes to `dxn_dtcg` and `dxn_dtcg_derive` are documented in this file. Both
crates share one version and one release tag `vX.Y.Z`. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crates follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). It is generated from
Conventional Commits by release-plz.

## [1.3.0] - 2026-10-31

### Build and CI
- test on macOS arm64 too
- run the whole test suite on every platform, wasm32 included

### Documentation
- *(expressions)* document token expressions in the guide, reference and website

### Features
- *(expressions)* evaluate JavaScript and TypeScript token expressions
- *(expressions)* support wasm32-unknown-unknown and test it in CI
- *(expressions)* import host and document JavaScript and TypeScript modules
- *(values)* convert and compose colors, serialize every value model
- *(expressions)* call Rust functions from expressions through native modules

### Fixes
- *(expressions)* keep stack overflows uncatchable and module values frozen
- *(values)* clip HSL, HWB and contrast into sRGB and keep near-gray hues
- *(expressions)* keep stack overflows uncatchable in async code and closing iterators
- *(expressions)* unwrap wrapped stack overflows however deeply nested
- *(expressions)* keep the stack margin small and give session setup its own limit
- *(expressions)* give the engine's overflow check room on large-frame builds

### Tests
- *(expressions)* cover modules, native calls and colors on wasm32, and the module document limit
- *(expressions)* show that a configured stack limit applies on every platform

## [1.2.0] - 2026-10-25

### Build and CI
- *(toolchain)* require Rust 1.97

### Documentation
- *(website)* add the documentation site and GitHub Pages workflow

### Features
- *(extensions)* move library extensions to dxn.dtcg.* and drop the project envelope
  - Migration: token documents: set `"$type": "dxn.dtcg.blur"` on blur tokens
  - Migration: Rust API: use `ValueExtensions::all()`, the `blur` field and `with_blur`; unwrap project files yourself, apply `LocalReferencePolicy::check_resolver` if needed and pass the resolver to `from_json_with_options`; drop `InvalidEnvelope` and `Envelope` match arms

## [1.1.0] - 2026-10-23

### Features
- *(repository)* rename the crates to dxn_dtcg and dxn_dtcg_derive
  - Migration: Rust API: depend on `dxn_dtcg = { git = "https://github.com/dXnSix/dXn.Dtcg", tag = "v1.1.0" }` instead of `dtcg_runtime`, and replace `dtcg_runtime::` and `dtcg_runtime_derive::` paths with `dxn_dtcg::` and `dxn_dtcg_derive::`.

## [1.0.0] - 2026-10-18

The runtime now conforms fully to the DTCG 2025.10 Format, Color and Resolver modules: all 250
fixtures of the official conformance corpus pass. See [docs/12-conformance.md](docs/12-conformance.md)
for the conformance tiers and every implementation-defined choice.

### Added

- **Official conformance corpus.** The DTCG 2025.10 test suite (`design-tokens/community-group` at
  `882ebd6`) is vendored under `crates/dtcg_runtime/tests/fixtures/dtcg-test-suite/2025.10/` and
  run by `tests/official_test_suite.rs` (`official corpus: 250 passed, 0 known gaps`). It is
  excluded from the published crate.
- **Options.** `RuntimeOptions { limits, extensions }`, `ValueExtensions` (`line_height_dimension`,
  the blur-type flag and a constructor that turns both on), `DtcgRuntime::with_options`,
  `DtcgRuntime::from_json_with_options`, `DtcgRuntime::options()` and `validate_value_with`.
- **Limits.** `RuntimeLimits::max_value_nodes` (default 4,000,000) and `RuntimeLimits::max_tokens`
  (default 100,000), the setters `with_max_documents`, `with_max_reference_depth`,
  `with_max_alias_depth`, `with_max_value_nodes`, `with_max_tokens`, and `DtcgRuntime::limits()`.
  Resolved values nest at most 128 levels (`kind: "value depth"`), flattened paths at most 128
  segments (`kind: "path depth"`).
- **Metadata.** `ResolvedToken::description` and `ResolvedToken::deprecated` (new `Deprecation`
  enum, inherited from groups), `ResolvedGroup`, `ResolvedTokens::group` and
  `ResolvedTokens::groups`.
- **Typed values.** The `values` module (`Color`, `Dimension`, `Duration`, `FontFamily`,
  `FontWeight`, `CubicBezier`, `StrokeStyle`, `Border`, `Transition`, `Shadow`, `Gradient`,
  `Typography`, `Blur`, `LineHeight`, `TypedValue`), `ResolvedToken::typed` and
  `TypedValue::from_resolved`. Every model implements `FromDtcgValue`.
- **Export.** `ResolvedTokens::to_document` writes a resolution as a DTCG 2025.10 token document.
- **Inputs.** `selection_from_json` and `selection_refs`; `DtcgError::InvalidInput`.
- **Errors.** `DtcgError::UnresolvedReference { resource, path, target, message }` and the
  diagnostic codes `UnresolvedReference` and `InvalidInput`.
- **API helpers.** `Resolution::{map, is_ready, is_missing, missing}`,
  `SourceOccurrence::resource()` and a `Debug` impl for
  `DtcgRuntime` that never prints document bodies. `DtcgError` is `Clone` and `PartialEq`.
- **Incremental resolution.** `AffectedResolution::removed()` and `AffectedResolution::order()`.
- **Derives.** `Option<T>` leaf fields (`Option`, `core::option::Option`,
  `std::option::Option`, with or without `with`) project to `None` when the token is missing.
- **Documentation.** The README and every `docs/` chapter are compiled and run as doctests.

### Changed

- **Strict values by default.** `validate_value` and resolution accept only DTCG 2025.10 values:
  typography `lineHeight` must be a number (or alias a `number` token) and the blur extension type
  is unsupported, unless `ValueExtensions` enables them.
- **One merged token structure.** All active sources fold into one structure before resolution:
  groups merge member by member, group `$type`, `$description`, `$deprecated` and `$extends`
  merge per key, group `$extensions` merge recursively, and tokens are still replaced whole.
  Type inheritance, `$extends`, `$deprecated` inheritance and JSON Pointers are evaluated on it,
  so an override inherits its group's `$type` from an earlier source and `$extends` reaches other
  sources and groups produced by `$extends`. A path that is a token in one source and a group in
  another is an error.
- **Type determination** follows Format §5.2.2: own `$type`, then the type of the token a
  whole-value reference targets, then the group's `$type`. An own `$type` that differs from the
  referenced token's type is a `reference type mismatch`.
- **Reference positions.** Curly-brace aliases are allowed only as the whole value, at composite
  members, and as shadow, gradient and `dashArray` elements, and there they must target the
  member's type (Format §7, §9). Aliases inside color, dimension, duration, `cubicBezier` or
  `fontFamily` arrays are rejected.
- **Token document structure.** A string root `$schema` is accepted and ignored. Tokens with child
  members, `$root` or `$extends`, tokens with both `$value` and `$ref`, non-token `$root`
  members, empty names, and mistyped `$description`, `$deprecated` or `$extensions` are rejected.
- **Resolver documents.** Unknown members (including root `$extensions` and `x-*` members), an
  empty `resolutionOrder` and modifiers with a single context are rejected (Resolver §4).
  `$defs` and `$schema` are allowed, and `source_references()` now fails with `InvalidDocument` for any resolver it reaches that breaks
  them, as `resolve` does.
  `modifier_contexts` rejects a single context.
- **`resolutionOrder` steps.** A `$ref` step's sibling overrides define the modifier for every
  API (`resolve`, `validate_complete`, `modifiers()`, `normalize_selection`, inspection). Two
  steps defining the same modifier, and inline modifiers without `name`, are errors everywhere.
  A token document with a `modifiers` group has no axes.
- **Inputs.** Modifier and context names match case-insensitively. An input is valid when any
  reached resolver declares it (nested resolvers, and the supplied source for `preview_source` and
  `inspect_source`); each resolver applies its own inputs and defaults; a graph without modifiers
  ignores inputs. `normalize_selection` validates like `resolve` across the whole graph and lists
  every reached modifier, root first, in declared spelling.
- **Selection errors of `resolve`.** An unknown context (was `DtcgError::Token`), a missing input
  and a modifier selected twice (were `DtcgError::InvalidDocument`) are now
  `DtcgError::UnknownContext`, as `normalize_selection` already reported them.
  `UnknownModifier::known` lists names in declaration order.
- **Missing alias and `$extends` targets** are `DtcgError::UnresolvedReference` (was
  `DtcgError::Projection`); `Projection` is used only by projection.
- **`max_alias_depth`** counts reference hops (aliases, token pointers and document pointers)
  independently of authored order; a limit of 0 still resolves literal tokens.
- **`max_documents`** also counts every discovery expansion step, including expansions that
  contribute no source.
- **JSON Pointers** follow RFC 6901: percent-encoding is decoded and array indexes are strict.
- **Resource ids** reject `#`, control characters, `< > " | ? *`, segments ending in a dot or
  space, and Windows device names.
- **Incremental resolution** compares the flattened tokens before and after an edit; deleting an
  aliased token, group edits and document-pointer edits are handled, and
  `ResolvedTokens::apply_affected` yields exactly the full resolution.
- **Fingerprints** hash values with sorted object keys; equality ignores object member order.
- **`source_references()`** walks reference objects that target resolvers through their override
  members and lists each authored site once however many reference objects reach it. Its work
  is linear in the documents' size: the first walk of every location is free, targets without
  source sites (token documents) are not walked, and walking a resolver or set again under
  different overrides counts toward `max_documents` in discovery's units (failing with
  `LimitExceeded { kind: "documents" }`), so every document `validate_complete` accepts is listed
  unless the repeated walks pass through dormant definitions.
- **`#[non_exhaustive]`** on `DtcgError`, `ValueValidationError`, `DtcgDiagnosticCode`,
  `RuntimeLimits`, `ResolvedToken`, `ModifierAxis`, `DocumentRequest`, `ResolvedGroup`,
  `RuntimeOptions`, `ValueExtensions` and `TypedValue`.
- **Derives.** Attributes are parsed strictly: unknown, duplicated or misplaced keys, `id` on
  `DtcgTokens`, `nested` with `with`, `path` with `rename`, and `rest` with any other key are
  compile errors. Type parameters are bounded per field (`FromDtcgValue` for leaves, `DtcgTokens`
  for nested fields) instead of all getting a `DtcgTokens` bound.

### Fixed

- Hostile documents can no longer overflow the stack or run for exponential time: document-pointer
  chains, pointer and `$extends` fan-out, long `$extends` chains and discovery fan-out are bounded.
- Pointer cycle detection no longer reports false cycles across sources or for sibling overrides
  that point at their own target.
- `ResourceId::parse` and `set_root` no longer silently drop a fragment.
- `$extends` and type determination are evaluated per merged structure, so cross-source extensions
  and inherited types work.
- `normalize_selection`, `modifiers()` and input validation honor `$ref` step overrides.
- Affected resolution no longer accepts a deletion that breaks an alias, and snapshots no longer go
  stale after group edits or moves.
- Generated derive code uses fully qualified paths (a local `Result` alias no longer breaks it),
  and raw identifiers such as `r#type` read the token `type`.
- `f32` projection fails for numbers outside the `f32` range instead of producing infinity.
- The `InvalidResourceId` and `InvalidPointer` messages close their backticks; the `ModifierAxis`
  and `diagnostics` module docs are accurate.

### Migration

- **Value extensions.** Documents with a dimension typography `lineHeight` (a value or an alias to
  a `dimension` token) or blur-typed tokens now fail with strict default options. Create the
  runtime with the matching `ValueExtensions` through `DtcgRuntime::with_options` or
  `from_json_with_options`.
- **`ModifierAxis` literals.** `ModifierAxis` is `#[non_exhaustive]`, so code outside the crate can
  no longer build it with a struct literal; compare its fields instead, for example
  `(axis.name.as_str(), axis.contexts.clone(), axis.default.clone())`.
- **Error mapping.** Missing alias targets now arrive as
  `DtcgError::UnresolvedReference { resource, path, target, message }` (was `Projection`): map it
  explicitly, and `DtcgError::InvalidInput` if JSON inputs are used. `path` is the referring token,
  `target` the alias as written. `DtcgError::Projection` values built by callers stay valid.
- **Selection errors.** `resolve` reports unknown contexts, missing inputs and duplicated inputs as
  `DtcgError::UnknownContext` (was `Token` or `InvalidDocument`); update any mapping or test that
  matched the old variants.
- **Stricter structure.** A token with any non-`$` member is a token with children and fails; keep
  such data in the token's `$extensions`. Unknown members at the resolver root, in sets, modifiers
  and inline steps, root `$extensions`, an empty `resolutionOrder` and single-context modifiers are
  rejected (reference objects keep extra members). Move tool data into `$defs` (root) or
  `$extensions` (sets, modifiers, inline steps), write at least one step (for example an empty
  inline set `{"type": "set", "name": "tokens", "sources": []}`), and give each single-context
  modifier a second context or turn it into a set.
- **Incremental resolution.** `apply_affected` now also removes deleted paths and adopts the new
  order; remove any manual reconciliation that compensated for stale snapshots.
- **Fingerprints.** `ResolvedTokens::fingerprint()` values change once (canonical key order);
  invalidate any persisted fingerprint caches.
- **`RuntimeLimits`.** Build limits with `RuntimeLimits::default().with_*` setters; struct literals
  no longer compile, and the fields stay public for reading.
