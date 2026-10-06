# 12. DTCG 2025.10 conformance

This chapter summarizes how `dtcg_runtime` relates to the DTCG 2025.10 format and resolver
modules: what is supported, what it adds, where it makes implementation-specific choices or is
stricter than you might expect, and what it does not do. Each point links to the chapter with the
details.

The runtime is a **resolver**. It reads documents and computes token values. It is not a schema
validator for authoring metadata, and it does not write documents.

- [Supported](#supported)
- [Extensions](#extensions)
- [Implementation-specific and strict behavior](#implementation-specific-and-strict-behavior)
- [Not supported](#not-supported)
- [Where conformance is tested](#where-conformance-is-tested)

## Supported

### Format module

| Feature | Status | Details |
|---|---|---|
| Tokens (`$value`) and nested groups | Supported | [Token documents](03-token-documents.md#structure) |
| `$type` on tokens and groups, with inheritance | Supported | [Types and inheritance](03-token-documents.md#types-and-inheritance) |
| `$description`, `$deprecated` | Accepted, not exposed | [Metadata](03-token-documents.md#metadata-description-deprecated-extensions) |
| `$extensions` | Accepted; token-level exposed as `ResolvedToken::extensions` | [Metadata](03-token-documents.md#metadata-description-deprecated-extensions) |
| `$root` tokens | Supported | [Root tokens](03-token-documents.md#root-tokens-root) |
| `$extends` group extension | Supported within a source | [Group extension](03-token-documents.md#group-extension-extends) |
| Name restrictions (`{`, `}`, `.`, `$`) | Enforced | [Names](03-token-documents.md#names) |
| Curly-brace aliases, whole and nested in composites | Supported | [Curly-brace aliases](04-aliases-and-references.md#curly-brace-aliases) |
| JSON Pointer `$ref`, at value, property and token level | Supported | [JSON Pointer references](04-aliases-and-references.md#json-pointer-references) |
| Reference objects with sibling overrides | Supported | [Adjusting a referenced value](04-aliases-and-references.md#adjusting-a-referenced-value) |
| Alias type inheritance | Supported | [Curly-brace aliases](04-aliases-and-references.md#curly-brace-aliases) |
| Alias and pointer cycle detection | Supported | [Cycles](04-aliases-and-references.md#cycles-depth-and-errors) |
| All 13 types, validated after resolution | Supported | [Value types](05-value-types.md) |

### Resolver module

| Feature | Status | Details |
|---|---|---|
| `version: "2025.10"` | Required, exactly | [Resolver structure](06-resolvers-and-modifiers.md#resolver-structure) |
| Sets | Supported | [Sets](06-resolvers-and-modifiers.md#sets) |
| Modifiers with contexts and an optional default | Supported | [Modifiers](06-resolvers-and-modifiers.md#modifiers-and-contexts) |
| `resolutionOrder` with `$ref` and inline steps | Supported | [`resolutionOrder`](06-resolvers-and-modifiers.md#resolutionorder) |
| Sources: inline, file, fragment, set, local pointer | Supported | [Sources](06-resolvers-and-modifiers.md#sources) |
| Source references with shallow overrides | Supported | [Reference objects](06-resolvers-and-modifiers.md#reference-objects-with-overrides) |
| Nested resolvers | Supported; selections are shared | [Nested resolvers](06-resolvers-and-modifiers.md#nested-resolvers) |
| Whole-token overlay, later wins | Supported | [Overlay semantics](06-resolvers-and-modifiers.md#overlay-semantics) |
| Modifier inputs with defaults | Supported | [Selections](06-resolvers-and-modifiers.md#selections) |

## Extensions

These are additions made for RayX. They are additive: documents that do not use them behave as
standard 2025.10.

| Extension | What it adds | Details |
|---|---|---|
| `dev.rayx.blur` type | `{ "kind": "layer" \| "background", "radius": dimension }` for Figma blur effects | [Value types](05-value-types.md#devrayxblur-rayx-extension) |
| Typography `lineHeight` as a `dimension` | Line heights given as px or rem tokens, besides the unitless number | [`typography`](05-value-types.md#typography) |
| `.tokeneditor` envelope | `{ "data": <resolver> }` project files for the RayX Token Editor | [Envelopes](07-document-loading.md#token-editor-envelopes) |
| `LocalReferencePolicy` | An opt-in rule that allows only relative local references | [`LocalReferencePolicy`](07-document-loading.md#localreferencepolicy) |

## Implementation-specific and strict behavior

These are choices the runtime makes that are worth knowing when moving token files from other
tools.

**Token documents**

- Unknown `$`-prefixed members are errors. That includes a `$schema` member in token documents,
  which some tools add. Resolver documents may contain any extra members.
- Every token must end up with one of the supported types. Unknown types, the pre-2025 `string`
  type and custom types are errors, not passed through.
- `$extends` resolves only within the source that contains it, not across documents.
- A token-level `$ref` takes its type from its own `$type` or from a `…/$value` target, not from an
  enclosing group's `$type`.
- `$description` and `$deprecated` are not type-checked.

**Aliases and references**

- A JSON string is an alias exactly when it starts with `{` and ends with `}`. There is no string
  interpolation, and a literal string in that shape cannot be expressed.
- Any object with a `$ref` member inside a `$value` is a reference, at any depth.
- An alias to a group (`{spacing}`) is an error. Root tokens are referenced as `{spacing.$root}`.
- A pointer to `…/$value` or `…/$type` of a token resolves against the final token set (like an
  alias). Other pointers read the raw JSON of the source that contains them. See
  [How a pointer is interpreted](04-aliases-and-references.md#how-a-pointer-is-interpreted).

**Overlay and resolvers**

- An overriding token does not inherit the type of the token it replaces; it needs its own
  (see [Overlay semantics](06-resolvers-and-modifiers.md#overlay-semantics)).
- One selection applies to every resolver in the graph. Each explicitly selected modifier must be
  declared by every resolver discovery passes through, and by the root when previewing.
- Every declared root modifier needs a selected or default context, even if `resolutionOrder` does
  not reference it.
- Selections are ignored when the root is a plain token document.
- Token order follows authored JSON order, and an overridden token moves to its last definition.

**Values**

- Numeric ranges are checked exactly as listed in [Value types](05-value-types.md). Negative
  dimensions and durations are accepted. Gradient stop positions are not range-checked or required
  to be sorted. A color's `hex` is format-checked but not compared with its components.

## Not supported

- **Remote or absolute references.** Resource ids are relative and logical. URLs and absolute paths
  are rejected, and loading is up to the caller, who may of course fetch from anywhere.
- **Other versions.** Resolver documents must declare exactly `2025.10`. Drafts and older formats,
  including the legacy `value`/`type` keys without `$`, are not read.
- **Deep merging.** Composite values and groups are never merged; overrides replace whole tokens.
- **Removing tokens** through overrides.
- **Writing documents.** There is no serializer or formatter. `authored_value` and the inspection
  APIs provide what an editor needs to write its own changes.

## Where conformance is tested

| Test group | Covers |
|---|---|
| `crates/dtcg_runtime/tests/dtcg_2025_10_conformance.rs` | Groups, `$root`, `$extends`, effective types, all 13 value types, JSON Pointers, names, ordering, fingerprints |
| `crates/dtcg_runtime/tests/resolver_2025_10.rs` | Resolver version, modifier inputs, inline steps, local reference overrides, ordering, set references |
| `crates/dtcg_runtime/tests/alias_overlay.rs` | Aliases resolving against the final overlay |
| `crates/dtcg_runtime/tests/rayx_export_types.rs` | The RayX value-shape extensions |

Run one group with `cargo test -p dtcg_runtime --all-features --test dtcg_2025_10_conformance`.
