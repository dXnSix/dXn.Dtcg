# 12. DTCG 2025.10 conformance

This chapter summarizes how `dtcg_runtime` relates to the DTCG 2025.10 Format, Color and Resolver
modules: how conformance is measured, what is supported, the choices the runtime makes where the
specification leaves room or contradicts itself, the opt-in RayX extensions, and what it does not
do. Each point links to the chapter with the details.

The runtime is a **resolver**. It reads documents, computes token values and can export a
resolution as a token document. It is not an authoring tool: it does not format or rewrite the
documents it reads.

- [Official conformance corpus](#official-conformance-corpus)
- [Conformance tiers](#conformance-tiers)
- [Supported](#supported)
- [Strict by default, RayX extensions on request](#strict-by-default-rayx-extensions-on-request)
- [Implementation-defined choices](#implementation-defined-choices)
- [Not supported](#not-supported)
- [Where conformance is tested](#where-conformance-is-tested)

## Official conformance corpus

The runtime passes the whole official DTCG 2025.10 conformance corpus:

```text
official corpus: 250 passed, 0 known gaps
```

| | |
|---|---|
| Upstream | `design-tokens/community-group`, directory `test-suite/tests/2025.10/` |
| Pinned commit | `882ebd6716abef9a46d8ef5fb12e82009d6cee2e` |
| Fixtures | 220 format and 30 resolver fixtures, each a positive or negative evaluation test |
| Vendored at | `crates/dtcg_runtime/tests/fixtures/dtcg-test-suite/2025.10/`, byte-identical to upstream, excluded from the published crate |
| Harness | `crates/dtcg_runtime/tests/official_test_suite.rs` |

The harness evaluates every manifest entry with the default `RuntimeOptions` (strict values):

- A **format fixture** is accepted when `DtcgRuntime::from_json` succeeds and `resolve(&[])` is
  `Ready`.
- A **resolver fixture** is accepted when `validate_complete()` is `Ready` and `resolve` then
  succeeds with each modifier's default context, or its first context when it has no default.
- A positive fixture must be accepted. A negative fixture must fail with an `Err`; `Missing` and
  panics never count as a pass.

The `KNOWN_GAPS` list in the harness is a ratchet. It must equal the set of fixtures whose
expectation the runtime does not meet, so a new failure fails the test, and so does a fixed gap that
is still listed. It is empty today. The vendored corpus has its own `README.md` that describes how
to update it to a newer upstream commit.

Run the harness with:

```bash
cargo test -p dtcg_runtime --all-features --test official_test_suite -- --nocapture
```

## Conformance tiers

Where sources disagree, the runtime follows them in this order:

1. **The published specification text**: the Format, Color and Resolver modules of 2025.10,
   including their errata.
2. **The official corpus**, where the text is silent or ambiguous.
3. **The runtime's own decisions** for the conflicts that remain. Each is listed under
   [Implementation-defined choices](#implementation-defined-choices) and pinned by a test.

## Supported

### Format and Color modules

| Feature | Status | Details |
|---|---|---|
| Tokens (`$value`) and nested groups | Supported | [Token documents](03-token-documents.md#structure) |
| `$schema` | Accepted and ignored at the root of a whole document | [Token documents](03-token-documents.md) |
| `$type` on tokens and groups, with inheritance | Supported, after merging all sources | [Types and inheritance](03-token-documents.md#types-and-inheritance) |
| Type of a reference (Format §5.2.2) | Own `$type`, else the target's type for a whole-value reference, else the group's | [Types and inheritance](03-token-documents.md#types-and-inheritance) |
| `$description`, `$deprecated` | Exposed on `ResolvedToken` and `ResolvedGroup`; `$deprecated` is inherited from groups | [Metadata](03-token-documents.md#metadata-description-deprecated-extensions) |
| `$extensions` | Exposed on `ResolvedToken` (own) and `ResolvedGroup` (merged) | [Metadata](03-token-documents.md#metadata-description-deprecated-extensions) |
| `$root` tokens | Supported | [Root tokens](03-token-documents.md#root-tokens-root) |
| `$extends` group extension | Supported, across sources and on groups produced by `$extends` | [Group extension](03-token-documents.md#group-extension-extends) |
| Name restrictions (`{`, `}`, `.`, `$`, non-empty) | Enforced | [Names](03-token-documents.md#names) |
| Curly-brace aliases, whole and at composite members | Supported at the positions Format §7 and §9 allow | [Curly-brace aliases](04-aliases-and-references.md#curly-brace-aliases) |
| JSON Pointer `$ref`, at value, property and token level | Supported (RFC 6901, percent-decoded) | [JSON Pointer references](04-aliases-and-references.md#json-pointer-references) |
| Reference objects with sibling overrides | Supported | [Adjusting a referenced value](04-aliases-and-references.md#adjusting-a-referenced-value) |
| Alias and pointer cycle detection | Supported | [Cycles](04-aliases-and-references.md#cycles-depth-and-errors) |
| All 13 types, validated after resolution | Supported | [Value types](05-value-types.md) |
| All 14 color spaces with their component ranges | Supported | [`color`](05-value-types.md#color) |
| Typed Rust models of every type | `dtcg_runtime::values`, `ResolvedToken::typed` | [Value types](05-value-types.md#reading-values-in-rust) |

### Resolver module

| Feature | Status | Details |
|---|---|---|
| `version: "2025.10"` | Required, exactly | [Resolver structure](06-resolvers-and-modifiers.md#resolver-structure) |
| Allowed resolver members | Enforced on the root, sets, modifiers and inline steps | [Resolver structure](06-resolvers-and-modifiers.md#resolver-structure) |
| Sets | Supported | [Sets](06-resolvers-and-modifiers.md#sets) |
| Modifiers with contexts and an optional default | Supported; at least two contexts | [Modifiers](06-resolvers-and-modifiers.md#modifiers-and-contexts) |
| `resolutionOrder` with `$ref` and inline steps | Supported; must not be empty | [`resolutionOrder`](06-resolvers-and-modifiers.md#resolutionorder) |
| Sources: inline, file, fragment, set, local pointer | Supported | [Sources](06-resolvers-and-modifiers.md#sources) |
| Source references with shallow overrides | Supported | [Reference objects](06-resolvers-and-modifiers.md#reference-objects-with-overrides) |
| Merging sources into one token structure (§6.2–6.3) | Groups merge member by member; tokens replace whole | [Resolvers and modifiers](06-resolvers-and-modifiers.md) |
| Inputs (§5) with defaults | Supported; names match case-insensitively | [Selections](06-resolvers-and-modifiers.md#selections) |
| Resolved output as a token document (§8) | `ResolvedTokens::to_document()` | [API reference](13-api-reference.md#resolvedtokens) |

## Strict by default, RayX extensions on request

With the default `RuntimeOptions`, values are validated strictly against DTCG 2025.10.
`validate_value` is always strict. The RayX value shapes are opt-in through `ValueExtensions`,
which `validate_value_with` and the runtime options accept:

| `ValueExtensions` field | Default | Adds | Details |
|---|---|---|---|
| `line_height_dimension` | `false` | Typography `lineHeight` as a `dimension`, or an alias to a `dimension` token, besides the specification's number | [`typography`](05-value-types.md#typography) |
| `rayx_blur` | `false` | The `dev.rayx.blur` type: `{ "kind": "layer" \| "background", "radius": dimension }`, whose `radius` may alias a `dimension` token | [Value types](05-value-types.md#devrayxblur-rayx-extension) |

`ValueExtensions::rayx()` turns both on. RayX and the RayX Token Editor opt in with it:

```rust
use dtcg_runtime::{DtcgRuntime, RuntimeOptions, ValueExtensions};

let document = r#"{
  "body": { "$type": "typography", "$value": {
    "fontFamily": "Inter", "fontSize": { "value": 14, "unit": "px" }, "fontWeight": 400,
    "letterSpacing": { "value": 0, "unit": "px" }, "lineHeight": { "value": 20, "unit": "px" }
  } }
}"#;

// Strict DTCG 2025.10: `lineHeight` must be a number.
let strict = DtcgRuntime::from_json("tokens.json", document).unwrap();
assert!(strict.resolve(&[]).is_err());

// The RayX profile accepts the dimension.
let rayx = DtcgRuntime::from_json_with_options(
    "tokens.json",
    document,
    RuntimeOptions::default().with_extensions(ValueExtensions::rayx()),
)
.unwrap();
assert!(rayx.resolve(&[]).unwrap().is_ready());
```

Two further additions do not change how standard documents behave:

| Addition | What it adds | Details |
|---|---|---|
| `.tokeneditor` envelope | `{ "data": <resolver> }` project files for the RayX Token Editor | [Envelopes](07-document-loading.md#token-editor-envelopes) |
| `LocalReferencePolicy` | An opt-in rule that allows only relative local references | [`LocalReferencePolicy`](07-document-loading.md#localreferencepolicy) |

## Implementation-defined choices

These are the tier-3 decisions: places where the specification is silent, leaves a choice to
implementations, or contradicts itself. They are worth knowing when moving token files from other
tools.

**References**

- A token-level `$ref` points at the target's `…/$value`, as in Format Example 32 and the corpus.
  A pointer to the token object itself (the form of Example 21) is an error; no `/$value` is
  appended implicitly.
- A `…/$value` pointer whose prefix is not a token is an ordinary document pointer, and the token
  then takes its group's type.
- JSON Pointer fragments are percent-decoded before `~1` and `~0` are unescaped (RFC 6901 §6), in
  resource fragments and in value-level and token-level `$ref`s alike. `lib.json#/brand%20colors`
  addresses the member `brand colors`; an unencoded `#/brand colors` keeps working when the target
  exists. Array indexes are `0` or have no leading zero or sign.
- An alias to a group (`{spacing}`) is an error (`UnresolvedReference`). A group's root token is
  referenced as `{spacing.$root}`.
- A JSON string is an alias exactly when it starts with `{` and ends with `}`. There is no string
  interpolation.

**Values**

- `xyz-d65` and `xyz-d50` components are bounded to [0, 1], like the RGB spaces.
- Hue components (`hsl`, `hwb`, `lch`, `oklch`) must lie in [0, 360). They are never wrapped.
- A color's `hex` is matched case-insensitively. It is format-checked, not compared with the
  components.
- Gradient stop positions outside [0, 1] are valid. The resolved JSON keeps them as written; the
  typed `values::Gradient` clamps them to [0, 1]. Stops need not be sorted.
- Numeric ranges are otherwise checked exactly as listed in [Value types](05-value-types.md).
  Negative dimensions and durations are accepted.

**Resolver documents and inputs**

- A SHOULD in the Resolver module becomes an error only where the official JSON schema and the
  corpus enforce it: a modifier with a single context, an unknown resolver member and an empty
  `resolutionOrder` are `InvalidDocument` errors. The recommendation that inputs match
  case-insensitively is implemented as behavior: an exact match wins, then names are compared in
  lowercase, and several candidates are an error.
- Every declared root modifier needs a selected or default context, even if `resolutionOrder` does
  not reference it.
- Selections are ignored when the resolver graph declares no modifiers, for example when the root
  is a plain token document.

**Nested resolvers** (a runtime extension: the Resolver module does not define a resolver used as
a source)

- A source may reference another resolver document. Its own `resolutionOrder` is expanded in place.
- One selection serves the whole graph. An input is valid when any resolver the graph reaches
  declares it. Each resolver applies the inputs that name its own modifiers and its defaults for
  the rest, and requires inputs only for its own modifiers without a default.
- `modifiers()` lists the root resolver's axes only; `normalize_selection` lists the chosen context
  of every reached modifier, root first.

See [Nested resolvers](06-resolvers-and-modifiers.md#nested-resolvers).

**Merging sources**

- Groups merge member by member. Group `$type`, `$description`, `$deprecated` and `$extends` are
  replaced per key: a later source's value wins, and a property it omits keeps the earlier value.
- Group `$extensions` objects merge recursively.
- A token replaces any earlier token at its path whole; it keeps nothing of the earlier
  definition. Composite values are never merged.
- A path that is a token in one source and a group in another is an error (`Token`).
- Type inheritance, `$extends`, `$deprecated` inheritance and every JSON Pointer are evaluated on
  the merged structure, so an override inherits its group's `$type` from an earlier source.

**Token order**

- The specification leaves token order to implementations. `ResolvedTokens::iter()` follows
  authored order across the merged sources, with each path at its last definition. Tokens copied by
  `$extends` take the position of the extending group's `$extends`. See
  [Token order](03-token-documents.md#token-order).

## Not supported

These optional (MAY) features and neighbors of the specification are out of scope:

- **Remote or absolute references.** Resource ids are relative and logical. URLs and absolute paths
  are rejected, and loading is up to the caller, who may fetch from anywhere.
- **JSONC or JSON5 resolver documents.** Every document must be strict JSON.
- **Warnings for names that differ only in case.** Such names are distinct tokens; no warning is
  produced.
- **Aggregated input-error reports.** Resolution stops at the first error.
- **Other versions.** Resolver documents must declare exactly `2025.10`. Drafts and older formats,
  including the legacy `value`/`type` keys without `$`, are not read.
- **Removing tokens** through overrides.

## Where conformance is tested

| Test group | Covers |
|---|---|
| `crates/dtcg_runtime/tests/official_test_suite.rs` | The vendored official corpus with the `KNOWN_GAPS` ratchet, and `$schema` handling |
| `crates/dtcg_runtime/tests/dtcg_2025_10_conformance.rs` | Groups, `$root`, `$extends`, effective types, all 13 value types, JSON Pointers, names, ordering, fingerprints |
| `crates/dtcg_runtime/tests/format_structure.rs` | Token and group structure rules (Format §5–§6) |
| `crates/dtcg_runtime/tests/type_determination.rs` | Effective types and reference type mismatches |
| `crates/dtcg_runtime/tests/reference_positions.rs` | Where aliases may appear and which types they must target |
| `crates/dtcg_runtime/tests/merged_resolution.rs` | Merging sources, `$extends` across sources, pointers on the merged structure |
| `crates/dtcg_runtime/tests/resolver_2025_10.rs`, `resolver_document_validation.rs` | Resolver version, allowed members, steps, overrides, set references |
| `crates/dtcg_runtime/tests/input_selection.rs` | Case-insensitive inputs, nested-resolver inputs, selection errors |
| `crates/dtcg_runtime/tests/value_extensions.rs`, `rayx_export_types.rs` | Strict values and the opt-in RayX extensions |

Run one group with `cargo test -p dtcg_runtime --all-features --test <group>`.
