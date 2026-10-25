---
title: Conformance
description: How dxn_dtcg relates to DTCG 2025.10 - the official corpus result, what is supported, implementation-defined choices and what is not supported.
group: Reference
order: 180
---

`dxn_dtcg` is a **resolver** for the DTCG 2025.10 Format, Color and Resolver modules: it reads documents, computes token values and can export a resolution as a token document. It is not an authoring tool and does not format or rewrite the documents it reads.

## Official conformance corpus

The runtime passes the whole official DTCG 2025.10 test suite. The harness prints:

```text
official corpus: 250 passed, 0 known gaps
```

| | |
|---|---|
| Upstream | `design-tokens/community-group`, directory `test-suite/tests/2025.10/` |
| Pinned commit | `882ebd6716abef9a46d8ef5fb12e82009d6cee2e` |
| Fixtures | 220 format and 30 resolver fixtures, each a positive or negative evaluation test |
| Vendored at | `crates/dxn_dtcg/tests/fixtures/dtcg-test-suite/2025.10/`, byte-identical to upstream, excluded from the packaged crate |
| Harness | `crates/dxn_dtcg/tests/official_test_suite.rs` |

The harness evaluates every manifest entry with default `RuntimeOptions` (strict values). A **format fixture** passes when `DtcgRuntime::from_json` succeeds and `resolve(&[])` is `Ready`; a **resolver fixture** passes when `validate_complete()` is `Ready` and `resolve` then succeeds with each modifier's default context, or its first context when it has no default. A positive fixture must be accepted; a negative one must fail with an `Err` (`Missing` and panics never count). The `KNOWN_GAPS` list is a ratchet: it must equal the set of fixtures whose expectation the runtime does not meet, so both a new failure and a fixed-but-still-listed gap fail the test. It is empty. The result above was reproduced at v1.1.0.

When sources disagree, the runtime follows them in this order: the published specification text (with errata), then the official corpus where the text is silent, then its own decisions, each listed below and pinned by a test.

## Supported

### Format and Color modules

| Feature | Status |
|---|---|
| Tokens (`$value`) and nested groups | Supported ([Token documents](./token-documents.md#structure)) |
| `$schema` | Accepted and ignored at the root of a whole document |
| `$type` on tokens and groups, with inheritance | Supported, evaluated after merging all sources |
| Type of a reference (Format 5.2.2) | Own `$type`, else the target's type for a whole-value reference, else the group's |
| `$description`, `$deprecated`, `$extensions` | Exposed on `ResolvedToken` and `ResolvedGroup`; `$deprecated` inherited from groups |
| `$root` tokens, `$extends` group extension | Supported, across sources and on groups produced by `$extends` |
| Name restrictions (`{`, `}`, `.`, `$`, non-empty) | Enforced |
| Curly-brace aliases, whole and at composite members | At the positions Format 7 and 9 allow ([Aliases](./aliases.md#where-aliases-are-allowed)) |
| JSON Pointer `$ref` at value, property and token level | RFC 6901, percent-decoded |
| Reference objects with sibling overrides; cycle detection | Supported |
| All 13 types, validated after resolution; all 14 color spaces with their component ranges | Supported ([Value types](./value-types.md)) |
| Typed Rust models of every type | `dxn_dtcg::values`, `ResolvedToken::typed` |

### Resolver module

| Feature | Status |
|---|---|
| `version: "2025.10"` | Required, exactly |
| Allowed resolver members | Enforced on the root, sets, modifiers and inline steps |
| Sets; modifiers with contexts and an optional default (at least two contexts) | Supported |
| `resolutionOrder` with `$ref` and inline steps | Supported; must not be empty |
| Sources: inline, file, fragment, set, local pointer; source references with shallow overrides | Supported |
| Merging sources into one token structure (6.2 to 6.3) | Groups merge member by member; tokens replace whole |
| Inputs (5) with defaults | Supported; names match case-insensitively |
| Resolved output as a token document (8) | `ResolvedTokens::to_document()` |

## Strict by default, value extensions on request

With default `RuntimeOptions`, values are validated strictly against 2025.10, and `validate_value` is always strict. Two value shapes beyond 2025.10 are opt-in through `ValueExtensions`: typography `lineHeight` as a `dimension` (or an alias to one), and the `dxn.dtcg.blur` type. `ValueExtensions::all()` turns both on. The opt-in `LocalReferencePolicy`, which limits a resolver to relative local documents, does not change how standard documents behave; a host applies it in its own loader. See [Value types](./value-types.md#strict-values-and-the-value-extensions) and [Loading documents](./loading.md#path-safety).

## Expressions: an extension, not a change

DTCG 2025.10 has no computed values. The opt-in [expressions](./expressions.md) extension adds them without changing how any standard document resolves:

- **Where it lives.** An expression is stored in a token's `$extensions` under `dxn.dtcg.expressions`, the place Format §5.2.3 gives tools for their own data, and module declarations under the same key in a document's root `$extensions`. The token keeps its `$value`, which Format §8 requires to match its `$type`.
- **The snapshot stays valid.** That `$value` is resolved and validated exactly as without expressions, so a document with expressions is a valid DTCG 2025.10 document for every tool, and an invalid snapshot is an error even when an expression would replace it.
- **Opaque unless enabled.** Without the `expressions` feature, or with it but without `RuntimeOptions::with_expressions`, the extension is ordinary metadata: values come from `$value`, the object is kept unchanged in `ResolvedToken::extensions` and never validated, and resolution is strict DTCG 2025.10 as described on this page. The official corpus runs with default options, so the result above does not depend on the feature.
- **With it enabled**, an evaluated result replaces the value, is validated against the token's type like any value, and the resolved `$value` is kept as `ResolvedToken::snapshot`. `to_document` writes the evaluated values as `$value` and keeps the `$extensions`, so the export is again a valid DTCG document.

## Implementation-defined choices

Places where the specification is silent, leaves a choice or contradicts itself. They matter when moving token files from other tools.

**References**

- A token-level `$ref` points at the target's `.../$value`, as in Format Example 32 and the corpus. A pointer to the token object itself (the form of Example 21) is an error; no `/$value` is appended implicitly.
- A `.../$value` pointer whose prefix is not a token is an ordinary document pointer, and the token then takes its group's type.
- JSON Pointer fragments are percent-decoded before `~1` and `~0` are unescaped (RFC 6901 6), everywhere. Array indexes are `0` or have no leading zero or sign.
- An alias to a group (`{spacing}`) is an error; a group's root token is `{spacing.$root}`.
- A JSON string is an alias exactly when it starts with `{` and ends with `}`. There is no interpolation.

**Values**

- `xyz-d65` and `xyz-d50` components are bounded to [0, 1], like the RGB spaces.
- Hue components (`hsl`, `hwb`, `lch`, `oklch`) must lie in [0, 360) and are never wrapped.
- A color's `hex` is matched case-insensitively and format-checked, not compared with the components.
- Gradient stop positions outside [0, 1] are valid and stops need not be sorted. The resolved JSON keeps them as written; `values::Gradient` clamps them.
- Negative dimensions and durations are accepted.

**Resolver documents and inputs**

- A SHOULD in the Resolver module becomes an error only where the official JSON schema and the corpus enforce it: a single-context modifier, an unknown resolver member and an empty `resolutionOrder`. Case-insensitive input matching is implemented as behavior: exact match first, then lowercase, several candidates are an error.
- Every declared root modifier needs a selected or default context, even if `resolutionOrder` does not reference it.
- Selections are ignored when the resolver graph declares no modifiers.

**Nested resolvers** are a runtime extension: a source may reference another resolver, expanded in place. One selection serves the whole graph; `modifiers()` lists the root's axes only, `normalize_selection` every reached modifier ([Resolvers](./resolvers.md#nested-resolvers)).

**Merging and order**

- Group `$type`, `$description`, `$deprecated` and `$extends` replace per key; group and root `$extensions` merge recursively; tokens replace whole; composite values are never merged; a path is a token or a group, not both.
- Token order is the authored order across merged sources, each path at its last definition; tokens copied by `$extends` take the position of the extending group's `$extends`.

## Not supported

- **Remote or absolute references.** Resource ids are relative and logical; URLs are rejected. Loading is up to you, and you may fetch from anywhere.
- **JSONC or JSON5 documents.** Every document must be strict JSON.
- **Warnings for names that differ only in case.** Such names are distinct tokens.
- **Aggregated error reports.** Resolution stops at the first error.
- **Other versions.** Resolvers must declare exactly `2025.10`. Drafts and the legacy `value`/`type` keys without `$` are not read.
- **Removing tokens** through overrides.

## Where conformance is tested

Run one group with `cargo test -p dxn_dtcg --all-features --test <group>`.

| Test group (`crates/dxn_dtcg/tests/`) | Covers |
|---|---|
| `official_test_suite.rs` | The vendored corpus with the `KNOWN_GAPS` ratchet, and `$schema` handling |
| `dtcg_2025_10_conformance.rs` | Groups, `$root`, `$extends`, effective types, the 13 value types, pointers, names, ordering, fingerprints |
| `format_structure.rs`, `type_determination.rs`, `reference_positions.rs` | Structure rules, effective types, alias positions and target types |
| `merged_resolution.rs`, `extensions_merge.rs` | Merging sources, `$extends` across sources, recursive `$extensions` merge |
| `resolver_2025_10.rs`, `resolver_document_validation.rs`, `input_selection.rs` | Resolver version, members, steps, overrides, case-insensitive and nested inputs |
| `value_extensions.rs`, `value_extension_types.rs` | Strict values and the opt-in extensions |
| `expressions_disabled.rs` | Expression extensions stay opaque metadata without the feature or the option (run with and without `--features expressions`) |

Next: [Platforms and versions](./platforms.md).
