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
- [Working with RayX](#working-with-rayx)

## Repository layout

```text
.
├── Cargo.toml                    workspace members, shared package metadata, dependency versions
├── rust-toolchain.toml           pinned toolchain (1.95.0) with clippy, rustfmt and the wasm32 target
├── crates/
│   ├── dtcg_runtime/
│   │   ├── src/                  the runtime (see the module map)
│   │   ├── tests/                one integration-test group per feature
│   │   └── benches/resolver.rs   timing baseline, written as a #[test]
│   └── dtcg_runtime_derive/
│       ├── src/lib.rs            the two derive macros
│       └── tests/                derive shape and projection tests
├── docs/                         this documentation
├── .github/workflows/ci.yml      CI
├── .agentx/                      AgentX repository memory: decisions and domain knowledge
├── AGENTS.md, CLAUDE.md          contributor guidance for coding agents
└── LICENSE-APACHE, LICENSE-MIT
```

## Module map

All modules of `dtcg_runtime` are private; `lib.rs` re-exports the public items.

| Module | Responsibility |
|---|---|
| `runtime.rs` | `DtcgRuntime`: owns the catalog, root, limits and registry; the public API |
| `catalog.rs` | `DocumentCatalog`: id → `Arc<str>` map, JSON well-formedness check, atomic batches, revision |
| `resource.rs` | `ResourceId` and `DocumentReference`: normalization and relative resolution |
| `json_pointer.rs` | Parsing of `#/` JSON Pointers into unescaped segments |
| `source_discovery.rs` | `Discovery`: expands resolvers into ordered terminal `SourceOccurrence`s, collects missing documents, enforces document and depth limits, detects resolver cycles |
| `resolver_definition.rs` | Resolver structure: steps, local references, set and context source arrays, input validation; the public `set_sources` and `modifier_contexts` |
| `selection.rs` | `ModifierAxis` listing and selection normalization |
| `source_occurrence.rs` | `SourceOccurrence`: JSON nodes that carry their origin through `child`, `select` and `overlay` |
| `source_origin.rs` | `SourceOrigin` |
| `source_reference.rs`, `source_references.rs` | `SourceReference` and the walk over authored reference sites |
| `resolver.rs` | Flattening, overlay order, alias and pointer resolution, effective types, fingerprints, affected resolution; also `Resolution`, `DocumentRequest`, `ResolvedToken(s)` and `RuntimeLimits` |
| `value.rs` | `validate_value`, the per-type rules, `BLUR_TYPE` |
| `dtcg_tokens.rs`, `dtcg_component_tokens.rs`, `from_dtcg_value.rs`, `type_registry.rs` | Projection traits, built-in conversions, `join_token_path` |
| `envelope.rs` | `ResolverEnvelope` |
| `local_reference.rs` | `LocalReferencePolicy` |
| `affected_resolution.rs` | `AffectedResolution` |
| `error.rs` | `DtcgError` |
| `diagnostics/mod.rs` | `DtcgDiagnostic`, compiled only with the `diagnostics` feature |

`dtcg_runtime_derive/src/lib.rs` parses the `#[dtcg(...)]` attributes with `syn` and emits a
`DtcgTokens` implementation (and, for the component derive, `DtcgComponentTokens`). The generated
code refers to `::dtcg_runtime::…` paths, which is why applications use the macros through
`dtcg_runtime`.

## Pipeline internals

### 1. Discovery (`source_discovery.rs`)

`discover(catalog, root, limits, selection, include_inactive)` produces the ordered terminal
sources:

- `Discovery::new` rejects duplicate selection inputs.
- `load` fetches a document from the catalog. If it is absent, it records a `DocumentRequest` in a
  `BTreeMap` keyed by id (so requests come out sorted and de-duplicated) and returns `None`.
  Otherwise it parses the text and applies the fragment pointer.
- `expand` runs an explicit LIFO work queue of `(occurrence, depth, ancestors)`. Non-resolvers
  become terminal sources. Resolvers are validated (`validate_resolver`) and expanded by
  `resolver_sources`. `ancestors` holds the origins of the resolvers on the current path only, so
  a resolver repeated at different positions is allowed while a resolver nested inside itself is a
  `DocumentCycle`.
- `resolver_sources` walks `resolutionOrder`. `resolution_step` turns `$ref` steps into their
  definitions (with overlay) and checks inline steps. `set_documents` and `modifier_documents`
  return the source arrays: the selected or default context, or every context when
  `include_inactive` is set.
- `append_source` handles one source: an inline source is kept; `#/sets/<name>` is expanded in
  place; another local pointer is selected from the scope document; an external reference is
  resolved against the resource that owns the `$ref` member, loaded and overlaid with the
  reference object's other members.
- Depth and document-count checks run throughout.

### 2. Flattening and overlay (`resolver.rs`)

`flatten_group` walks each terminal source and records a `RawToken` per token path, in visit
order:

- reserved-name and string-type checks;
- `effective_group_type`: the group's own `$type`, else the `$extends` chain, else the inherited
  type;
- a `$value` token, or a token-level `$ref` token, ends the walk at that node;
- `$extends` flattens the target group at the same prefix first, with a stack for cycle
  detection;
- then `$root`, then the children, with name validation.

Each `RawToken` keeps an `Arc` of its source's JSON for document-pointer resolution.
`keep_last_definitions` then fixes the order in one reverse pass with a `HashSet`, keeping each
path at its last definition. This single pass replaced a per-override scan of the order that
dominated edit latency in the Token Editor.

### 3. Resolution and validation (`resolver.rs`)

`resolve_token` is a depth-first resolution with memoization in the `resolved` map:

- a `visiting` stack detects alias cycles and enforces `max_alias_depth`; a separate
  `pointer_visiting` stack detects circular document pointers;
- a token without a type gets the target's type when its value is a whole-token reference
  (`whole_token_reference`);
- `resolve_value` substitutes curly aliases, resolves `$ref` objects (`resolve_json_reference_target`
  distinguishes token targets from document targets and applies sibling overrides), and recurses
  through arrays and objects;
- `validate_value` checks the result against the effective type.

`fingerprint` hashes `(path, type, value, source)` for every token in order with SHA-256.

### 4. Affected resolution (`resolver.rs`)

`resolve_affected` repeats discovery and flattening, builds a reverse dependency map with
`collect_token_dependencies`, and walks it outwards from the changed paths. Only the tokens it
reaches are resolved. If any source contains `$extends` (`contains_group_extension`), every token
is affected. `DtcgRuntime::replace_json_and_resolve_affected` wraps this with the catalog
replacement and the rollback.

### 5. Inspection (`source_references.rs`, `source_discovery.rs`)

`source_references::inspect` walks authored positions (`Resolver`, `Definition`, `Source`) across
documents with a visited set, recording each source `$ref` it meets. It does not consult the
selection. `discover_source` runs the normal discovery with a caller-supplied source appended in the
root's scope. The previews call `resolve_sources` on the root's sources plus the previewed
occurrences.

## Invariants

These rules come from the design decisions recorded in `.agentx/memory/decisions/`. Changes must
keep them:

- **I/O-free and UI-neutral.** No dependency on RayX, gpux or GPUI, and no file system, network or
  async runtime. Loading belongs to the caller through `Resolution::Missing`.
- **Whole-token overlay.** A later definition replaces the whole token at its path. Never
  deep-merge composite values, groups or Serde objects.
- **Resolve after overlay.** Effective types and aliases resolve only after the complete overlay of
  the active selection.
- **Atomic mutations.** Invalid ids, references, JSON or requests leave the catalog unchanged.
  Affected resolution restores the previous document unless it is `Ready`.
- **One ordering pass.** Fix the token order once per resolution. Do not reintroduce per-override
  scans.
- **Strict version.** Resolvers must declare exactly `2025.10`.
- **Order is semantic.** `serde_json`'s `preserve_order` feature must stay enabled: token order,
  ordered duplicate occurrences and `ResolvedTokens::iter()` all depend on authored order.
- **One path for previews.** Previews use the same discovery and resolution as `resolve`. Never
  build a synthetic resolver or mutate the catalog to preview.
- **Redacted diagnostics.** Diagnostics never retain document text or JSON values, only ids,
  paths and stable codes.
- **Narrow projection.** Structs request only their fields, extra tokens are tolerated, and
  unprojected tokens stay available as alias targets. Consumer-owned types convert through
  `#[dtcg(with = "...")]` in the consumer; do not add consumer-specific `FromDtcgValue`
  implementations here.
- **Documented extensions.** Every value shape beyond DTCG 2025.10 needs a doc comment naming its
  consumer and a pinned test group, as `tests/rayx_export_types.rs` does for the RayX Figma export.

## Development workflow

The toolchain is pinned in `rust-toolchain.toml`; `rustup` installs Rust 1.95.0 with Clippy,
rustfmt and the `wasm32-unknown-unknown` target on first use.

While working on a change, run only the test group for the feature you are touching:

```bash
cargo test -p dtcg_runtime --all-features --test lazy_catalog
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
cargo test -p dtcg_runtime --bench resolver -- --nocapture
```

Commits follow [Conventional Commits](https://www.conventionalcommits.org/) with meaningful scopes,
for example `fix(resolver): …` or `docs(readme): …`.

### Documentation

The README and the chapters in `docs/` are written against the current API, and every Rust example
is a complete program. When you change public behavior, update the affected chapters. To check
that the examples still compile and pass, include the Markdown files as doc comments in a scratch
crate that depends on `dtcg_runtime` (with the `diagnostics` feature) and `serde_json`, then run
`cargo test --doc`:

```rust,ignore
#![doc = include_str!("../../RayX.Dtcg/README.md")]
#[doc = include_str!("../../RayX.Dtcg/docs/06-resolvers-and-modifiers.md")]
pub mod resolvers {}
// …one module per chapter
```

Every code block in the Markdown must therefore carry a language tag: `rust` (compiled and run),
`rust,no_run` (compiled), `rust,ignore` (not compiled), or a non-Rust tag such as `json`, `toml`,
`text` or `bash`.

## Test groups

Each feature has its own integration-test file, selectable with `--test <name>`. A new feature or
behavior change gets a new group.

| Group | Covers |
|---|---|
| `dtcg_runtime/tests/vertical_slice.rs` | End to end: missing documents, resolve, project |
| `dtcg_runtime/tests/dtcg_2025_10_conformance.rs` | Format conformance: groups, `$root`, `$extends`, types, pointers, names, order, fingerprints |
| `dtcg_runtime/tests/resolver_2025_10.rs` | Resolver version, inputs, inline steps, overrides, set references |
| `dtcg_runtime/tests/alias_overlay.rs` | Aliases resolve against the final overlay |
| `dtcg_runtime/tests/lazy_catalog.rs` | The missing-document protocol, batch atomicity, revisions, inactive contexts, cycles and limits |
| `dtcg_runtime/tests/resource_id_adversarial.rs` | Resource id normalization and rejection |
| `dtcg_runtime/tests/resource_id_drive_prefix.rs` | Rejection of `:` in ids and references: drive prefixes, URI schemes, NTFS streams |
| `dtcg_runtime/tests/incremental_resolution.rs` | Affected resolution, rollback, the `$extends` fallback |
| `dtcg_runtime/tests/source_inspection.rs` | Inspection, previews, reference sites, shared limits, definition helpers |
| `dtcg_runtime/tests/derive_projection.rs` | Derive attributes: `root`, `rename`, `rest`, component ids |
| `dtcg_runtime/tests/rayx_export_types.rs` | RayX value-shape extensions |
| `dtcg_runtime/tests/diagnostics.rs` | Feature gating and redaction of diagnostics |
| `dtcg_runtime_derive/tests/derive_projection.rs` | A narrow projection through the derive crate |
| `dtcg_runtime_derive/tests/trybuild.rs` | Derive shapes with stable component metadata (despite the name, it does not use the `trybuild` crate) |

## CI

`.github/workflows/ci.yml` runs on pull requests and on pushes to `main`, on `ubuntu-latest` and
`windows-latest`, with the pinned toolchain:

1. `cargo fmt --check`
2. `cargo clippy --workspace --all-targets --all-features -- -D warnings`
3. `cargo test --workspace --all-features`
4. `cargo check --workspace --target wasm32-unknown-unknown`
5. `cargo package --workspace`

## Releases

Both crates share the workspace version (`[workspace.package]` in the root `Cargo.toml`) and are
dual-licensed MIT OR Apache-2.0.

- **Publish order:** `dtcg_runtime_derive` first, then `dtcg_runtime`, because the runtime's
  normal dependency on the derive crate must resolve from the registry.
- The derive crate's dev-dependency on `dtcg_runtime` is path-only on purpose: publishing strips it,
  which breaks the runtime ↔ derive dependency cycle.
- Both crates use the repository `README.md` as their crates.io readme, so keep it accurate for
  crates.io readers too.

## Working with RayX

[RayX](https://github.com/rayx-ui) depends on this repository through a relative path
(`../RayX.Dtcg/crates/...`) for local co-development. A change to a public API that RayX uses needs
the matching RayX change. Keep the two commits paired, and validate RayX's affected crates and Test
Suite specs. RayX's `rayx_theme` crate is the main consumer. It projects one generated theme model
with `nested` fields and `with` converters for gpux types, and loads `.tokeneditor` projects through
`from_envelope`.
