# 13. API reference

Every public item of `dtcg_runtime`, grouped by purpose. All items are exported from the crate
root. Signatures are shown in simplified form, without `pub`.

- [Runtime](#runtime): `DtcgRuntime`
- [Results](#results): `Resolution`, `DocumentRequest`, `ResolvedTokens`, `ResolvedToken`, `AffectedResolution`
- [Configuration](#configuration): `RuntimeLimits`
- [Documents and references](#documents-and-references): `ResourceId`, `DocumentReference`, `DocumentCatalog`, `ResolverEnvelope`, `LocalReferencePolicy`
- [Modifiers](#modifiers): `ModifierAxis`
- [Projection](#projection): `DtcgTokens`, `DtcgComponentTokens`, `FromDtcgValue`, `TypeRegistry`, `join_token_path`, derive macros
- [Values](#values): `validate_value`, `ValueValidationError`, `BLUR_TYPE`
- [Inspection](#inspection): `SourceOccurrence`, `SourceOrigin`, `SourceReference`, `set_sources`, `modifier_contexts`
- [Errors](#errors): `DtcgError`
- [Diagnostics](#diagnostics-feature-diagnostics) (feature `diagnostics`): `DtcgDiagnostic`, `DtcgDiagnosticCode`

Throughout, a *selection* is `&[(&str, &str)]`, a list of `(modifier, context)` pairs.

## Runtime

### `DtcgRuntime`

```rust,ignore
#[derive(Clone)]
struct DtcgRuntime { /* catalog, root, limits, registry */ }
impl Default for DtcgRuntime
```

The entry point: a document catalog, a root document id, `RuntimeLimits` and a `TypeRegistry`.
It is `Send + Sync`, and cheap to clone because documents are reference-counted. It does not
implement `Debug`.

#### Construction

| Signature | Description |
|---|---|
| `fn default() -> Self` | An empty runtime with default limits and no root |
| `fn with_limits(limits: RuntimeLimits) -> Self` | An empty runtime with the given limits |
| `fn from_json(resource_id: impl AsRef<str>, json: impl Into<Arc<str>>) -> Result<Self, DtcgError>` | An empty runtime with one document, which is also the root |
| `fn from_envelope(resource_id: impl AsRef<str>, json: &str) -> Result<(Self, ResolverEnvelope), DtcgError>` | A runtime whose root is the resolver inside a `.tokeneditor` envelope, stored under `resource_id` |

#### Catalog

| Signature | Description |
|---|---|
| `fn set_root(&mut self, resource_id: impl AsRef<str>) -> Result<(), DtcgError>` | Names the root document. It need not be provided yet. |
| `fn provide_json(&mut self, resource_id: impl AsRef<str>, json: impl Into<Arc<str>>) -> Result<(), DtcgError>` | Inserts or replaces a document. Checks the id and JSON syntax. |
| `fn provide_requested_json(&mut self, request: &DocumentRequest, json: impl Into<Arc<str>>) -> Result<(), DtcgError>` | `provide_json` for `request.resource_id` |
| `fn provide_json_batch<I, Id, Json>(&mut self, documents: I) -> Result<(), DtcgError>` where `I: IntoIterator<Item = (Id, Json)>, Id: AsRef<str>, Json: Into<Arc<str>>` | Inserts several documents atomically, as one revision |
| `fn replace_json(&mut self, resource_id: impl AsRef<str>, json: impl Into<Arc<str>>) -> Result<(), DtcgError>` | Replaces an existing document |
| `fn remove_document(&mut self, resource_id: impl AsRef<str>) -> Result<(), DtcgError>` | Removes an existing document |
| `fn contains_document(&self, resource_id: impl AsRef<str>) -> Result<bool, DtcgError>` | Whether a document is in the catalog. `Err` only for an invalid id. |
| `fn catalog_revision(&self) -> u64` | A counter that goes up on every successful mutation |

#### Resolution

| Signature | Description |
|---|---|
| `fn resolve(&self, selection: &[(&str, &str)]) -> Result<Resolution<ResolvedTokens>, DtcgError>` | Resolves the root for a selection |
| `fn project<T: DtcgTokens>(&self, selection: &[(&str, &str)]) -> Result<Resolution<T>, DtcgError>` | `resolve`, then `T::project` |
| `fn registry(&self) -> &TypeRegistry` | The conversion registry, for `T::project(&tokens, runtime.registry())` |
| `fn modifiers(&self) -> Result<Resolution<Vec<ModifierAxis>>, DtcgError>` | The root resolver's modifiers, in resolution order, then the remaining root modifiers in authored order. Empty for a token-document root. |
| `fn normalize_selection(&self, selection: &[(&str, &str)]) -> Result<Resolution<Vec<(String, String)>>, DtcgError>` | Fills in defaults, validates, and orders a selection by axis |
| `fn validate_complete(&self) -> Result<Resolution<()>, DtcgError>` | Acquires and structurally checks the documents of every context, without resolving values |
| `fn replace_json_and_resolve_affected(&mut self, resource_id: impl AsRef<str>, json: impl Into<Arc<str>>, selection: &[(&str, &str)], changed_paths: &[&str]) -> Result<Resolution<AffectedResolution>, DtcgError>` | Replaces a document and resolves only the changed tokens and their dependents. Restores the previous text unless the result is `Ready`. |

#### Inspection

| Signature | Description |
|---|---|
| `fn source_references(&self) -> Result<Resolution<Vec<SourceReference>>, DtcgError>` | Every authored source `$ref` in the resolver graph, dormant ones included |
| `fn inspect_source(&self, source: &serde_json::Value, selection: &[(&str, &str)]) -> Result<Resolution<Vec<SourceOccurrence>>, DtcgError>` | Expands a source into its terminal occurrences in the root's context |
| `fn validate_source(&self, source: &serde_json::Value) -> Result<Resolution<()>, DtcgError>` | Structural discovery of a source across all its contexts |
| `fn preview_source(&self, source: &serde_json::Value, selection: &[(&str, &str)]) -> Result<Resolution<ResolvedTokens>, DtcgError>` | Resolves with the source's occurrences applied last |
| `fn preview_occurrence(&self, source: &SourceOccurrence, selection: &[(&str, &str)]) -> Result<Resolution<ResolvedTokens>, DtcgError>` | Resolves with one occurrence applied last |

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
}
```

The outcome of an operation that may need more documents. `Missing` lists them, de-duplicated and
sorted by resource id.

### `DocumentRequest`

```rust,ignore
#[derive(Clone, Debug, Eq, PartialEq)]
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
struct ResolvedTokens { /* tokens by path, resolution order, fingerprint */ }
```

| Signature | Description |
|---|---|
| `fn get(&self, path: &str) -> Option<&ResolvedToken>` | The token at a dotted path |
| `fn iter(&self) -> impl Iterator<Item = &ResolvedToken>` | All tokens in resolution order |
| `fn len(&self) -> usize` / `fn is_empty(&self) -> bool` | Token count |
| `fn fingerprint(&self) -> &str` | Hex SHA-256 over path, type, value and source of every token, in order. Empty for `ResolvedTokens::default()`. |
| `fn apply_affected(&mut self, affected: &AffectedResolution)` | Merges an affected re-resolution: replaces existing tokens, appends new ones, refreshes the fingerprint |

### `ResolvedToken`

```rust,ignore
#[derive(Clone, Debug, PartialEq)]
struct ResolvedToken {
    path: Arc<str>,                  // dotted token path
    token_type: Option<Arc<str>>,    // effective DTCG type; always Some after resolution
    authored_value: serde_json::Value, // the $value as written (the token object for a token-level $ref)
    value: serde_json::Value,        // fully resolved and validated value
    extensions: serde_json::Value,   // the token's $extensions, or Null
    source: ResourceId,              // document of the winning definition
}
```

All fields are public.

### `AffectedResolution`

```rust,ignore
#[derive(Clone, Debug, Default)]
struct AffectedResolution { /* … */ }
```

| Signature | Description |
|---|---|
| `fn iter(&self) -> impl Iterator<Item = &ResolvedToken>` | The affected tokens in resolution order |
| `fn nodes_visited(&self) -> usize` | *(feature `diagnostics`)* Tokens resolved, including unaffected dependencies |

## Configuration

### `RuntimeLimits`

```rust,ignore
#[derive(Clone, Debug)]
struct RuntimeLimits {
    max_documents: usize,       // default 256
    max_reference_depth: usize, // default 64
    max_alias_depth: usize,     // default 64
}
impl Default for RuntimeLimits
```

All fields are public. See [Runtime limits](11-errors-limits-and-diagnostics.md#runtime-limits).

## Documents and references

### `ResourceId`

```rust,ignore
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
struct ResourceId(/* Arc<str> */);
impl Display for ResourceId
```

| Signature | Description |
|---|---|
| `fn parse(value: impl AsRef<str>) -> Result<Self, DtcgError>` | Parses and normalizes a logical id. A fragment is dropped. |
| `fn resolve(&self, value: impl AsRef<str>) -> Result<DocumentReference, DtcgError>` | Resolves a reference relative to this document's directory |
| `fn as_str(&self) -> &str` | The normalized id |

### `DocumentReference`

```rust,ignore
#[derive(Clone, Debug, Eq, PartialEq)]
struct DocumentReference { /* … */ }
```

| Signature | Description |
|---|---|
| `fn resource_id(&self) -> &ResourceId` | The target document |
| `fn fragment(&self) -> Option<&str>` | The JSON Pointer fragment without `#` (for example `/palette`), if any |
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
| `fn parse(resource: &ResourceId, json: &str) -> Result<Self, DtcgError>` | Parses `{ "data": <resolver> }`, checks the version and applies `LocalReferencePolicy::check_resolver` |
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

## Modifiers

### `ModifierAxis`

```rust,ignore
#[derive(Clone, Debug, Eq, PartialEq)]
struct ModifierAxis {
    name: String,
    contexts: Vec<String>,   // authored order
    default: Option<String>,
}
```

All fields are public. Returned by `DtcgRuntime::modifiers`.

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

Implemented for `String` (JSON strings), `f64` and `f32` (JSON numbers) and `bool` (JSON
booleans). A mismatch is `DtcgError::Projection` with `expected string value`,
`expected numeric value` or `expected boolean value`.

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
| struct | `#[dtcg(id = "name")]` | `component_id()` (component derive only; defaults to the struct name) |
| field | `#[dtcg(path = "a.b")]` | Token path relative to the struct's base |
| field | `#[dtcg(rename = "name")]` | Token name, when `path` is not given |
| field | `#[dtcg(with = "path::to::fn")]` | Converter `fn(&ResolvedToken) -> Result<FieldType, DtcgError>` |
| field | `#[dtcg(nested)]` | Field type is `DtcgTokens`, projected at the field's path |
| field | `#[dtcg(rest)]` | `Default::default()`, reads nothing; at most one |

Type parameters receive a `DtcgTokens` bound. See [Typed projection](08-typed-projection.md).

## Values

### `validate_value`

```rust,ignore
fn validate_value(token_type: &str, value: &serde_json::Value) -> Result<(), ValueValidationError>
```

Checks a resolved value against one of the 13 DTCG 2025.10 types or `dev.rayx.blur`.

### `ValueValidationError`

```rust,ignore
#[derive(Clone, Debug, Eq, PartialEq)]
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

## Inspection

### `SourceOccurrence`

```rust,ignore
#[derive(Clone, Debug)]
struct SourceOccurrence { /* … */ }
```

| Signature | Description |
|---|---|
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
structural checks. `resource` appears in errors only.

## Errors

### `DtcgError`

```rust,ignore
#[derive(Debug, thiserror::Error)]
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
    Projection { path: String, message: String },
    TokenType { resource: ResourceId, path: String, message: String },
    InvalidEnvelope { resource: ResourceId, message: String },
    InvalidReference { resource: ResourceId, reference: String, message: String },
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
enum DtcgDiagnosticCode {
    InvalidResourceId, InvalidPointer, InvalidJson, InvalidDocument, UnknownDocument,
    DuplicateDocument, LimitExceeded, DocumentCycle, Token, Projection, Envelope, Reference,
    Selection,
}
```

The mapping from `DtcgError` is in
[The `diagnostics` feature](11-errors-limits-and-diagnostics.md#the-diagnostics-feature).
