---
title: Architecture and contributing
description: Source layout, the internal pipeline, invariants a change must keep, the test groups, documentation doctests and CI for contributors to dxn_dtcg.
group: Project
order: 200
---

This page is for people changing the crates: how the source is organized, how the pipeline is implemented, the invariants to keep and how to test.

## Repository layout

```text
.
├── Cargo.toml                    workspace members, shared package metadata, dependency versions
├── rust-toolchain.toml           pinned toolchain (1.97.0) with clippy, rustfmt and the wasm32 target
├── crates/
│   ├── dxn_dtcg/
│   │   ├── src/                  the runtime (see the module map)
│   │   ├── tests/                one integration-test group per feature
│   │   │   ├── fixtures/dtcg-test-suite/   the vendored official 2025.10 corpus (not packaged)
│   │   │   ├── fixtures/expressions/       the ES module library fixture of the module tests
│   │   │   └── expression_module_ui/       trybuild cases of #[expression_module]
│   │   └── benches/resolver.rs   timing baseline, written as a #[test]
│   └── dxn_dtcg_derive/
│       ├── src/lib.rs            the macro entry points: the two derives and #[expression_module]
│       ├── src/expression_module.rs  the #[expression_module] attribute macro
│       └── tests/                derive shape, projection and hardening tests
│           └── ui/               trybuild compile-fail cases with their .stderr snapshots
├── docs/                         the Markdown documentation (compiled as doctests)
├── .github/workflows/            ci.yml, release-plz.yml
├── release-plz.toml              release-plz configuration and the changelog template
├── .agentx/                      repository memory: decisions and domain knowledge
└── CHANGELOG.md                  written by release-plz
```

## Module map

All modules of `dxn_dtcg` are private except `values` and, with its feature, `expressions`; `lib.rs` re-exports the public items.

| Module | Responsibility |
|---|---|
| `runtime.rs` | `DtcgRuntime`: catalog, root, options and registry; the public API |
| `options.rs` | `RuntimeOptions`, `ValueExtensions` |
| `catalog.rs` | `DocumentCatalog`: id to `Arc<str>` map with a `DocumentKind` per entry, JSON well-formedness check, kind checks, atomic batches, revision |
| `resource.rs` | `ResourceId`, `DocumentReference`: portable-id rules, normalization, relative resolution |
| `json_pointer.rs` | The single RFC 6901 parser for fragments and value-level and token-level `$ref`s |
| `resolver_definition.rs` | The single owner of resolver structure: allowed members, step rules, effective definitions; `set_sources`, `modifier_contexts` |
| `selection.rs` | `ModifierAxis`, case-insensitive name matching, `selection_from_json`, `selection_refs` |
| `source_discovery.rs` | Expands resolvers into ordered terminal `SourceOccurrence`s, picks contexts, collects missing documents, enforces document and depth limits, detects cycles |
| `source_occurrence.rs`, `source_origin.rs`, `source_reference.rs`, `source_references.rs` | Occurrences with origins, and the walk over authored reference sites |
| `flatten.rs` | Folds sources into one merged structure, validates structure, expands `$extends`, flattens to dotted paths with inherited type and deprecation, fixes token order |
| `token_resolution.rs` | Bounded resolution of aliases and pointers, type determination, value validation, the dependency collector |
| `reference_positions.rs` | Where a curly-brace alias may appear in a value and which type it must target |
| `resolver.rs` | Result types (`Resolution`, `DocumentRequest`, `DocumentKind`, `ResolvedToken(s)`, `ResolvedGroup`, `Deprecation`, `RuntimeLimits`), the `resolve`, `validate_complete` and `resolve_affected` drivers, fingerprints, `to_document`, `apply_affected` |
| `value.rs`, `values.rs` | `validate_value(_with)`, `BLUR_TYPE`; the public typed models, `TypedValue`, `ResolvedToken::typed`, DTCG JSON forms and serde for every model |
| `color.rs` | The color API on `values::Color`: CSS Color 4 conversions between the 14 spaces, mixing, lightness, hex, contrast, gamut (no feature) |
| `expression_extension.rs` | `EXPRESSION_EXTENSION`, present without the feature |
| `expressions/mod.rs` | Feature `expressions`: `ExpressionOptions`, the public re-exports, and the QuickJS clock symbol `__rquickjs_host_now_us` on `wasm32-unknown-unknown` |
| `expressions/format.rs`, `compile.rs`, `nesting.rs` | Validation of the token expression object; oxc (syntax and early errors, rejecting `import()`, the static import scan, TypeScript stripping with a source map); the nesting bound checked before oxc's recursive parser sees a source |
| `expressions/engine.rs`, `sandbox.rs` | The per-resolution QuickJS `Session` (wrapper modules, module table and loader, ES evaluation order, the native bridge, JSON conversion, error positions); the counting allocator, the step budget's interrupt handler and the hardening script |
| `expressions/modules.rs`, `native.rs`, `builtins.rs`, `declarations.rs` | `ExpressionModule`, document module declarations and the request walk over import graphs; `NativeModule` and its traits; the `dxn.dtcg` and `dxn.dtcg/color` modules; the `.d.ts` renderer |
| `dtcg_tokens.rs`, `dtcg_component_tokens.rs`, `from_dtcg_value.rs`, `type_registry.rs` | Projection traits and built-in conversions |
| `local_reference.rs`, `affected_resolution.rs`, `error.rs`, `diagnostics/mod.rs` | `LocalReferencePolicy`, `AffectedResolution`, `DtcgError`, `DtcgDiagnostic` (feature `diagnostics`) |

`dxn_dtcg_derive/src/lib.rs` parses `#[dtcg(...)]` strictly with `syn` and emits a `DtcgTokens` implementation (and `DtcgComponentTokens`); `expression_module.rs` turns an `impl` block's public functions into `NativeModule` registrations with mapped TypeScript signatures. The generated code refers to `::dxn_dtcg::...` and `::core::...` paths only, which is why applications use the macros through `dxn_dtcg`.

## Pipeline internals

A resolution runs four stages: **discover** the active sources, **fold** them into one merged structure, **flatten** it into tokens, and **resolve** each token within the limits.

1. **Discovery** (`source_discovery.rs`). `load` fetches a document or records a `DocumentRequest` in a `BTreeMap` (sorted, de-duplicated). `expand` runs an explicit work queue; resolvers are validated by `resolver_definition.rs` and expanded with the ancestor chain tracked for cycles. `choose_contexts` matches the selection against declared modifiers and applies defaults. After discovery, `check_inputs` rejects any input no reached resolver claimed.
2. **Folding and flattening** (`flatten.rs`). `Merger::fold` merges every terminal occurrence in discovery order, validating structure as it goes. `Flattener::flatten_group` walks the merged tree once with the inherited type and deprecation; `$extends` expansions are resolved iteratively in dependency order and memoized. Each raw token keeps an `Arc` of the merged document, so every JSON Pointer reads the merged structure.
3. **Resolution** (`token_resolution.rs`). Each path resolves in order with memoization. One hop counter covers aliases, token pointers and document pointers; memoized results carry their hop count so the outcome does not depend on authored order. Every copy is charged against `max_value_nodes` first. The effective type is the own `$type`, else the whole-value reference's target type, else the group's. With expressions enabled, the token's inputs then resolve through the same code (same limits, same cycle detection), the expression runs in the resolution's lazily started `Session`, and the result is converted, bounded and validated against the same type, with the resolved `$value` kept as the snapshot. Before resolving, `resolver.rs` walks the document modules expressions import and returns `Missing` for those the catalog lacks.
4. **Fingerprint and affected resolution** (`resolver.rs`). The fingerprint is SHA-256 over `(path, type, value, source)` in order. `replace_json_and_resolve_affected` flattens before and after the edit, seeds the affected set, walks reverse dependencies (with expressions enabled, through expression inputs too) and resolves only those; the previous document is restored unless the result is `Ready`.

## Invariants

These come from the design decisions in `.agentx/memory/decisions/`. Changes must keep them.

- **I/O-free and UI-neutral.** No dependency on any UI toolkit, and no file system, network or async runtime. Loading belongs to the caller through `Resolution::Missing`.
- **Strict by default.** Every value shape beyond DTCG 2025.10 is a `ValueExtensions` flag, off by default, with a pinned test group.
- **One merged structure; resolve after merging.** Types, `$extends`, deprecation, pointers and aliases are evaluated only on the complete merge of the active selection. Tokens are replaced whole; composite values are never merged.
- **One owner per rule.** Resolver structure lives in `resolver_definition.rs`, pointer syntax in `json_pointer.rs`, alias positions in `reference_positions.rs`, so every API agrees.
- **Bounded work.** Every loop and recursion is bounded by a `RuntimeLimits` field or a fixed bound; new code that follows references or copies values must charge the existing budgets. Expression evaluation is bounded by `ExpressionOptions`, enforced in Rust so that expression code cannot catch it.
- **Expressions are off twice.** The `expressions` feature compiles the engine; only `RuntimeOptions::with_expressions` evaluates. Without both, the extension stays opaque metadata and the default build compiles no C.
- **Atomic mutations.** Invalid input leaves the catalog unchanged.
- **Order is semantic.** `serde_json`'s `preserve_order` feature must stay enabled.
- **One path for previews.** Previews use the same discovery and resolution as `resolve`; never build a synthetic resolver or mutate the catalog to preview.
- **Redacted diagnostics.** Diagnostics and `Debug` never retain document text or JSON values.
- **Narrow projection.** Structs request only their fields. Consumer-owned types convert through `#[dtcg(with = "...")]` in the consumer; do not add consumer-specific `FromDtcgValue` implementations here.
- **Conformance ratchet.** The official corpus must keep passing; a deliberate deviation is listed in `KNOWN_GAPS` and in [Conformance](./conformance.md), never silently.

## Development workflow

The toolchain is pinned in `rust-toolchain.toml`; `rustup` installs Rust 1.97.0 with Clippy, rustfmt and the `wasm32-unknown-unknown` target. While working, run only the group you touch:

```bash
cargo test -p dxn_dtcg --all-features --test lazy_catalog
```

Before committing, run the full validation:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo package --workspace
cargo check --workspace --target wasm32-unknown-unknown   # for portability-sensitive changes
cargo test -p dxn_dtcg --bench resolver -- --nocapture    # timing baseline
```

`--all-features` includes `expressions`, so a native C compiler is needed to build QuickJS; the `wasm32` check uses the default features and needs none.

Commits follow [Conventional Commits](https://www.conventionalcommits.org/) with meaningful scopes, for example `fix(resolver): ...`. Every new feature or behavior change gets its own integration-test group in `crates/<crate>/tests/<feature>.rs`.

### Documentation is tested

The README and every chapter in `docs/` are compiled as doctests: a private `documentation` module in `crates/dxn_dtcg/src/lib.rs` includes each file under `#[cfg(doctest)]`. Every code block must carry a language tag: `rust` (compiled and run), `rust,no_run` (compiled), `rust,ignore` (only for signature listings) or a non-Rust tag. The expressions chapter is included only with the `expressions` feature, so default doctests stay free of the engine. Check one chapter at a time:

```bash
cargo test -p dxn_dtcg --all-features --doc documentation::resolvers_and_modifiers
```

## Test groups

Each feature has its own integration-test file, selectable with `--test <name>`.

| Group | Covers |
|---|---|
| `official_test_suite` | The official corpus (250 fixtures, `KNOWN_GAPS` ratchet), line endings, `$schema` |
| `vertical_slice`, `api_foundations` | End to end (missing documents, resolve, project); limit setters, redacted `Debug`, comparable errors, `Resolution` helpers |
| `dtcg_2025_10_conformance`, `format_structure`, `type_determination`, `reference_positions`, `merged_resolution`, `extensions_merge`, `token_metadata` | Format rules, types, alias positions, merging, metadata |
| `document_export`, `typed_values`, `value_extensions`, `value_extension_types` | `to_document`, the `values` models, strict defaults and extensions |
| `resolver_2025_10`, `resolver_document_validation`, `resolution_step_consistency`, `input_selection`, `alias_overlay` | Resolvers, inputs and aliases against the final set |
| `bounded_expansion`, `bounded_references`, `lazy_catalog`, `resource_id_*`, `local_reference_policy` | Limits, hostile input, the missing-document protocol, id normalization, `LocalReferencePolicy` |
| `incremental_resolution`, `incremental_consistency` | Affected resolution, rollback, that every patched snapshot equals a full resolution |
| `source_inspection`, `source_reference_bounds`, `source_reference_parity` | Inspection, previews, reference sites and their bounds |
| `derive_projection`, `diagnostics` | Derives through the runtime crate; feature gating and redaction |
| `color_conversion` | The color API on `values::Color` against CSS Color 4 reference values, round trips, operations |
| `value_serde` | Serde for every value model through its DTCG JSON form, and rejection of invalid JSON |
| `expressions_disabled` | Expression extensions stay opaque without the feature or the option; run it with and without `--features expressions` |
| `expressions_format`, `expressions_evaluation`, `expressions_typescript` | The expression object's validation; snapshots, types, inputs with their limits and cycles, result conversion and rounding; TypeScript stripping and authored error positions |
| `expressions_sandbox` | Removed capabilities, frozen built-ins, uncatchable limits, deterministic step budgets, the nesting bound, determinism |
| `expressions_apis` | Every resolving API, affected resolution, export round trips |
| `expressions_modules`, `expressions_document_modules` | Host modules and ES module semantics; module declarations, the request/provide protocol for modules, catalog kinds, `validate_complete` |
| `expressions_native`, `expressions_native_macro` | `NativeModule`, the bridge's errors and panics; `#[expression_module]` with its `trybuild` cases in `tests/expression_module_ui` |
| `expressions_stack_top`, `expressions_async_overflow` | Stack limits from each evaluation's start; stack overflows uncatchable through every `catch`/`finally` form, modules, the regular expression compiler, async functions and wrapped errors |
| `expressions_module_state`, `expressions_shared_compilation` | Frozen module values, taming, the freezing charge, shared module state, the internal module; shared expression modules named from their text |
| `expressions_diagnostic_names`, `expressions_native_finite` | Redacted diagnostics keep standard error names; NaN and Infinity in native calls are `TypeError`s |
| `color_gamut_contrast` | HSL/HWB clipping into sRGB, contrast on sRGB, near-gray round trips |
| `expressions_builtin`, `expressions_color`, `expressions_declarations` | The `dxn.dtcg` and `dxn.dtcg/color` modules; the `.d.ts` output parses and matches the run-time exports |
| `expressions_wasm` | Expressions on `wasm32-unknown-unknown` under Node ([below](#the-webassembly-group)) |
| `dxn_dtcg_derive`: `derive_projection`, `derive_shapes`, `derive_hardening`, `compile_fail` | Derive behavior and `trybuild` compile-fail cases in `tests/ui` |

The vendored corpus under `tests/fixtures/dtcg-test-suite/2025.10/` is byte-identical to upstream (pinned commit and update steps are in its `README.md`). Never edit fixtures. When a change fixes a known gap, remove it from `KNOWN_GAPS`; when it breaks a fixture on purpose, add it and document the deviation. After an intended change to a derive error message, regenerate the snapshots and review the diff with `TRYBUILD=overwrite cargo test -p dxn_dtcg_derive --test compile_fail`. The `#[expression_module]` cases live in `crates/dxn_dtcg/tests/expression_module_ui/`; regenerate them with `TRYBUILD=overwrite cargo test -p dxn_dtcg --features expressions --test expressions_native_macro`.

Every `expressions_*` group except `expressions_disabled` compiles only with the feature, and `expressions_wasm` only for `wasm32-unknown-unknown`; select a native one with `cargo test -p dxn_dtcg --all-features --test expressions_sandbox`.

### The WebAssembly group

`expressions_wasm.rs` runs on `wasm32-unknown-unknown` under Node, through `wasm-bindgen-test-runner`, with the default 1 MiB stack. It needs Node, an LLVM clang with the WebAssembly target and `llvm-ar` (Apple's clang has no WebAssembly target; Homebrew's `llvm` works), and the runner at the version of `wasm-bindgen` in `Cargo.lock` (0.2.129 at the time of writing):

```bash
cargo install wasm-bindgen-cli --version 0.2.129 --locked
CC_wasm32_unknown_unknown=/opt/homebrew/opt/llvm/bin/clang \
AR_wasm32_unknown_unknown=/opt/homebrew/opt/llvm/bin/llvm-ar \
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
cargo test -p dxn_dtcg --features expressions --target wasm32-unknown-unknown --test expressions_wasm
```

The workspace pins `wasm-bindgen-test`, whose `wasm-bindgen` must match the installed runner.

## CI

`.github/workflows/ci.yml` runs on pull requests and pushes to `main` with the pinned toolchain. Two jobs:

- **`rust`**, on `ubuntu-latest` and `windows-latest`: `cargo fmt --check`, Clippy with `-D warnings`, `cargo test --workspace --all-features` (including the corpus, the documentation doctests and the expressions groups), `cargo check --workspace --target wasm32-unknown-unknown` (default features, so no C compiler) and `cargo package --workspace`. The `--all-features` steps compile QuickJS with GCC on Ubuntu and MSVC on Windows.
- **`wasm-expressions`**, on `ubuntu-latest`: installs `clang` and `llvm`, sets `CC_wasm32_unknown_unknown=clang`, `AR_wasm32_unknown_unknown=llvm-ar` and the `wasm-bindgen-test-runner` as the wasm32 runner, installs `wasm-bindgen-cli` at the locked `wasm-bindgen` version, then runs `cargo check -p dxn_dtcg --features expressions --target wasm32-unknown-unknown` and the `expressions_wasm` group.

The manifests stay packageable: the derive crate's dev-dependency on `dxn_dtcg` is path-only on purpose, which breaks the runtime and derive dependency cycle in a packaged crate, and `dxn_dtcg` excludes `tests/fixtures/dtcg-test-suite/**` from the package.

Next: [Releasing](./releasing.md).
