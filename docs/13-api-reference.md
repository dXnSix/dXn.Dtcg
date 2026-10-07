# 13. API reference

Every public item of `dtcg_runtime`, grouped by purpose. All items are exported from the crate
root, except the typed value models, which live in the public `dtcg_runtime::values` module.
Signatures are shown in simplified form, without `pub`.

- [Runtime](#runtime): `DtcgRuntime`
- [Results](#results): `Resolution`, `DocumentRequest`, `ResolvedTokens`, `ResolvedToken`, `ResolvedGroup`, `Deprecation`, `AffectedResolution`
- [Configuration](#configuration): `RuntimeOptions`, `RuntimeLimits`, `ValueExtensions`
- [Documents and references](#documents-and-references): `ResourceId`, `DocumentReference`, `DocumentCatalog`, `ResolverEnvelope`, `LocalReferencePolicy`
- [Modifiers and selections](#modifiers-and-selections): `ModifierAxis`, `selection_from_json`, `selection_refs`
- [Projection](#projection): `DtcgTokens`, `DtcgComponentTokens`, `FromDtcgValue`, `TypeRegistry`, `join_token_path`, derive macros
- [Values](#values): `validate_value`, `validate_value_with`, `ValueValidationError`, `BLUR_TYPE`, the `values` module
- [Inspection](#inspection): `SourceOccurrence`, `SourceOrigin`, `SourceReference`, `set_sources`, `modifier_contexts`
- [Errors](#errors): `DtcgError`
- [Diagnostics](#diagnostics-feature-diagnostics) (feature `diagnostics`): `DtcgDiagnostic`, `DtcgDiagnosticCode`

Throughout, a *selection* is `&[(&str, &str)]`, a list of `(modifier, context)` pairs.

Types marked `#[non_exhaustive]` may gain fields or variants in minor releases: match enums with a
wildcard arm, and build structs through their constructors and setters rather than struct literals.

## Runtime

### `DtcgRuntime`

```rust,ignore
#[derive(Clone)]
struct DtcgRuntime { /* catalog, root, options, registry */ }
impl Default for DtcgRuntime
impl Debug for DtcgRuntime
```

The entry point: a document catalog, a root document id, `RuntimeOptions` and a `TypeRegistry`.
It is `Send + Sync`, and cheap to clone because documents are reference-counted. Its `Debug`
output is redacted: it prints the root, the options, the catalog revision and the document ids,
never document bodies.

#### Construction

| Signature | Description |
|---|---|
| `fn default() -> Self` | An empty runtime with default options (default limits, strict values) and no root |
| `fn with_limits(limits: RuntimeLimits) -> Self` | An empty runtime with the given limits and strict values |
| `fn with_options(options: RuntimeOptions) -> Self` | An empty runtime with the given limits and value extensions |
| `fn from_json(resource_id: impl AsRef<str>, json: impl Into<Arc<str>>) -> Result<Self, DtcgError>` | A runtime with default options and one document, which is also the root |
| `fn from_json_with_options(resource_id: impl AsRef<str>, json: impl Into<Arc<str>>, options: RuntimeOptions) -> Result<Self, DtcgError>` | `from_json` with `options` |
| `fn from_envelope(resource_id: impl AsRef<str>, json: &str) -> Result<(Self, ResolverEnvelope), DtcgError>` | A runtime with default options whose root is the resolver inside a `.tokeneditor` envelope, stored under `resource_id` |
| `fn from_envelope_with_options(resource_id: impl AsRef<str>, json: &str, options: RuntimeOptions) -> Result<(Self, ResolverEnvelope), DtcgError>` | `from_envelope` with `options` |

#### Configuration

| Signature | Description |
|---|---|
| `fn options(&self) -> &RuntimeOptions` | The limits and value extensions this runtime applies |
| `fn limits(&self) -> &RuntimeLimits` | The limits every resolution, discovery and inspection call applies |
| `fn registry(&self) -> &TypeRegistry` | The conversion registry, for `T::project(&tokens, runtime.registry())` |

Options are fixed at construction.

#### Catalog

| Signature | Description |
|---|---|
| `fn set_root(&mut self, resource_id: impl AsRef<str>) -> Result<(), DtcgError>` | Names the root document. It need not be provided yet. |
| `fn provide_json(&mut self, resource_id: impl AsRef<str>, json: impl Into<Arc<str>>) -> Result<(), DtcgError>` | Inserts or replaces a document. Checks the id and JSON syntax. |
| `fn provide_requested_json(&mut self, request: &DocumentRequest, json: impl Into<Arc<str>>) -> Result<(), DtcgError>` | `provide_json` for `request.resource_id` |
| `fn provide_json_batch<I, Id, Json>(&mut self, documents: I) -> Result<(), DtcgError>` where `I: IntoIterator<Item = (Id, Json)>, Id: AsRef<str>, Json: Into<Arc<str>>` | Inserts several documents atomically, as one revision |
| `fn replace_json(&mut self, resource_id: impl AsRef<str>, json: impl Into<Arc<str>>) -> Result<(), DtcgError>` | Replaces an existing document |
| `fn replace_envelope(&mut self, resource_id: impl AsRef<str>, json: &str) -> Result<ResolverEnvelope, DtcgError>` | Re-parses a `.tokeneditor` envelope with the same checks as `from_envelope` and replaces the stored resolver of the existing document `resource_id`. `UnknownDocument` if it was never provided; an invalid envelope leaves the catalog unchanged. |
| `fn remove_document(&mut self, resource_id: impl AsRef<str>) -> Result<(), DtcgError>` | Removes an existing document |
| `fn contains_document(&self, resource_id: impl AsRef<str>) -> Result<bool, DtcgError>` | Whether a document is in the catalog. `Err` only for an invalid id. |
| `fn catalog_revision(&self) -> u64` | A counter that goes up on every successful mutation |

#### Resolution

| Signature | Description |
|---|---|
| `fn resolve(&self, selection: &[(&str, &str)]) -> Result<Resolution<ResolvedTokens>, DtcgError>` | Resolves the root for a selection |
| `fn project<T: DtcgTokens>(&self, selection: &[(&str, &str)]) -> Result<Resolution<T>, DtcgError>` | `resolve`, then `T::project` |
| `fn modifiers(&self) -> Result<Resolution<Vec<ModifierAxis>>, DtcgError>` | The root resolver's modifiers only, in `resolutionOrder` order, then the remaining root modifiers in authored order, with `$ref` step overrides applied. Empty for a token-document root. |
| `fn normalize_selection(&self, selection: &[(&str, &str)]) -> Result<Resolution<Vec<(String, String)>>, DtcgError>` | Validates a selection exactly as `resolve` does, across every resolver the root graph reaches, and returns the chosen context of every reached modifier in declaration order (root first), with the declared spellings. Equal selections normalize to equal vectors. |
| `fn validate_complete(&self) -> Result<Resolution<()>, DtcgError>` | Acquires and structurally checks the documents of every context, without resolving values |
| `fn replace_json_and_resolve_affected(&mut self, resource_id: impl AsRef<str>, json: impl Into<Arc<str>>, selection: &[(&str, &str)], changed_paths: &[&str]) -> Result<Resolution<AffectedResolution>, DtcgError>` | Replaces a document and re-resolves only the tokens the edit can change (see [Incremental resolution](09-incremental-resolution.md)). Restores the previous text unless the result is `Ready`. |

#### Inspection

| Signature | Description |
|---|---|
| `fn source_references(&self) -> Result<Resolution<Vec<SourceReference>>, DtcgError>` | Every authored source `$ref` in the resolver graph, dormant ones included |
| `fn inspect_source(&self, source: &serde_json::Value, selection: &[(&str, &str)]) -> Result<Resolution<Vec<SourceOccurrence>>, DtcgError>` | Expands a source into its terminal occurrences in the root's context. Inputs may name modifiers of the source, the root or any reached resolver. |
| `fn validate_source(&self, source: &serde_json::Value) -> Result<Resolution<()>, DtcgError>` | Structural discovery of a source across all its contexts |
| `fn preview_source(&self, source: &serde_json::Value, selection: &[(&str, &str)]) -> Result<Resolution<ResolvedTokens>, DtcgError>` | Resolves with the source's occurrences merged last. Inputs may name the source's own modifiers. |
| `fn preview_occurrence(&self, source: &SourceOccurrence, selection: &[(&str, &str)]) -> Result<Resolution<ResolvedTokens>, DtcgError>` | Resolves with one occurrence merged last. The selection applies to the root graph only. |

## Results

### `Resolution<T>`

```rust,ignore
#[derive(Clone, Debug, PartialEq)]
enum Resolution<T> {
    Ready(T),
    Missing(Vec<DocumentRequest>),
}

impl<T> Resolution<T> {
    fn ready(self) -> Option<T>;
    fn map<U>(self, function: impl FnOnce(T) -> U) -> Resolution<U>;
    fn is_ready(&self) -> bool;
    fn is_missing(&self) -> bool;
    fn missing(&self) -> Option<&[DocumentRequest]>;
}
```

The outcome of an operation that may need more documents. `Missing` lists them, de-duplicated and
sorted by resource id. `map` converts a ready value and keeps the requests; `missing` returns them,
or `None` when ready.

### `DocumentRequest`

```rust,ignore
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
struct DocumentRequest {
    resource_id: ResourceId,          // the normalized id to provide
    referenced_from: ResourceId,      // the document containing the reference
    original_reference: Arc<str>,     // the reference as written, including any fragment
}
```

All fields are public.

### `ResolvedTokens`

```rust,ignore
#[derive(Clone, Debug, Default, PartialEq)]
struct ResolvedTokens { /* tokens by path, resolution order, groups, fingerprint */ }
```

| Signature | Description |
|---|---|
| `fn get(&self, path: &str) -> Option<&ResolvedToken>` | The token at a dotted path |
| `fn iter(&self) -> impl Iterator<Item = &ResolvedToken>` | All tokens in resolution order: authored order across the merged sources, each path at its last definition |
| `fn len(&self) -> usize` / `fn is_empty(&self) -> bool` | Token count |
| `fn group(&self, path: &str) -> Option<&ResolvedGroup>` | The group at a dotted path (`""` for the merged root), when it contains at least one resolved token |
| `fn groups(&self) -> impl Iterator<Item = &ResolvedGroup>` | Every group that contains a resolved token, in path order |
| `fn fingerprint(&self) -> &str` | Hex SHA-256 over path, type, canonical value (object keys sorted) and source of every token, in order. Metadata does not contribute. Empty for `ResolvedTokens::default()`. |
| `fn to_document(&self) -> serde_json::Value` | The resolution as a nested DTCG token document (see below) |
| `fn apply_affected(&mut self, affected: &AffectedResolution)` | Applies an affected re-resolution of the same selection: removes the removed paths, replaces and adds the re-resolved tokens, adopts the new order and groups, and refreshes the fingerprint. The result equals a full `resolve` of the edited documents. |

Equality ignores the member order of object values.

`to_document` writes tokens in resolution order, each with an explicit `$type`, its resolved
`$value` (no references) and its own `$description` and `$extensions`. `$deprecated` is written
where it differs from what the written groups imply. `$root` tokens are written as `$root`
members. Groups keep their own `$description`, `$deprecated` and `$extensions`, and the root keeps
its merged `$extensions`. No `$schema` is written. Resolving the document again yields the same
paths, types, values and metadata; an interleaved resolution order comes back grouped by group.

```rust
use dtcg_runtime::DtcgRuntime;

let runtime = DtcgRuntime::from_json(
    "tokens.json",
    r#"{ "space": { "$type": "dimension", "sm": { "$value": { "value": 4, "unit": "px" } },
                    "md": { "$value": "{space.sm}" } } }"#,
)
.unwrap();
let tokens = runtime.resolve(&[]).unwrap().ready().unwrap();
let document = tokens.to_document();

assert_eq!(document["space"]["md"]["$type"], "dimension");
assert_eq!(document["space"]["md"]["$value"]["value"], 4);

let again = DtcgRuntime::from_json("export.json", document.to_string()).unwrap();
let exported = again.resolve(&[]).unwrap().ready().unwrap();
assert_eq!(exported.get("space.md").unwrap().value, tokens.get("space.md").unwrap().value);
```

### `ResolvedToken`

```rust,ignore
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
struct ResolvedToken {
    path: Arc<str>,                    // dotted token path
    token_type: Option<Arc<str>>,      // effective DTCG type; always Some after resolution
    authored_value: serde_json::Value, // the $value as written (the token object for a token-level $ref)
    value: serde_json::Value,          // fully resolved and validated value
    extensions: serde_json::Value,     // the token's own $extensions, or Null; never inherited
    description: Option<Arc<str>>,     // the token's own $description; never inherited
    deprecated: Deprecation,           // own $deprecated, else the nearest group's
    source: ResourceId,                // document of the winning definition
}

impl ResolvedToken {
    fn typed(&self, extensions: &ValueExtensions) -> Result<TypedValue, DtcgError>;
}
```

All fields are public. A token copied by `$extends` names the document of that `$extends` as its
`source`. `typed` converts `value` by the effective type into a [`TypedValue`](#typedvalue),
validating it with `extensions`; a failure is `DtcgError::Projection`.

### `ResolvedGroup`

```rust,ignore
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
struct ResolvedGroup {
    path: Arc<str>,                 // dotted group path; empty for the merged root
    token_type: Option<Arc<str>>,   // the $type its tokens inherit: its own or the nearest ancestor's
    description: Option<Arc<str>>,  // its own $description
    deprecated: Option<Deprecation>, // its own $deprecated, when it declares one
    extensions: serde_json::Value,  // its merged $extensions, or Null
}
```

A group of the merged token structure, with its properties after merge and `$extends`. Groups
produced by `$extends` carry the extended group's properties. All fields are public.

### `Deprecation`

```rust,ignore
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
enum Deprecation {
    #[default]
    NotDeprecated,                // no $deprecated applies, or it is false
    Deprecated,                   // $deprecated: true
    DeprecatedWithReason(Arc<str>), // $deprecated: "reason"
}

impl Deprecation {
    fn is_deprecated(&self) -> bool;
    fn reason(&self) -> Option<&str>;
    fn to_json(&self) -> serde_json::Value; // false, true or the reason string
}
```

A token inherits the nearest group's `$deprecated` unless it declares its own; `false` ends an
inherited deprecation.

### `AffectedResolution`

```rust,ignore
#[derive(Clone, Debug, Default)]
struct AffectedResolution { /* … */ }
```

| Signature | Description |
|---|---|
| `fn iter(&self) -> impl Iterator<Item = &ResolvedToken>` | The re-resolved tokens, in the new resolution order |
| `fn removed(&self) -> &[String]` | The token paths the edit removed |
| `fn order(&self) -> &[String]` | Every token path after the edit, in resolution order |
| `fn nodes_visited(&self) -> usize` | *(feature `diagnostics`)* Tokens resolved, including unaffected dependencies |

Apply it to a snapshot with `ResolvedTokens::apply_affected`.

## Configuration

### `RuntimeOptions`

```rust,ignore
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
struct RuntimeOptions {
    limits: RuntimeLimits,         // default RuntimeLimits::default()
    extensions: ValueExtensions,   // default: none, strict DTCG 2025.10 values
}

impl RuntimeOptions {
    fn with_limits(self, limits: RuntimeLimits) -> Self;
    fn with_extensions(self, extensions: ValueExtensions) -> Self;
}
```

How a runtime bounds its work and validates values. All fields are public. Pass it to
`DtcgRuntime::with_options`, `from_json_with_options` or `from_envelope_with_options`.

### `RuntimeLimits`

```rust,ignore
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
struct RuntimeLimits {
    max_documents: usize,       // default 256
    max_reference_depth: usize, // default 64
    max_alias_depth: usize,     // default 64
    max_value_nodes: usize,     // default 4,000,000
    max_tokens: usize,          // default 100,000
}
impl Default for RuntimeLimits

impl RuntimeLimits {
    fn with_max_documents(self, limit: usize) -> Self;
    fn with_max_reference_depth(self, limit: usize) -> Self;
    fn with_max_alias_depth(self, limit: usize) -> Self;
    fn with_max_value_nodes(self, limit: usize) -> Self;
    fn with_max_tokens(self, limit: usize) -> Self;
}
```

All fields are public. Build it from `RuntimeLimits::default()` with the setters. What each limit
counts, and the fixed value-depth and path-depth bounds: [Runtime
limits](11-errors-limits-and-diagnostics.md#runtime-limits).

### `ValueExtensions`

```rust,ignore
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
struct ValueExtensions {
    line_height_dimension: bool, // typography lineHeight may be (or alias) a dimension
    rayx_blur: bool,             // the dev.rayx.blur type
}

impl ValueExtensions {
    fn rayx() -> Self; // both on: the RayX and Token Editor profile
    fn with_line_height_dimension(self, enabled: bool) -> Self;
    fn with_rayx_blur(self, enabled: bool) -> Self;
}
```

All fields are public and default to `false`. See [Strict by default, RayX extensions on
request](12-conformance.md#strict-by-default-rayx-extensions-on-request).

```rust
use dtcg_runtime::{DtcgRuntime, RuntimeLimits, RuntimeOptions, ValueExtensions};

let options = RuntimeOptions::default()
    .with_limits(RuntimeLimits::default().with_max_tokens(10_000))
    .with_extensions(ValueExtensions::default().with_rayx_blur(true));
let runtime = DtcgRuntime::with_options(options.clone());

assert_eq!(runtime.options(), &options);
assert_eq!(runtime.limits().max_tokens, 10_000);
assert!(!runtime.options().extensions.line_height_dimension);
```

## Documents and references

### `ResourceId`

```rust,ignore
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
struct ResourceId(/* Arc<str> */);
impl Display for ResourceId
```

| Signature | Description |
|---|---|
| `fn parse(value: impl AsRef<str>) -> Result<Self, DtcgError>` | Parses and normalizes a logical id. An id containing `#` is rejected, as are non-portable segments (see [`InvalidResourceId`](11-errors-limits-and-diagnostics.md#dtcgerror)). |
| `fn resolve(&self, value: impl AsRef<str>) -> Result<DocumentReference, DtcgError>` | Resolves a reference (`path#/fragment`, or `#/fragment` for this document) relative to this document's directory, keeping the fragment |
| `fn as_str(&self) -> &str` | The normalized id |

### `DocumentReference`

```rust,ignore
#[derive(Clone, Debug, Eq, PartialEq)]
struct DocumentReference { /* … */ }
```

| Signature | Description |
|---|---|
| `fn resource_id(&self) -> &ResourceId` | The target document |
| `fn fragment(&self) -> Option<&str>` | The JSON Pointer fragment without `#` (for example `/palette`), as written, if any |
| `fn original(&self) -> &str` | The reference as written |

### `DocumentCatalog`

```rust,ignore
#[derive(Clone, Default)]
struct DocumentCatalog { /* … */ }
```

`fn new() -> Self`, `fn len(&self) -> usize`, `fn is_empty(&self) -> bool`. The runtime's catalog is
managed through `DtcgRuntime`. This type exposes no other public operations.

### `ResolverEnvelope`

```rust,ignore
#[derive(Clone, Debug)]
struct ResolverEnvelope { /* … */ }
```

| Signature | Description |
|---|---|
| `fn parse(resource: &ResourceId, json: &str) -> Result<Self, DtcgError>` | Parses `{ "data": <resolver> }`, checks the version, applies `LocalReferencePolicy::check_resolver` and validates the resolver structure |
| `fn name(&self) -> Option<&str>` | `data.name` |
| `fn resolver_json(&self) -> &Arc<str>` | The resolver as JSON text, to provide as the root |

### `LocalReferencePolicy`

```rust,ignore
struct LocalReferencePolicy;
```

| Signature | Description |
|---|---|
| `fn check(resource: &ResourceId, reference: &str) -> Result<(), DtcgError>` | Rejects empty references, absolute paths, backslashes and URI schemes (drive letters included) |
| `fn check_resolver(resource: &ResourceId, resolver: &serde_json::Value) -> Result<(), DtcgError>` | Applies `check` to every `$ref` in `sets.*.sources` and `modifiers.*.contexts.*` |

## Modifiers and selections

### `ModifierAxis`

```rust,ignore
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
struct ModifierAxis {
    name: String,
    contexts: Vec<String>,   // authored order
    default: Option<String>,
}
```

All fields are public. Returned by `DtcgRuntime::modifiers`, with `$ref` step overrides (such as a
`default`) applied.

### `selection_from_json` and `selection_refs`

```rust,ignore
fn selection_from_json(inputs: &serde_json::Value) -> Result<Vec<(String, String)>, DtcgError>;
fn selection_refs(selection: &[(String, String)]) -> Vec<(&str, &str)>;
```

`selection_from_json` reads resolver inputs such as `{"theme": "dark"}` into owned pairs, in
member order. Anything but a JSON object of strings is `DtcgError::InvalidInput`.
`selection_refs` borrows owned pairs as the selection every resolving method takes.

```rust
use dtcg_runtime::{DtcgError, selection_from_json, selection_refs};
use serde_json::json;

let inputs = selection_from_json(&json!({ "theme": "dark", "density": "compact" })).unwrap();
assert_eq!(selection_refs(&inputs), [("theme", "dark"), ("density", "compact")]);

assert!(matches!(
    selection_from_json(&json!(["theme", "dark"])),
    Err(DtcgError::InvalidInput { .. })
));
```

Modifier and context names match case-insensitively when there is no exact match; see
[Selections](06-resolvers-and-modifiers.md#selections).

## Projection

### `DtcgTokens`

```rust,ignore
trait DtcgTokens: Sized {
    fn project_at(tokens: &ResolvedTokens, registry: &TypeRegistry, root: &str)
        -> Result<Self, DtcgError>;
    fn project(tokens: &ResolvedTokens, registry: &TypeRegistry) -> Result<Self, DtcgError> {
        Self::project_at(tokens, registry, "")
    }
}
```

A typed view of the token set, rooted at a dotted path. Usually derived.

### `DtcgComponentTokens`

```rust,ignore
trait DtcgComponentTokens: DtcgTokens {
    fn component_id() -> &'static str;
}
```

### `FromDtcgValue`

```rust,ignore
trait FromDtcgValue: Sized + 'static {
    fn from_dtcg(token: &ResolvedToken) -> Result<Self, DtcgError>;
}
```

Implemented for `String` (JSON strings), `f64` and `f32` (JSON numbers), `bool` (JSON booleans),
the typed models of the [`values`](#the-values-module) module and `TypedValue`. A mismatch is
`DtcgError::Projection` naming the token path, with `expected string value`,
`expected numeric value`, `expected boolean value`, or `expected a <type> value` for the models.
`f32` also fails for a number outside the finite `f32` range
(`expected numeric value within f32 range`). The models accept the RayX extension shapes, because
the runtime has already validated the value with its own options.

### `TypeRegistry`

```rust,ignore
#[derive(Clone, Default)]
struct TypeRegistry;

impl TypeRegistry {
    fn convert<T: FromDtcgValue>(&self, token: &ResolvedToken) -> Result<T, DtcgError>;
}
```

A unit struct that forwards to `FromDtcgValue`. It can be written as `TypeRegistry` or
`TypeRegistry::default()`.

### `join_token_path`

```rust,ignore
fn join_token_path(root: &str, path: &str) -> String
```

Joins two dotted paths with `.`, ignoring empty parts: `("", "a") → "a"`, `("a", "") → "a"`,
`("a", "b.c") → "a.b.c"`.

### Derive macros

`#[derive(DtcgTokens)]` and `#[derive(DtcgComponentTokens)]`, re-exported from
`dtcg_runtime_derive`. Both accept structs with named fields only.

| Position | Attribute | Effect |
|---|---|---|
| struct | `#[dtcg(root = "path")]` | Prefix for every field path |
| struct | `#[dtcg(id = "name")]` | `component_id()`; component derive only (defaults to the struct name) |
| field | `#[dtcg(path = "a.b")]` | Token path relative to the struct's base |
| field | `#[dtcg(rename = "name")]` | Token name, when `path` is not given |
| field | `#[dtcg(with = "path::to::fn")]` | Converter `fn(&ResolvedToken) -> Result<FieldType, DtcgError>` |
| field | `#[dtcg(nested)]` | Field type is `DtcgTokens`, projected at the field's path |
| field | `#[dtcg(rest)]` | `Default::default()`, reads nothing; at most one |

Without `path` or `rename`, the token name is the field name, with a raw identifier unrawed
(`r#type` reads `type`). A leaf field of type `Option<T>` (also written `core::option::Option<T>`
or `std::option::Option<T>`, with or without `with`) is `None` when the token is missing and
converted when it is present; any other missing leaf fails with `Projection` and
`required token is missing`.

Attributes are parsed strictly. These are compile errors: unknown or duplicated keys, a struct
attribute on a field or a field attribute on the struct, `id` on `DtcgTokens`, `nested` with
`with`, `path` with `rename`, `rest` with `path`, `rename`, `with` or `nested`, a value on the
`nested` or `rest` flags, a `with` that is not a path, enums and tuple structs.

The generated code names every item by its full path, so local aliases such as
`type Result<T> = …` do not affect it. Type parameters get bounds per field: `FromDtcgValue` for
the value type of a leaf field without `with`, `DtcgTokens` for a `nested` field and `Default` for a
`rest` field. See [Typed projection](08-typed-projection.md).

## Values

### `validate_value` and `validate_value_with`

```rust,ignore
fn validate_value(token_type: &str, value: &serde_json::Value) -> Result<(), ValueValidationError>;

fn validate_value_with(
    token_type: &str,
    value: &serde_json::Value,
    extensions: &ValueExtensions,
) -> Result<(), ValueValidationError>;
```

`validate_value` checks a resolved value against one of the 13 DTCG 2025.10 types, strictly.
`validate_value_with` also accepts the shapes `extensions` enables: a `dimension` typography
`lineHeight`, and the `dev.rayx.blur` type.

```rust
use dtcg_runtime::{BLUR_TYPE, ValueExtensions, ValueValidationError, validate_value, validate_value_with};
use serde_json::json;

let blur = json!({ "kind": "layer", "radius": { "value": 8, "unit": "px" } });
assert_eq!(
    validate_value(BLUR_TYPE, &blur),
    Err(ValueValidationError::UnsupportedType(BLUR_TYPE.into()))
);
assert!(validate_value_with(BLUR_TYPE, &blur, &ValueExtensions::rayx()).is_ok());
```

### `ValueValidationError`

```rust,ignore
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
enum ValueValidationError {
    UnsupportedType(String), // "unsupported DTCG type `…` for final 2025.10"
    InvalidValue(String),    // "resolved value does not match DTCG type `…`"
}
impl Display for ValueValidationError
impl std::error::Error for ValueValidationError
```

### `BLUR_TYPE`

```rust,ignore
const BLUR_TYPE: &str = "dev.rayx.blur";
```

### The `values` module

`dtcg_runtime::values` holds typed Rust models of resolved values. They are plain data with public
fields, parsed from JSON that validation already accepted, and they apply the consumer rules of the
specification.

| Type | Shape |
|---|---|
| `Color { color_space: ColorSpace, components: [Component; 3], alpha: f64, hex: Option<String> }` | `alpha` is `1.0` when absent; `hex` is the authored fallback |
| `ColorSpace` | The 14 color spaces: `Srgb`, `SrgbLinear`, `Hsl`, `Hwb`, `Lab`, `Lch`, `Oklab`, `Oklch`, `DisplayP3`, `A98Rgb`, `ProphotoRgb`, `Rec2020`, `XyzD65`, `XyzD50`; `fn name(self) -> &'static str` returns the DTCG name |
| `Component` | `Number(f64)` or `None` (the `"none"` keyword) |
| `Dimension { value: f64, unit: DimensionUnit }` | `DimensionUnit::{Px, Rem}`; `fn to_px(&self, rem_base: f64) -> f64` |
| `Duration { value: f64, unit: DurationUnit }` | `DurationUnit::{Ms, S}`; `fn to_millis(&self) -> f64` |
| `FontFamily(Vec<String>)` | One name, or names in fallback order |
| `FontWeight(f64)` | Keywords mapped to numbers (`thin`/`hairline` 100 … `extra-black`/`ultra-black` 950); `fn keyword(name: &str) -> Option<f64>` |
| `CubicBezier([f64; 4])` | `[x1, y1, x2, y2]` |
| `StrokeStyle` | `Style(LineStyle)` or `Dashes { dash_array: Vec<Dimension>, line_cap: LineCap }`; `LineStyle::{Solid, Dashed, Dotted, Double, Groove, Ridge, Outset, Inset}`, `LineCap::{Round, Butt, Square}` |
| `Border { color: Color, width: Dimension, style: StrokeStyle }` | |
| `Transition { duration: Duration, delay: Duration, timing_function: CubicBezier }` | |
| `Shadow(Vec<ShadowLayer>)` | A single shadow object is one layer |
| `ShadowLayer { color, offset_x, offset_y, blur, spread, inset: bool }` | `inset` is `false` when absent |
| `Gradient(Vec<GradientStop>)` | |
| `GradientStop { color: Color, position: f64 }` | `position` clamped to [0, 1] |
| `Typography { font_family, font_size, font_weight, letter_spacing, line_height: LineHeight }` | |
| `LineHeight` | `Multiplier(f64)`, or `Dimension(Dimension)` with the `line_height_dimension` extension |
| `Blur { kind: BlurKind, radius: Dimension }` | `BlurKind::{Layer, Background}`; the `dev.rayx.blur` extension |

#### `TypedValue`

```rust,ignore
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
enum TypedValue {
    Color(Color), Dimension(Dimension), FontFamily(FontFamily), FontWeight(FontWeight),
    Duration(Duration), CubicBezier(CubicBezier), Number(f64), StrokeStyle(StrokeStyle),
    Border(Border), Transition(Transition), Shadow(Shadow), Gradient(Gradient),
    Typography(Typography), Blur(Blur),
}

impl TypedValue {
    fn from_resolved(
        token_type: &str,
        value: &serde_json::Value,
        extensions: &ValueExtensions,
    ) -> Result<Self, ValueValidationError>;
}
```

A resolved value converted by its DTCG type. `from_resolved` validates with `extensions`, then
converts. `ResolvedToken::typed` is the usual way to get one:

```rust
use dtcg_runtime::{
    DtcgRuntime, ValueExtensions,
    values::{Dimension, DimensionUnit, TypedValue},
};

let runtime = DtcgRuntime::from_json(
    "tokens.json",
    r#"{ "gap": { "$type": "dimension", "$value": { "value": 0.5, "unit": "rem" } } }"#,
)
.unwrap();
let tokens = runtime.resolve(&[]).unwrap().ready().unwrap();
let gap = tokens.get("gap").unwrap().typed(&ValueExtensions::default()).unwrap();

assert_eq!(gap, TypedValue::Dimension(Dimension { value: 0.5, unit: DimensionUnit::Rem }));
if let TypedValue::Dimension(gap) = gap {
    assert_eq!(gap.to_px(16.0), 8.0);
}
```

## Inspection

### `SourceOccurrence`

```rust,ignore
#[derive(Clone, Debug)]
struct SourceOccurrence { /* … */ }
```

| Signature | Description |
|---|---|
| `fn resource(&self) -> &ResourceId` | The document that holds this source. An inline source supplied to `inspect_source` reports the root resolver's id. |
| `fn value(&self) -> &serde_json::Value` | The authored JSON of this terminal source, with reference overrides applied |
| `fn origin(&self) -> &SourceOrigin` | Where the JSON lives |
| `fn member_origins(&self) -> &BTreeMap<String, SourceOrigin>` | Owners of top-level members replaced by reference overrides |

### `SourceOrigin`

```rust,ignore
#[derive(Clone, Debug, Eq, PartialEq)]
struct SourceOrigin { /* … */ }
```

| Signature | Description |
|---|---|
| `fn resource(&self) -> Option<&ResourceId>` | The document, or `None` for a source passed inline to `inspect_source` |
| `fn pointer(&self) -> &str` | JSON Pointer within that document, without `#`; empty for the root |

### `SourceReference`

```rust,ignore
#[derive(Clone, Debug)]
struct SourceReference { /* … */ }
```

| Signature | Description |
|---|---|
| `fn origin(&self) -> &SourceOrigin` | The location of the `$ref` string |
| `fn reference(&self) -> &DocumentReference` | The resolved target |

### `set_sources` and `modifier_contexts`

```rust,ignore
fn set_sources<'a>(resource: &ResourceId, definition: &'a serde_json::Value)
    -> Result<&'a [serde_json::Value], DtcgError>;

fn modifier_contexts<'a>(resource: &ResourceId, definition: &'a serde_json::Value)
    -> Result<&'a serde_json::Map<String, serde_json::Value>, DtcgError>;
```

Read a set definition's `sources` array or a modifier definition's `contexts`, with the runtime's
structural checks: a modifier needs at least two contexts, each an array, and a `default` must name
one of them. `resource` appears in errors only.

## Errors

### `DtcgError`

```rust,ignore
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[non_exhaustive]
enum DtcgError {
    InvalidResourceId(String),
    InvalidPointer(String),
    InvalidJson { resource: ResourceId, message: String },
    InvalidDocument { resource: ResourceId, message: String },
    UnknownDocument(ResourceId),
    DuplicateDocument(ResourceId),
    LimitExceeded { kind: &'static str, limit: usize },
    DocumentCycle(String),
    Token { resource: ResourceId, path: String, message: String },
    UnresolvedReference { resource: ResourceId, path: String, target: String, message: String },
    Projection { path: String, message: String },
    TokenType { resource: ResourceId, path: String, message: String },
    InvalidEnvelope { resource: ResourceId, message: String },
    InvalidReference { resource: ResourceId, reference: String, message: String },
    InvalidInput { message: String },
    UnknownModifier { modifier: String, known: Vec<String> },
    UnknownContext { modifier: String, context: String, allowed: Vec<String>, message: String },
}
```

When each variant occurs: [`DtcgError`](11-errors-limits-and-diagnostics.md#dtcgerror).

## Diagnostics (feature `diagnostics`)

### `DtcgDiagnostic`

```rust,ignore
#[derive(Clone, Debug, Eq, PartialEq)]
struct DtcgDiagnostic { /* … */ }
```

| Signature | Description |
|---|---|
| `fn from_error(error: &DtcgError) -> Self` | A redacted diagnostic: code, resource, path and message, never document text |
| `fn code(&self) -> DtcgDiagnosticCode` | The stable category |
| `fn resource(&self) -> Option<&ResourceId>` | The resource, when the error has one |
| `fn path(&self) -> Option<&str>` | A token path, limit kind, modifier or reference, depending on the code |
| `fn message(&self) -> &str` | The message |

### `DtcgDiagnosticCode`

```rust,ignore
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
enum DtcgDiagnosticCode {
    InvalidResourceId, InvalidPointer, InvalidJson, InvalidDocument, UnknownDocument,
    DuplicateDocument, LimitExceeded, DocumentCycle, Token, UnresolvedReference, Projection,
    Envelope, Reference, Selection, InvalidInput,
}
```

The mapping from `DtcgError` is in
[The `diagnostics` feature](11-errors-limits-and-diagnostics.md#the-diagnostics-feature).
