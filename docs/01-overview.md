# 1. Overview

`dxn_dtcg` turns DTCG 2025.10 design-token documents into resolved, validated token values that
Rust programs can use directly. This chapter explains what that means, the concepts involved, how
the runtime is organized, and the principles behind its design.

- [What the runtime does](#what-the-runtime-does)
- [DTCG in five minutes](#dtcg-in-five-minutes)
- [The resolution pipeline](#the-resolution-pipeline)
- [Design principles](#design-principles)
- [Crates and features](#crates-and-features)
- [Platform support](#platform-support)
- [RayX-specific conveniences (opt-in)](#rayx-specific-conveniences-opt-in)
- [Glossary](#glossary)

## What the runtime does

A design system describes its visual decisions (colors, spacing, type, radii, shadows, motion) as
*design tokens*. The Design Tokens Community Group (DTCG) standardizes a JSON format for them.
Version 2025.10 has two parts: the **format**, for token files, and **resolvers**, documents that
say which token files to combine and how to switch between variants such as light and dark themes.

Given a set of these documents and a choice of variant, `dxn_dtcg`:

1. finds every document the choice depends on, and asks the caller for any it does not have;
2. merges the sources, in the order the resolver gives, into one token document: groups merge, and
   a later definition of a token replaces an earlier one;
3. resolves aliases (`{color.brand}`) and JSON Pointer references against that merged document;
4. works out each token's type and checks its value against the DTCG rules for that type;
5. returns the tokens as `ResolvedTokens`, with their metadata, in a deterministic order and with a
   content fingerprint;
6. optionally converts values into typed Rust models (`dxn_dtcg::values`) or maps them onto
   your own structs through `#[derive(DtcgTokens)]`.

It does no I/O. It works on JSON *strings* that you provide under logical names, and when it needs a
document it does not have, it returns a request for it instead of an error.

## DTCG in five minutes

### Tokens and groups

A token is a JSON object with a `$value`. Every other object is a group. Tokens are named by the
path of group names that leads to them:

```json
{
  "color": {
    "$type": "color",
    "brand": {
      "primary": { "$value": { "colorSpace": "srgb", "components": [0.2, 0.4, 0.9] } }
    }
  }
}
```

This document defines one token, `color.brand.primary`. Its type, `color`, is inherited from the
`color` group. Tokens are leaves: a token has no child tokens or groups. Members whose names start
with `$` are reserved properties (`$value`, `$type`, `$description`, `$deprecated`, `$extensions`,
`$root`, `$extends`, and `$schema` at the top of a file); see [Token documents](03-token-documents.md).

### Types

Each token has one of 13 types: `color`, `dimension`, `fontFamily`, `fontWeight`, `duration`,
`cubicBezier`, `number`, `strokeStyle`, `border`, `transition`, `shadow`, `gradient` and
`typography`. The type fixes the shape of the value; a `dimension`, for example, is
`{ "value": 16, "unit": "px" }`. See [Value types](05-value-types.md).

### Aliases

A token's value can point to another token, either as a curly-brace alias (`"{color.brand.primary}"`)
or as a JSON Pointer (`{ "$ref": "#/color/brand/primary/$value" }`). A curly-brace alias can be the
whole value or a member of a composite value, so a border can take its color from a palette token.
A JSON Pointer can reach any part of a value. See [Aliases and references](04-aliases-and-references.md).

### Resolvers, sets and modifiers

A resolver document combines token sources:

```json
{
  "version": "2025.10",
  "sets": {
    "foundation": { "sources": [{ "$ref": "foundation.json" }] }
  },
  "modifiers": {
    "theme": {
      "default": "light",
      "contexts": {
        "light": [{ "$ref": "light.json" }],
        "dark": [{ "$ref": "dark.json" }]
      }
    }
  },
  "resolutionOrder": [{ "$ref": "#/sets/foundation" }, { "$ref": "#/modifiers/theme" }]
}
```

- A **set** is an ordered list of sources that is always applied.
- A **modifier** has named **contexts**, each with its own sources. Only the selected context is
  applied. A **default** context is used when the caller selects none.
- **`resolutionOrder`** decides the order in which sources merge. Later sources override earlier
  ones.
- A **selection** is the caller's choice of context for each modifier, such as
  `[("theme", "dark")]`. Names match case-insensitively.

See [Resolvers and modifiers](06-resolvers-and-modifiers.md).

## The resolution pipeline

```text
          caller                                        dxn_dtcg
 ┌────────────────────────┐        ┌──────────────────────────────────────────────────────┐
 │ provide_json(id, text) │──────► │ DocumentCatalog: ResourceId → Arc<str>, revision      │
 └────────────────────────┘        └──────────────────────────────────────────────────────┘
 ┌────────────────────────┐        ┌──────────────────────────────────────────────────────┐
 │ resolve(selection)     │──────► │ 1. Discovery                                         │
 └────────────────────────┘        │    walk resolutionOrder for the selected contexts,   │
             ▲                     │    expand set refs, files, fragments, nested         │
             │ Missing(requests)   │    resolvers; collect missing documents              │
             └──────────────────── │ 2. Merge + flatten                                   │
                                   │    fold sources into one structure (groups merge,    │
                                   │    tokens replace whole), apply $extends, inherit    │
                                   │    $type and $deprecated, list token paths           │
                                   │ 3. Resolve                                           │
                                   │    aliases and pointers on the merged document,      │
                                   │    effective types, cycle and limit checks           │
                                   │ 4. Validate                                          │
                                   │    each resolved value against its DTCG type         │
                                   │ 5. Fingerprint                                       │
                                   │    SHA-256 over path, type, value and source         │
                                   └───────────────────────────┬──────────────────────────┘
                                                               ▼
                                      Resolution::Ready(ResolvedTokens) ──► project::<T>()
```

Every `resolve` call runs this pipeline from the catalog's current contents. Nothing is cached
between calls. If you want caching, use the [fingerprint](09-incremental-resolution.md) or the
catalog revision as the key.

## Design principles

### I/O-free, string-backed catalog

The runtime stores documents as `Arc<str>` keyed by a logical `ResourceId`, a normalized, relative,
`/`-separated name such as `themes/dark.json`. It never opens files or sockets. When discovery
reaches a document that is not in the catalog, `resolve` returns
`Resolution::Missing(Vec<DocumentRequest>)`. The caller loads those documents however it wants and
calls `resolve` again.

Because of this, one runtime serves file systems, HTTP, embedded assets, databases and in-memory
tests alike, and it builds for `wasm32-unknown-unknown`. The cost is that the caller runs the short
request/provide loop. See [Document loading](07-document-loading.md).

### One merged document, then aliases

Sources fold, in order, into one merged token structure, as Resolver §6 describes. Groups merge
member by member, and group properties such as `$type` are replaced per key, so an override
inherits the group type that an earlier file declared. A later definition of the same token path
replaces the **whole** token: its value, its own type and its metadata. Composite values are never
deep-merged. Types, `$extends`, aliases and JSON Pointers are evaluated only on the merged
structure, so an alias always sees the definition that won for the current selection. That is what
makes a dark context that overrides `palette.primary` change every token that aliases it. See
[Token documents](03-token-documents.md#several-sources-one-merged-document).

### Strict validation

The runtime implements DTCG 2025.10 strictly by default and rejects what it does not understand.
It refuses unknown `$`-prefixed properties, malformed token structure, unsupported types, tokens
without a type, values that do not match their type, aliases at positions the format does not
allow, references whose target has the wrong type, dangling aliases and reference cycles. Value
shapes beyond the specification are opt-in through `RuntimeOptions`. Errors name the resource and
the token path, so a broken token file fails loudly at load time, not as a wrong color on screen.

### Narrow typed projection

A projected struct asks for exactly the tokens it needs. A requested token that is missing is an
error. Tokens it does not request are allowed, and they still take part in resolution, so a
projected token can alias an unprojected one. See [Typed projection](08-typed-projection.md).

### Deterministic and bounded

- JSON member order is preserved (the crate enables `serde_json`'s `preserve_order` feature), so
  token order follows the authored order and the merge rules, never hash order.
- Missing-document requests are de-duplicated and sorted by resource id.
- `RuntimeLimits` caps the work of every call: source documents and expansion steps, the depth of
  nested references, the hops in a reference chain, the JSON nodes references may copy, and the
  number of tokens. Resolved values and token paths are also bounded in depth, and nothing
  recurses without a bound, so hostile or accidental input terminates with an error. Build limits
  with `RuntimeLimits::default()` and its `with_*` setters; see
  [Errors, limits and diagnostics](11-errors-limits-and-diagnostics.md#runtime-limits).
- Catalog mutations are atomic. Invalid input leaves the catalog as it was.

## Crates and features

| Crate | Kind | Contents |
|---|---|---|
| `dxn_dtcg` | library | `DtcgRuntime`, catalog, resolver, value validation, projection traits, source inspection, envelopes, diagnostics |
| `dxn_dtcg_derive` | proc-macro | `#[derive(DtcgTokens)]`, `#[derive(DtcgComponentTokens)]` |

`dxn_dtcg` depends on the derive crate and re-exports both macros, so applications depend only
on `dxn_dtcg`.

| Feature | Default | Adds |
|---|---|---|
| `diagnostics` | off | `DtcgDiagnostic` and `DtcgDiagnosticCode`, plus `AffectedResolution::nodes_visited()` |

Runtime dependencies are `serde`, `serde_json` (with `preserve_order`), `sha2` and `thiserror`. The
derive crate depends on `syn`, `quote` and `proc-macro2`.

## Platform support

| Platform | Status |
|---|---|
| Linux x86_64 | CI: `cargo fmt`, Clippy with `-D warnings`, tests, `cargo package` |
| Windows x86_64 | CI: same as Linux |
| `wasm32-unknown-unknown` | CI: `cargo check --workspace --target wasm32-unknown-unknown` |
| macOS, iOS, Android, other `std` targets | Expected to work; not covered by CI |

The crates:

- contain no `unsafe` code, build scripts, platform-conditional code or FFI;
- need `std` (`Arc`, `BTreeMap`, `HashSet`); `no_std` is not supported;
- start no threads and need no async runtime, clock, randomness or environment access;
- need Rust **1.95** or newer and use edition 2024. The repository pins the toolchain in
  `rust-toolchain.toml`.

`DtcgRuntime`, `ResolvedTokens`, `ResolvedToken` and `DtcgError` are `Send + Sync`. All resolution
methods take `&self`, so one runtime can be shared behind an `Arc` and resolved from several threads
at once. Mutating methods take `&mut self`; to update while others read, use a lock or a
copy-on-write handle. `DtcgRuntime` is `Clone`, and cloning is cheap because document bodies are
reference-counted.

## RayX-specific conveniences (opt-in)

The crate is UI-neutral and does not depend on RayX. It does carry a few conveniences that RayX
needs and other users can ignore. The two value extensions are **off by default**; a runtime
accepts them only when created with
`RuntimeOptions::default().with_extensions(ValueExtensions::rayx())`, as RayX and the RayX Token
Editor do ([value types](05-value-types.md#strict-values-and-the-rayx-extensions)):

- the `dev.rayx.blur` value type, `{ "kind": "layer" | "background", "radius": <dimension> }`, used
  by the RayX Figma export ([value types](05-value-types.md#devrayxblur-rayx-extension));
- typography `lineHeight` given as a `dimension` as well as the standard unitless number.

The other conveniences need no options:

- `ResolverEnvelope`, `DtcgRuntime::from_envelope` (or `from_envelope_with_options`) and
  `DtcgRuntime::replace_envelope` for the Token Editor's `.tokeneditor` project files; after an
  edit, `replace_envelope` re-parses the envelope with the same checks and replaces the stored
  resolver ([document loading](07-document-loading.md#token-editor-envelopes));
- `LocalReferencePolicy`, the relative-local-reference rule shared by RayX themes and the Token
  Editor.

## Glossary

| Term | Meaning |
|---|---|
| **Token** | A JSON object with `$value` (or a token-level `$ref`). It has no children. Identified by its token path. |
| **Group** | Any other object in a token document. Groups nest and can carry `$type`, `$description`, `$deprecated`, `$extensions`, `$root` and `$extends`. |
| **Token path** | The dot-joined names from the source root to a token, such as `color.brand.primary`, or `spacing.$root` for a root token. |
| **Alias** | A reference from one token's value to another token: `"{path}"` or `{ "$ref": "#/path/$value" }`. |
| **Resolver** | A document with `version: "2025.10"` and a `resolutionOrder`, which combines sources from sets and modifiers. |
| **Source** | One entry in a set's `sources` or a modifier context: an inline token document, a reference to a file or fragment, a reference to a set, or a nested resolver. |
| **Set** | A named, always-applied, ordered list of sources. |
| **Modifier** | A named switch with **contexts**, each holding its own sources, and an optional **default** context. |
| **Selection** | The caller's `&[(modifier, context)]` choice. Modifiers left out use their default. |
| **Merged document** | The one token structure all active sources fold into, in order: groups merge, a later token with the same path replaces the earlier one. Types, `$extends` and pointers are evaluated on it. |
| **Resource id** | A logical, normalized, relative document name such as `themes/dark.json`. It identifies a catalog entry. |
| **Catalog** | The runtime's map from resource ids to document text. It has a revision counter. |
| **Document request** | A `DocumentRequest` in `Resolution::Missing`, asking the caller for one document. |
| **Projection** | Converting resolved tokens into a Rust struct through `DtcgTokens`. |
| **Occurrence** | A `SourceOccurrence`: one terminal source, as authored, together with where it came from. Used by the inspection APIs. |
| **Fingerprint** | The SHA-256 digest of a resolved token set's paths, types, values and sources, as a hex string. Metadata does not contribute. |
| **Extensions** | Opt-in value shapes beyond DTCG 2025.10, selected with `ValueExtensions` in `RuntimeOptions`. Not to be confused with `$extensions` metadata. |
