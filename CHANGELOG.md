# Changelog

All notable changes to `dtcg_runtime` and `dtcg_runtime_derive` are documented in this file. Both
crates share one version and one release tag `vX.Y.Z`. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crates follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). It is generated from
Conventional Commits by release-plz.

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
  `rayx_blur`, `ValueExtensions::rayx()`), `DtcgRuntime::with_options`,
  `DtcgRuntime::from_json_with_options`, `DtcgRuntime::from_envelope_with_options`,
  `DtcgRuntime::options()` and `validate_value_with`.
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
  `SourceOccurrence::resource()`, `DtcgRuntime::replace_envelope` and a `Debug` impl for
  `DtcgRuntime` that never prints document bodies. `DtcgError` is `Clone` and `PartialEq`.
- **Incremental resolution.** `AffectedResolution::removed()` and `AffectedResolution::order()`.
- **Derives.** `Option<T>` leaf fields (`Option`, `core::option::Option`,
  `std::option::Option`, with or without `with`) project to `None` when the token is missing.
- **Documentation.** The README and every `docs/` chapter are compiled and run as doctests.

### Changed

- **Strict values by default.** `validate_value` and resolution accept only DTCG 2025.10 values:
  typography `lineHeight` must be a number (or alias a `number` token) and `dev.rayx.blur` is an
  unsupported type, unless `ValueExtensions` enables them.
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
  `$defs` and `$schema` are allowed. `.tokeneditor` envelopes apply the same rules to `data`, and
  `source_references()` now fails with `InvalidDocument` for any resolver it reaches that breaks
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

#### RayX

- **Enable the RayX value extensions.** `rayx_theme/src/project.rs` opens projects with
  `DtcgRuntime::from_envelope`; use
  `DtcgRuntime::from_envelope_with_options(path, envelope, RuntimeOptions::default().with_extensions(ValueExtensions::rayx()))`.
  Without it the shipped design system (`rayx_components_core/assets/dtcg/rayx/*.tokens.json`)
  fails: 33 typography tokens alias dimension `lineHeight` tokens (first error:
  ``token `theme.styles.text.typography.h1`: reference type mismatch: expected number, found dimension``)
  and 16 tokens use `dev.rayx.blur`. Do the same in the Test Suite helpers
  (`rayx_theme/tests/support/mod.rs`, `rayx_theme/tests/dtcg_theme_envelope.rs`,
  `rayx_theme/tests/theme_styles.rs`) and wherever the Test Suite opens
  `apps/test_suite/assets/dtcg/scopes/second.tokeneditor`, which layers the same RayX style tokens.
  Code generated by `xtask theme` (`#[dtcg(with = …)]`, `path`, `nested`) stays valid.
- **`ModifierAxis` literals.** `rayx_theme/tests/dtcg_theme_envelope.rs`
  (`modifiers_are_enumerated_in_resolution_order_with_defaults`) builds `ModifierAxis { .. }`
  values, which `#[non_exhaustive]` forbids outside the crate. Compare the fields instead, for
  example `(axis.name.as_str(), axis.contexts.clone(), axis.default.clone())`.
- **Error mapping.** `rayx_theme/src/error.rs` converts `DtcgError` with a wildcard arm, so it still
  compiles; add an arm for `DtcgError::UnresolvedReference` (missing alias targets, which used to
  arrive as `Projection`) so they keep a token-shaped `ThemeError`, and for
  `DtcgError::InvalidInput` if JSON inputs are used. `rayx_theme/src/scale.rs` and
  `rayx_theme/src/value.rs` construct `DtcgError::Projection` themselves, which stays valid.
- **Fixtures.** These `rayx_theme/tests/fixtures` files break the stricter structure rules:
  - `complete.tokeneditor`: the resolver-root member `x-resolver` (move it under `$defs`), and
    through it `tokens/base.tokens.json`, whose token `color.primary` has an `unknown` member (a
    token with a non-`$` member is a token with children; move such data into `$extensions`);
  - `tokens/editable.tokens.json`: the token `group.first` has an `x-unknown` member.
  `unsupported-version.tokeneditor` and the absolute-reference envelope in
  `tests/dtcg_theme_envelope.rs` also have an empty `resolutionOrder`, but they keep failing for
  their intended reason first (version, local-reference policy).
- **Android assets.** `apps/lab/platform/android/gradle/app/src/main/assets/assets/dtcg/default.resolver.dtcg.json`
  (copied into build outputs, including the Test Suite's) declares the single-context modifiers
  `contrast`, `density` and `platform`; if it is loaded with `dtcg_runtime`, give each modifier a
  second context or turn it into a set.
- **Test Suite DTCG page.** `apps/test_suite/src/pages/dtcg_runtime.rs` shows the diagnostic of the
  invalid-alias fixture; its code is now `UnresolvedReference` (was `Projection`), with
  `path=button.fill` and the target `missing.token` in the message. The spec
  `tools-node/test_suite/rayx-dtcg-runtime.spec.ts` only checks the "Structured diagnostic"
  prefix and needs no change.
- **Fingerprints.** `ResolvedTokens::fingerprint()` values change once (canonical key order);
  invalidate any persisted fingerprint caches.
- **`RuntimeLimits`.** Build limits with `RuntimeLimits::default().with_*` setters; struct literals
  no longer compile.

#### RayX.TokenEditor

- **Enable the RayX value extensions** where projects are resolved:
  `services/tokens/resolution/project_resolution.rs` and `services/storage/save_as.rs` call
  `DtcgRuntime::from_json`; use `DtcgRuntime::from_json_with_options(.., RuntimeOptions::default().with_extensions(ValueExtensions::rayx()))`.
- **Diagnostics.** `services/errors/diagnostic.rs` maps `DtcgError::Projection` to a location and
  message; missing alias targets now arrive as `DtcgError::UnresolvedReference { path, target,
  message, .. }`. Map it explicitly. The contract test `group_name_alias_does_not_implicitly_target_root`
  (`tests/contracts/dtcg_2025_10.rs`) expects location `palette`: either use `target` as the
  location, or update the test to the referring token `semantic.accent` (the message stays
  `missing alias target`). `error_resource` in `project_resolution.rs` can add
  `UnresolvedReference { resource, .. }`.
- **Selection errors.** `resolve` now reports unknown contexts, missing inputs and duplicated
  inputs as `DtcgError::UnknownContext` (was `Token` or `InvalidDocument`); update any mapping or
  test that matched the old variants.
- **New projects.** `TokenProject::empty` (`services/tokens/project/token_project.rs`) writes
  `"resolutionOrder": []`, which is now rejected (Resolver §4 requires at least one step), so
  creating a project fails in `validate_complete`. Write at least one step, for example an empty
  inline set `{"type": "set", "name": "tokens", "sources": []}`.
- **Unknown members on tokens.** The editor preserves unknown token members such as `x-preserve`;
  a token with any non-`$` member is now "a token with children" and fails. Keep such data in the
  token's `$extensions` instead. Affected:
  - fixtures `tests/fixtures/all-types.tokeneditor` (`x-preserve`, `x-before`, `x-after`),
    `tests/fixtures/table-identity.tokeneditor` (`x-token`), `tests/fixtures/tokens/base.tokens.json`
    (`unknown`) and `tests/fixtures/tokens/editable.tokens.json` (`x-unknown`);
  - inline documents in `tests/scenarios/editor_interactions.rs` (`x-unknown`, `x-keep`),
    `tests/scenarios/typed_token.rs` (`x-before`, `x-after`), `tests/scenarios/history_properties.rs`
    (`x-token`, `x-preserved`, `x-unknown`) and
    `tests/services/tokens/store/token_storage/history_replay.rs` (`x-unknown`).
- **Resolver members, empty `resolutionOrder`, single contexts.** Unknown members at the resolver
  root, in sets, modifiers and inline steps, root `$extensions`, an empty `resolutionOrder` and
  single-context modifiers are rejected (reference objects keep extra members, so `x-occurrence`
  and `x-order` on `$ref` steps stay valid). Move editor data into `$defs` (root) or
  `$extensions` (sets, modifiers, inline steps). Affected:
  - `tests/fixtures/complete.tokeneditor` (`x-resolver`), `workspace.tokeneditor` (`x-resolver`,
    the set member `x-set-member`, the modifier member `x-modifier-member`),
    `dormant-edit.tokeneditor` (`x-source`, root `$extensions`, empty `resolutionOrder`, the
    single-context modifier `theme`), `external-edit.tokeneditor` (root `$extensions`, the
    single-context modifier `theme`) and `table-identity.tokeneditor` (root `$extensions`, the
    modifier member `x-modifier`);
  - `tests/features/resolver/resolution_flows.rs` (the inline step member `x-inline`),
    `tests/scenarios/history_properties.rs` (the modifier member `x-modifier`, the root member
    `x-project`, the single-context modifier `contrast`), `tests/scenarios/editor_interactions.rs`
    (empty `resolutionOrder` around lines 524, 588, 754 and 2053; `x-resolver`/`x-source` at
    line 588) and `tests/scenarios/typed_token.rs` (empty `resolutionOrder` at line 457).
  `unsupported-version.tokeneditor` also has an empty `resolutionOrder` but keeps failing on its
  version first.
- **Incremental resolution.** `apply_affected` now also removes deleted paths and adopts the new
  order; remove any manual reconciliation that compensated for stale snapshots.
- **`RuntimeLimits`.** `RuntimeLimits::default().max_documents` still reads the limit; the field
  stays public.
