# 14. Architecture and contributing

This chapter is for people changing the crates: how the source is organized, how the resolution
pipeline is implemented, the invariants changes must keep, and how to test and release.

- [Repository layout](#repository-layout)
- [Module map](#module-map)
- [Pipeline internals](#pipeline-internals)
- [Invariants](#invariants)
- [Development workflow](#development-workflow)
- [Test groups](#test-groups)
- [CI](#ci)
- [Releases](#releases)
- [Consuming releases](#consuming-releases)

## Repository layout

```text
.
├── Cargo.toml                    workspace members, shared package metadata, dependency versions
├── rust-toolchain.toml           pinned toolchain (1.97.0) with clippy, rustfmt and the wasm32 target
├── crates/
│   ├── dxn_dtcg/
│   │   ├── src/                  the runtime (see the module map)
│   │   ├── tests/                one integration-test group per feature
│   │   │   └── fixtures/dtcg-test-suite/   the vendored official 2025.10 corpus (not packaged)
│   │   └── benches/resolver.rs   timing baseline, written as a #[test]
│   └── dxn_dtcg_derive/
│       ├── src/lib.rs            the two derive macros
│       └── tests/                derive shape, projection and hardening tests
│           └── ui/               trybuild compile-fail cases with their .stderr snapshots
├── docs/                         this documentation (compiled as doctests)
│   └── releasing.md              how releases are cut (maintainers; not a doctest chapter)
├── .github/workflows/
│   ├── ci.yml                    CI
│   └── release-plz.yml           release pull requests, tags and GitHub Releases
├── release-plz.toml              release-plz configuration and the changelog template
├── .agentx/                      AgentX repository memory: decisions and domain knowledge
├── AGENTS.md, CLAUDE.md          contributor guidance for coding agents
├── CHANGELOG.md                  written by release-plz
└── LICENSE-APACHE, LICENSE-MIT
```

## Module map

All modules of `dxn_dtcg` are private except `values`; `lib.rs` re-exports the public items.

| Module | Responsibility |
|---|---|
| `runtime.rs` | `DtcgRuntime`: owns the catalog, root, options and registry; the public API and its redacted `Debug` |
| `options.rs` | `RuntimeOptions` and `ValueExtensions` |
| `catalog.rs` | `DocumentCatalog`: id → `Arc<str>` map, JSON well-formedness check, atomic batches, revision |
| `resource.rs` | `ResourceId` and `DocumentReference`: portable-id rules, normalization and relative resolution |
| `json_pointer.rs` | The single RFC 6901 parser for resource fragments, value-level and token-level `$ref`s: percent-decoding, `~` unescaping, array indexes |
| `resolver_definition.rs` | The single owner of resolver structure: allowed members, `resolutionOrder` and context-count rules, the effective definition of every step (with `$ref` overrides), declared modifiers; the public `set_sources` and `modifier_contexts` |
| `selection.rs` | Resolver inputs: `ModifierAxis` listing, case-insensitive name matching, `selection_from_json`, `selection_refs` |
| `source_discovery.rs` | `Discovery`: expands resolvers into ordered terminal `SourceOccurrence`s, chooses each modifier's context, records which inputs were used, collects missing documents, enforces the document and depth limits, detects resolver cycles |
| `source_occurrence.rs`, `source_origin.rs` | `SourceOccurrence` (JSON nodes that carry their origin through `child`, `select` and `overlay`) and `SourceOrigin` |
| `source_reference.rs`, `source_references.rs` | `SourceReference` and the walk over authored reference sites |
| `flatten.rs` | Folds all active sources into one merged structure (per-key group properties, recursive `$extensions`, whole-token replacement, token/group collisions), validates token and group structure, expands `$extends`, flattens to dotted paths with inherited type and deprecation, fixes token order |
| `token_resolution.rs` | Bounded resolution of aliases and JSON Pointers with one hop counter, value-node and value-depth budgets, type determination, value validation; the dependency collector for affected resolution |
| `reference_positions.rs` | Where a curly-brace alias may appear inside a value, and which type it must target |
| `resolver.rs` | The result types (`Resolution`, `DocumentRequest`, `ResolvedToken(s)`, `ResolvedGroup`, `Deprecation`, `RuntimeLimits`), the `resolve`, `validate_complete` and `resolve_affected` drivers, fingerprints, `to_document`, `apply_affected` |
| `value.rs` | `validate_value`, `validate_value_with`, the per-type rules, `BLUR_TYPE` |
| `values.rs` | The public `values` module: typed models, `TypedValue`, `ResolvedToken::typed` and their `FromDtcgValue` implementations |
| `dtcg_tokens.rs`, `dtcg_component_tokens.rs`, `from_dtcg_value.rs`, `type_registry.rs` | Projection traits, built-in conversions, `join_token_path` |
| `local_reference.rs` | `LocalReferencePolicy` |
| `affected_resolution.rs` | `AffectedResolution` |
| `error.rs` | `DtcgError` |
| `diagnostics/mod.rs` | `DtcgDiagnostic`, compiled only with the `diagnostics` feature |

`dxn_dtcg_derive/src/lib.rs` parses the `#[dtcg(...)]` attributes strictly with `syn` and
emits a `DtcgTokens` implementation (and, for the component derive, `DtcgComponentTokens`). The
generated code refers to `::dxn_dtcg::…` and `::core::…` paths only, which is why applications
use the macros through `dxn_dtcg` and why local aliases cannot change it.

## Pipeline internals

A resolution runs four stages: **discover** the active sources, **fold** them into one merged
structure, **flatten** it into tokens, and **resolve** each token within the limits.

### 1. Discovery (`source_discovery.rs`)

`discover(catalog, root, limits, selection, include_inactive)` produces the ordered terminal
sources:

- `load` fetches a document from the catalog. If it is absent, it records a `DocumentRequest` in a
  `BTreeMap` keyed by id (so requests come out sorted and de-duplicated) and returns `None`.
  Otherwise it parses the text and applies the fragment pointer through `json_pointer.rs`.
- `expand` runs an explicit LIFO work queue of `(occurrence, depth, ancestors)`. Non-resolvers
  become terminal sources. Resolvers are validated by `resolver_definition.rs` and expanded by
  `resolver_sources`. `ancestors` holds the origins of the resolvers on the current path only, so
  a resolver repeated at different positions is allowed while a resolver nested inside itself is a
  `DocumentCycle`.
- `resolver_sources` reads the effective steps (`effective_steps`), and `choose_contexts` matches
  the selection against the resolver's declared modifiers (`selection::match_name`): it records
  which inputs this resolver claimed, applies defaults for the rest, and fails for a missing,
  unknown or duplicated context of its own modifiers. Set and modifier steps then return their
  source arrays: the chosen context, or every context when `include_inactive` is set.
- `append_source` handles one source: an inline source is kept; `#/sets/<name>` is expanded in
  place; another local pointer is selected from the scope document; an external reference is
  resolved against the resource that owns the `$ref` member, loaded and overlaid with the
  reference object's other members. References into `resolutionOrder` or `modifiers` are rejected
  on the decoded pointer.
- `check_depth`, `check_count` and `count_expansion` enforce `max_reference_depth` and the two
  `max_documents` counts throughout.

After discovery, `check_inputs` rejects any input that no reached resolver claimed. Previews and
`inspect_source` pass the input use of the supplied source and of the root graph together.

### 2. Folding and flattening (`flatten.rs`)

`flatten_sources(sources, limits)`:

- `Merger::fold` merges every terminal occurrence, in discovery order, into one JSON tree. Groups
  merge member by member; group `$type`, `$description`, `$deprecated` and `$extends` replace per
  key; `$extensions` objects merge recursively; a token replaces any earlier token whole; a token
  meeting a group at the same path is a collision. Structure is validated as it is folded (reserved
  properties, token rules, `$schema` only at a whole-document root), and the merger remembers which
  source last defined each token and group.
- `Flattener::flatten_group` walks the merged tree once with the inherited type and deprecation.
  `$extends` expansions are resolved iteratively in dependency order and memoized by group path, so
  a group may extend a group from another source or one produced by `$extends`; cycles are
  detected on the way. `emit` records a `RawToken` per path and counts `max_tokens`; `charge`
  counts the JSON nodes `$extends` copies; paths deeper than 128 segments fail with `path depth`.
- The token order is fixed once: each path at its last definition, sorted by a sequence key.
  `RawGroup`s become the `ResolvedGroup`s of the result.

Each `RawToken` keeps an `Arc` of the merged document, so every JSON Pointer reads the merged
structure.

### 3. Resolution and validation (`token_resolution.rs`)

`Resolver::resolve_token` resolves each path in order with memoization:

- A `visiting` stack detects alias cycles. One hop counter covers aliases, token pointers and
  document pointers and is checked against `max_alias_depth` before descending; memoized results
  carry their hop count, node count and depth, so the outcome does not depend on authored order.
- `resolve_value` substitutes curly aliases where `reference_positions.rs` allows them and checks
  the target's type; `resolve_json_reference` distinguishes token targets (`…/$value`, `…/$type`)
  from document pointers and applies sibling overrides. Every copy is charged against
  `max_value_nodes` first, and every node's depth is checked against the fixed value depth of 128.
- The effective type is the own `$type`, else the type of a whole-value reference's target, else
  the group's; an own type that differs from the target's is a `reference type mismatch`.
- `validate_value_with` checks the result with the runtime's `ValueExtensions`.

`resolver.rs` then computes the fingerprint: SHA-256 over `(path, type, value, source)` for every
token in order, hashing object members with sorted keys.

### 4. Affected resolution (`resolver.rs`)

`DtcgRuntime::replace_json_and_resolve_affected` flattens the current selection
(`flatten_selection`), replaces the document and calls `resolve_affected`. That repeats discovery
and flattening, then seeds the affected set with the named paths and everything under them, every
changed or new flattened definition (`same_definition`), and the removed paths. If any group uses
`$extends`, or the previous state is unknown, every token is affected. `DependencyCollector`
builds the reverse dependencies, marks readers of document pointers as affected, and the closure is
walked outwards. Only affected tokens are resolved. The runtime restores the previous document
unless the result is `Ready`. `ResolvedTokens::apply_affected` patches a snapshot so that it equals a
full resolution.

### 5. Inspection and export

`source_references::inspect` walks authored positions across documents with a visited set,
recording each source `$ref` it meets; a reference object that targets a resolver is walked as its
overlaid target. It does not consult the selection. `discover_source_with_inputs` runs the normal
discovery with a caller-supplied source appended in the root's scope. The previews call
`resolve_sources` on the root's sources plus the previewed occurrences.
`ResolvedTokens::to_document` writes a resolution back as a nested token document.

## Invariants

These rules come from the design decisions recorded in `.agentx/memory/decisions/`. Changes must
keep them:

- **I/O-free and UI-neutral.** No dependency on a UI toolkit or an application, and no file system,
  network or async runtime. Loading belongs to the caller through `Resolution::Missing`.
- **Strict by default.** Default options validate DTCG 2025.10 exactly. Every value shape beyond
  it is a `ValueExtensions` flag, off by default, with a doc comment stating its use case and a
  pinned test group (`tests/value_extensions.rs`, `tests/value_extension_types.rs`).
- **One merged structure.** All active sources fold into one structure before types, `$extends`,
  deprecation and pointers are evaluated. Groups merge; tokens are replaced whole; composite values
  are never merged.
- **Resolve after merging.** Effective types and aliases resolve only after the complete merge of
  the active selection.
- **One owner per rule.** Resolver structure lives in `resolver_definition.rs`, JSON Pointer syntax
  in `json_pointer.rs`, alias positions in `reference_positions.rs`. Every API reads them from
  there, so `resolve`, `validate_complete`, `modifiers`, `normalize_selection` and inspection
  cannot disagree.
- **Bounded work.** Every loop and recursion is bounded by a `RuntimeLimits` field or a fixed
  bound; new code that follows references or copies values must charge the existing budgets.
- **Atomic mutations.** Invalid ids, references, JSON or requests leave the catalog
  unchanged. Affected resolution restores the previous document unless it is `Ready`.
- **One ordering pass.** Fix the token order once per resolution. Do not reintroduce per-override
  scans.
- **Strict version.** Resolvers must declare exactly `2025.10`.
- **Order is semantic.** `serde_json`'s `preserve_order` feature must stay enabled: token order,
  ordered duplicate occurrences and `ResolvedTokens::iter()` all depend on authored order.
- **One path for previews.** Previews use the same discovery and resolution as `resolve`. Never
  build a synthetic resolver or mutate the catalog to preview.
- **Redacted diagnostics.** Diagnostics and `Debug` output never retain document text or JSON
  values, only ids, paths and stable codes.
- **Narrow projection.** Structs request only their fields, extra tokens are tolerated, and
  unprojected tokens stay available as alias targets. Consumer-owned types convert through
  `#[dtcg(with = "...")]` in the consumer; do not add consumer-specific `FromDtcgValue`
  implementations here.
- **Conformance ratchet.** The official corpus must keep passing. A deliberate deviation is listed
  in `KNOWN_GAPS` and in [Conformance](12-conformance.md), never silently.

## Development workflow

The toolchain is pinned in `rust-toolchain.toml`; `rustup` installs Rust 1.97.0 with Clippy,
rustfmt and the `wasm32-unknown-unknown` target on first use.

While working on a change, run only the test group for the feature you are touching:

```bash
cargo test -p dxn_dtcg --all-features --test lazy_catalog
```

Before committing, run the full validation:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo package --workspace
```

and, for changes that could affect portability:

```bash
cargo check --workspace --target wasm32-unknown-unknown
```

The timing baseline prints cold, repeat, context-switch and invalidation timings:

```bash
cargo test -p dxn_dtcg --bench resolver -- --nocapture
```

Commits follow [Conventional Commits](https://www.conventionalcommits.org/) with meaningful scopes,
for example `fix(resolver): …` or `docs(readme): …`.

### Documentation

The README and the chapters in `docs/` are written against the current API and compiled as
doctests. The private `documentation` module at the end of `crates/dxn_dtcg/src/lib.rs`
includes each file under `#[cfg(doctest)]`, one module per file:

```rust,ignore
#[cfg(doctest)]
mod documentation {
    #[doc = include_str!("../../../README.md")]
    mod readme {}
    #[doc = include_str!("../../../docs/06-resolvers-and-modifiers.md")]
    mod resolvers_and_modifiers {}
    // …one module per chapter
}
```

Every code block in the Markdown must therefore carry a language tag: `rust` (compiled and run),
`rust,no_run` (compiled), `rust,ignore` (only for signature listings that cannot compile), or a
non-Rust tag such as `json`, `toml`, `text` or `bash`. Doctests see the dev-dependencies, so the
`diagnostics` feature and `serde_json` are available. When you change public behavior, update the
affected chapters and check them, one chapter at a time:

```bash
cargo test -p dxn_dtcg --all-features --doc documentation::resolvers_and_modifiers
```

## Test groups

Each feature has its own integration-test file, selectable with `--test <name>`. A new feature or
behavior change gets a new group.

| Group | Covers |
|---|---|
| `dxn_dtcg/tests/official_test_suite.rs` | The official DTCG 2025.10 corpus (250 fixtures, `KNOWN_GAPS` ratchet), upstream line endings, `$schema` handling |
| `dxn_dtcg/tests/vertical_slice.rs` | End to end: missing documents, resolve, project |
| `dxn_dtcg/tests/api_foundations.rs` | Limit setters, redacted `Debug`, `UnresolvedReference`, comparable and clonable errors, `Resolution` helpers |
| `dxn_dtcg/tests/dtcg_2025_10_conformance.rs` | Format conformance: groups, `$root`, `$extends`, types, pointers, names, order, fingerprints |
| `dxn_dtcg/tests/format_structure.rs` | Token and group structure rules |
| `dxn_dtcg/tests/type_determination.rs` | Effective types and reference type mismatches |
| `dxn_dtcg/tests/reference_positions.rs` | Allowed alias positions and their target types |
| `dxn_dtcg/tests/merged_resolution.rs` | Merging sources, `$extends` across sources, pointers on the merged structure |
| `dxn_dtcg/tests/token_metadata.rs` | `$description`, `$deprecated`, `$extensions`, `ResolvedGroup` |
| `dxn_dtcg/tests/document_export.rs` | `to_document` and its round trip |
| `dxn_dtcg/tests/typed_values.rs` | The `values` models and `TypedValue` |
| `dxn_dtcg/tests/value_extensions.rs` | Strict defaults and the `ValueExtensions` flags through every constructor |
| `dxn_dtcg/tests/value_extension_types.rs` | The value extension shapes |
| `dxn_dtcg/tests/local_reference_policy.rs` | `LocalReferencePolicy` for single references and resolvers |
| `dxn_dtcg/tests/resolver_2025_10.rs` | Resolver version, inputs, inline steps, overrides, set references |
| `dxn_dtcg/tests/resolver_document_validation.rs` | Allowed resolver members, empty `resolutionOrder`, single-context modifiers |
| `dxn_dtcg/tests/resolution_step_consistency.rs` | One effective step definition for every API |
| `dxn_dtcg/tests/input_selection.rs` | Case-insensitive inputs, nested-resolver inputs, selection errors |
| `dxn_dtcg/tests/alias_overlay.rs` | Aliases resolve against the final merged token set |
| `dxn_dtcg/tests/bounded_expansion.rs` | `max_tokens`, `max_documents` fan-out, `$extends` depth |
| `dxn_dtcg/tests/bounded_references.rs` | Alias depth, value nodes, value depth, deep pointer chains on a small stack, percent-decoding |
| `dxn_dtcg/tests/lazy_catalog.rs` | The missing-document protocol, batch atomicity, revisions, inactive contexts, cycles and limits |
| `dxn_dtcg/tests/resource_id_adversarial.rs`, `resource_id_drive_prefix.rs`, `resource_id_portability.rs` | Resource id normalization and rejection: fragments, `:`, non-portable segments |
| `dxn_dtcg/tests/incremental_resolution.rs`, `incremental_consistency.rs` | Affected resolution, rollback, and that every patched snapshot equals a full resolution |
| `dxn_dtcg/tests/source_inspection.rs` | Inspection, previews, reference sites, shared limits, definition helpers |
| `dxn_dtcg/tests/source_reference_bounds.rs` | `source_references` lists each site once, walks in linear time and counts repeated walks against `max_documents` |
| `dxn_dtcg/tests/source_reference_parity.rs` | `source_references` counts repeated walks in discovery's units and lists what `validate_complete` accepts |
| `dxn_dtcg/tests/extensions_merge.rs` | Recursive group and root `$extensions` merge across sources and `$extends` |
| `dxn_dtcg/tests/derive_projection.rs` | Derive attributes through the runtime crate |
| `dxn_dtcg/tests/diagnostics.rs` | Feature gating and redaction of diagnostics |
| `dxn_dtcg_derive/tests/derive_projection.rs`, `derive_shapes.rs` | Narrow projection and derive shapes with stable component metadata |
| `dxn_dtcg_derive/tests/derive_hardening.rs` | `Option` fields, hygienic generated code, raw identifiers, per-field generic bounds |
| `dxn_dtcg_derive/tests/compile_fail.rs` | `trybuild` compile-fail cases in `tests/ui` |

### The official corpus

The corpus under `crates/dxn_dtcg/tests/fixtures/dtcg-test-suite/2025.10/` is byte-identical
to upstream (pinned commit and update steps in that directory's `README.md`). Never edit fixture
files. When a change makes a fixture fail on purpose, add its id to `KNOWN_GAPS` in
`tests/official_test_suite.rs` and document the deviation in [Conformance](12-conformance.md);
when a change fixes one, remove it. The test fails in both directions until the list matches.

### Compile-fail snapshots

Each `crates/dxn_dtcg_derive/tests/ui/*.rs` case must fail to compile with exactly the
diagnostic in its `.stderr` file. After an intended change to a derive error message, regenerate
the snapshots and review the diff:

```bash
TRYBUILD=overwrite cargo test -p dxn_dtcg_derive --test compile_fail
```

## CI

`.github/workflows/ci.yml` runs on pull requests and on pushes to `main`, on `ubuntu-latest` and
`windows-latest`, with the pinned toolchain:

1. `cargo fmt --check`
2. `cargo clippy --workspace --all-targets --all-features -- -D warnings`
3. `cargo test --workspace --all-features` (including the official corpus and the documentation
   doctests)
4. `cargo check --workspace --target wasm32-unknown-unknown`
5. `cargo package --workspace`

## Releases

A release is a git tag `vX.Y.Z` on `main` with a GitHub Release; nothing is published to crates.io.
Both crates are dual-licensed MIT OR Apache-2.0 and share one version (`[workspace.package]` in the
root `Cargo.toml`), one tag and one root `CHANGELOG.md`. release-plz (`release-plz.toml`,
`.github/workflows/release-plz.yml`) derives the next version from
[Conventional Commits](https://www.conventionalcommits.org/), keeps a release pull request with the
version bump and the new changelog section open on `main`, and tags the release and creates the
GitHub Release when that pull request is merged. `CHANGELOG.md` is written by release-plz;
migrations for consumers go into `Migration:` footers on the commits. The crates stay on `1.x`: no
commit is marked breaking.

[Releasing](releasing.md) has the details: commit types and the 1.x policy, what to check before
merging a release pull request, the `RELEASE_PLZ_TOKEN` secret, the tag ruleset and the by-hand
fallback.

The manifests stay packageable (`cargo package --workspace` runs in CI): the derive crate's
dev-dependency on `dxn_dtcg` is path-only on purpose, which breaks the runtime ↔ derive
dependency cycle in a packaged crate, and `dxn_dtcg` excludes
`tests/fixtures/dtcg-test-suite/**` from the package, so the corpus never ships.

## Consuming releases

Consumers depend on a release tag of this repository,
`dxn_dtcg = { git = "https://github.com/dXnSix/dXn.Dtcg", tag = "vX.Y.Z" }`, and move to a new
release by changing the tag. A change a consumer needs is released here first. A consumer whose
documents use dimension line heights or `dxn.dtcg.blur` constructs its runtimes with the matching
[value extensions](05-value-types.md#strict-values-and-the-value-extensions), for example
`RuntimeOptions::default().with_extensions(ValueExtensions::all())`.
