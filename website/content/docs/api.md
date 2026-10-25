---
title: API reference
description: Every public type, trait, function and method of dxn_dtcg grouped by purpose, with simplified signatures.
group: Reference
order: 170
---

Every public item of `dxn_dtcg`, grouped by purpose. All items are exported from the crate root, except the typed value models, which live in the public `dxn_dtcg::values` module, and the expression types, which live in `dxn_dtcg::expressions` (feature `expressions`). Signatures are simplified (no `pub`). The crate is not published to docs.rs; run `cargo doc --open` in your project for rustdoc of the pinned version.

A *selection* is `&[(&str, &str)]`, a list of `(modifier, context)` pairs. Types marked `#[non_exhaustive]` may gain fields or variants in minor releases: match enums with a wildcard arm and build structs through constructors and setters.

## DtcgRuntime

The entry point: a document catalog, a root id, `RuntimeOptions` and a `TypeRegistry`. It is `Clone` (cheap: documents are reference-counted), `Default`, `Send + Sync`, and its `Debug` output never contains document bodies.

### Construction

| Signature | Description |
|---|---|
| `fn default() -> Self` | An empty runtime, default options (default limits, strict values), no root |
| `fn with_limits(limits: RuntimeLimits) -> Self` | An empty runtime with the given limits and strict values |
| `fn with_options(options: RuntimeOptions) -> Self` | An empty runtime with the given limits, value extensions and, with the `expressions` feature, expression options |
| `fn from_json(resource_id, json) -> Result<Self, DtcgError>` | One document, which is also the root |
| `fn from_json_with_options(resource_id, json, options) -> Result<Self, DtcgError>` | `from_json` with `options` |

`resource_id` is `impl AsRef<str>` and `json` is `impl Into<Arc<str>>` (`&str`, `String` and `Arc<str>` work).

### Configuration

| Signature | Description |
|---|---|
| `fn options(&self) -> &RuntimeOptions` | The limits, value extensions and expression options in use |
| `fn limits(&self) -> &RuntimeLimits` | The limits in use |
| `fn registry(&self) -> &TypeRegistry` | The conversion registry, for `T::project(&tokens, runtime.registry())` |

Options are fixed at construction.

### Catalog

| Signature | Description |
|---|---|
| `fn set_root(&mut self, resource_id) -> Result<(), DtcgError>` | Names the root document; it need not be provided yet |
| `fn provide_json(&mut self, resource_id, json) -> Result<(), DtcgError>` | Inserts or replaces a document; checks the id and JSON syntax |
| `fn provide_requested_json(&mut self, request: &DocumentRequest, json) -> Result<(), DtcgError>` | `provide_json` for `request.resource_id`; a request of `kind: DocumentKind::Module` is a kind mismatch (`InvalidDocument`) |
| `fn provide_json_batch(&mut self, documents: impl IntoIterator<Item = (Id, Json)>) -> Result<(), DtcgError>` | Inserts several documents atomically as one revision |
| `fn replace_json(&mut self, resource_id, json) -> Result<(), DtcgError>` | Replaces an existing document |
| `fn provide_module(&mut self, resource_id, source) -> Result<(), DtcgError>` | Inserts or replaces the source text of a JavaScript (`.js`, `.mjs`) or TypeScript (`.ts`, `.mts`) module; the text is checked only when a resolution imports it |
| `fn provide_requested_module(&mut self, request: &DocumentRequest, source) -> Result<(), DtcgError>` | `provide_module` for `request.resource_id`; a request of `kind: DocumentKind::Json` is a kind mismatch |
| `fn replace_module(&mut self, resource_id, source) -> Result<(), DtcgError>` | Replaces an existing module; affected resolution does not follow module edits |
| `fn remove_document(&mut self, resource_id) -> Result<(), DtcgError>` | Removes an existing JSON document or module |
| `fn contains_document(&self, resource_id) -> Result<bool, DtcgError>` | Whether a document is present; `Err` only for an invalid id |
| `fn catalog_revision(&self) -> u64` | Goes up on every successful mutation |

The module methods exist in every build; only a runtime with expressions enabled reads modules. Every catalog entry has one kind: providing JSON where the catalog holds a module, or the reverse, fails with `InvalidDocument` and changes nothing. See [Loading documents](./loading.md#module-documents).

### Resolution

| Signature | Description |
|---|---|
| `fn resolve(&self, selection) -> Result<Resolution<ResolvedTokens>, DtcgError>` | Resolves the root for a selection |
| `fn project<T: DtcgTokens>(&self, selection) -> Result<Resolution<T>, DtcgError>` | `resolve`, then `T::project` |
| `fn modifiers(&self) -> Result<Resolution<Vec<ModifierAxis>>, DtcgError>` | The root resolver's modifiers; empty for a token-document root |
| `fn normalize_selection(&self, selection) -> Result<Resolution<Vec<(String, String)>>, DtcgError>` | Validates like `resolve` across the graph and returns every reached modifier's chosen context, declared spelling, root first |
| `fn validate_complete(&self) -> Result<Resolution<()>, DtcgError>` | Acquires and structurally checks the documents of every context, without resolving values; with expressions enabled, also requests the modules every context declares and what they import |
| `fn replace_json_and_resolve_affected(&mut self, resource_id, json, selection, changed_paths: &[&str]) -> Result<Resolution<AffectedResolution>, DtcgError>` | See [Incremental resolution](./incremental.md) |

### Inspection

| Signature | Description |
|---|---|
| `fn source_references(&self) -> Result<Resolution<Vec<SourceReference>>, DtcgError>` | Every authored source `$ref`, dormant ones included |
| `fn inspect_source(&self, source: &serde_json::Value, selection) -> Result<Resolution<Vec<SourceOccurrence>>, DtcgError>` | Expands a source into terminal occurrences in the root's context |
| `fn validate_source(&self, source: &serde_json::Value) -> Result<Resolution<()>, DtcgError>` | Structural discovery of a source across all contexts |
| `fn preview_source(&self, source: &serde_json::Value, selection) -> Result<Resolution<ResolvedTokens>, DtcgError>` | Resolves with the source merged last |
| `fn preview_occurrence(&self, source: &SourceOccurrence, selection) -> Result<Resolution<ResolvedTokens>, DtcgError>` | Resolves with one occurrence merged last |

## Results

### Resolution

```rust
enum Resolution<T> { Ready(T), Missing(Vec<DocumentRequest>) }   // Clone, Debug, PartialEq

fn ready(self) -> Option<T>
fn map<U>(self, f: impl FnOnce(T) -> U) -> Resolution<U>
fn is_ready(&self) -> bool
fn is_missing(&self) -> bool
fn missing(&self) -> Option<&[DocumentRequest]>
```

### DocumentRequest

`#[non_exhaustive]`, all fields public: `resource_id: ResourceId` (the normalized id to provide), `referenced_from: ResourceId` (the containing document or importing module), `original_reference: Arc<str>` (as written, fragment included), `kind: DocumentKind` (what to provide).

### DocumentKind

```rust
enum DocumentKind { Json, Module }   // non_exhaustive, Clone, Copy, Debug, Eq, Hash, PartialEq
```

`Json` is a token document or resolver, answered with `provide_requested_json`; `Module` is the source text of a JavaScript or TypeScript module that expressions import, answered with `provide_requested_module`. Without expressions, every request is `Json`.

### ResolvedTokens

`Clone`, `Debug`, `Default`, `PartialEq`; equality ignores the member order of object values.

| Signature | Description |
|---|---|
| `fn get(&self, path: &str) -> Option<&ResolvedToken>` | The token at a dotted path |
| `fn iter(&self) -> impl Iterator<Item = &ResolvedToken>` | All tokens in resolution order |
| `fn len(&self) -> usize` / `fn is_empty(&self) -> bool` | Token count |
| `fn group(&self, path: &str) -> Option<&ResolvedGroup>` | A group (`""` for the merged root) that contains at least one resolved token |
| `fn groups(&self) -> impl Iterator<Item = &ResolvedGroup>` | Those groups in path order |
| `fn fingerprint(&self) -> &str` | Hex SHA-256 over path, type, canonical value and source of every token, in order; metadata does not contribute; empty for `default()` |
| `fn to_document(&self) -> serde_json::Value` | The resolution as a nested DTCG token document ([export](./token-documents.md#export)) |
| `fn apply_affected(&mut self, affected: &AffectedResolution)` | Patches the snapshot; the result equals a full `resolve`, except for expression results that depend on writes to shared [module state](./expressions.md#the-sandbox-and-its-limits) |

### ResolvedToken

`#[non_exhaustive]`, `Clone`, `Debug`, `PartialEq`, all fields public:

```text
path: Arc<str>                     dotted token path
token_type: Option<Arc<str>>       effective DTCG type; always Some after resolution
authored_value: serde_json::Value  the $value as written (the whole object for a token-level $ref)
value: serde_json::Value           fully resolved and validated value (an expression's result when one was evaluated)
snapshot: Option<serde_json::Value>  the resolved $value an evaluated expression replaced; None for other tokens
extensions: serde_json::Value      the token's own $extensions, or Null; never inherited
description: Option<Arc<str>>      the token's own $description; never inherited
deprecated: Deprecation            own $deprecated, else the nearest group's
source: ResourceId                 document of the winning definition
```

`fn typed(&self, extensions: &ValueExtensions) -> Result<TypedValue, DtcgError>` converts `value` by the effective type; failure is `DtcgError::Projection`.

### ResolvedGroup and Deprecation

`ResolvedGroup` (`#[non_exhaustive]`, public fields): `path`, `token_type: Option<Arc<str>>` (own or nearest ancestor's), `description`, `deprecated: Option<Deprecation>` (own), `extensions` (merged, or `Null`).

`Deprecation` is `NotDeprecated` (default), `Deprecated` or `DeprecatedWithReason(Arc<str>)`, with `is_deprecated()`, `reason() -> Option<&str>` and `to_json() -> Value`. It derives `Clone`, `Debug`, `Default`, `Eq`, `Hash`, `PartialEq`.

### AffectedResolution

`Clone`, `Debug`, `Default`. `iter()` (re-resolved tokens in the new order), `removed() -> &[String]`, `order() -> &[String]`, and `nodes_visited() -> usize` with the `diagnostics` feature.

## Configuration

| Type | Fields and setters |
|---|---|
| `RuntimeOptions` (`#[non_exhaustive]`, `Default`, `Clone`, `Debug`, `Eq`) | `limits: RuntimeLimits`, `extensions: ValueExtensions`; `with_limits(..)`, `with_extensions(..)`. With the `expressions` feature also `expressions: Option<ExpressionOptions>` (`None` by default) and `with_expressions(ExpressionOptions)` |
| `RuntimeLimits` (`#[non_exhaustive]`, `Default`) | `max_documents` (256), `max_reference_depth` (64), `max_alias_depth` (64), `max_value_nodes` (4,000,000), `max_tokens` (100,000); `with_max_*` setters. See [Limits](./limits.md) |
| `ValueExtensions` (`#[non_exhaustive]`, `Default`) | `line_height_dimension: bool`, `blur: bool` (both `false`); `all()` turns every value extension on; `with_line_height_dimension(bool)`, `with_blur(bool)` |

## Documents and references

| Type | Members |
|---|---|
| `ResourceId` (`Clone`, `Eq`, `Ord`, `Hash`, `Display`) | `parse(value) -> Result<Self, DtcgError>`, `resolve(&self, value) -> Result<DocumentReference, DtcgError>`, `as_str() -> &str` |
| `DocumentReference` | `resource_id() -> &ResourceId`, `fragment() -> Option<&str>` (without `#`, as written), `original() -> &str` |
| `DocumentCatalog` (`Clone`, `Default`) | `new()`, `len()`, `is_empty()` only; a runtime's catalog is managed through `DtcgRuntime` |
| `LocalReferencePolicy` | `check(resource: &ResourceId, reference: &str) -> Result<(), DtcgError>`, `check_resolver(resource, &Value) -> Result<(), DtcgError>` |

## Modifiers and selections

`ModifierAxis` (`#[non_exhaustive]`): `name: String`, `contexts: Vec<String>` (authored order), `default: Option<String>`. Functions:

```rust
fn selection_from_json(inputs: &serde_json::Value) -> Result<Vec<(String, String)>, DtcgError>
fn selection_refs(selection: &[(String, String)]) -> Vec<(&str, &str)>
```

## Projection

```rust
trait DtcgTokens: Sized {
    fn project_at(tokens: &ResolvedTokens, registry: &TypeRegistry, root: &str) -> Result<Self, DtcgError>;
    fn project(tokens: &ResolvedTokens, registry: &TypeRegistry) -> Result<Self, DtcgError>;   // root ""
}
trait DtcgComponentTokens: DtcgTokens { fn component_id() -> &'static str; }
trait FromDtcgValue: Sized + 'static { fn from_dtcg(token: &ResolvedToken) -> Result<Self, DtcgError>; }
struct TypeRegistry;   // fn convert<T: FromDtcgValue>(&self, token: &ResolvedToken) -> Result<T, DtcgError>
fn join_token_path(root: &str, path: &str) -> String
```

`FromDtcgValue` is implemented for `String`, `f64`, `f32`, `bool`, the `values` models and `TypedValue`. `join_token_path` joins with `.` and ignores empty parts. The derive macros `#[derive(DtcgTokens)]` and `#[derive(DtcgComponentTokens)]` are re-exported from `dxn_dtcg_derive`; attributes are in [Typed projection](./typed-projection.md#attribute-reference).

## Values

```rust
fn validate_value(token_type: &str, value: &Value) -> Result<(), ValueValidationError>
fn validate_value_with(token_type: &str, value: &Value, extensions: &ValueExtensions) -> Result<(), ValueValidationError>
const BLUR_TYPE: &str = "dxn.dtcg.blur"
const EXPRESSION_EXTENSION: &str = "dxn.dtcg.expressions"
enum ValueValidationError { UnsupportedType(String), InvalidValue(String) }   // non_exhaustive, std::error::Error
```

The `dxn_dtcg::values` module holds `Color`, `ColorSpace` (14 variants, `name()`), `Component` (`Number(f64)` or `None`), `Dimension`, `DimensionUnit`, `Duration`, `DurationUnit`, `FontFamily`, `FontWeight`, `CubicBezier`, `StrokeStyle`, `LineStyle`, `LineCap`, `Border`, `Transition`, `Shadow`, `ShadowLayer`, `Gradient`, `GradientStop`, `Typography`, `LineHeight`, `Blur`, `BlurKind` and `TypedValue`, a `#[non_exhaustive]` enum with one variant per type. `TypedValue::from_resolved(token_type, &value, &extensions)` validates and converts. Field-by-field tables are in [Value types](./value-types.md#reading-values-in-rust). `ColorSpace::from_name(name) -> Option<ColorSpace>` is the inverse of `name()`.

`EXPRESSION_EXTENSION` is the `$extensions` key of [expressions](./expressions.md); the constant exists in every build.

### Color operations

Methods on `values::Color`, available without any feature ([Color conversions](./value-types.md#color-conversions)):

| Signature | Description |
|---|---|
| `fn new(color_space: ColorSpace, components: [f64; 3], alpha: f64) -> Color` | A valid color from numeric components (clamped; a non-finite component counts as 0, a non-finite alpha as 1) |
| `fn coordinates(&self) -> [f64; 3]` | The components as numbers, `none` counting as 0 |
| `fn convert(&self, space: ColorSpace) -> Color` | The color in `space`, clamped to its ranges; alpha kept, `hex` dropped |
| `fn mix(&self, other: &Color, amount: f64) -> Color` | `other` mixed in by `amount` in OKLCH |
| `fn mix_in(&self, other: &Color, amount: f64, space: ColorSpace) -> Color` | The same in `space`, premultiplied, shorter hue arc, a powerless hue taking the other color's |
| `fn lighten(&self, amount: f64) -> Color` / `fn darken(&self, amount: f64) -> Color` | OKLCH lightness shifted, in the color's own space |
| `fn with_alpha(&self, alpha: f64) -> Color` | The color with `alpha`, clamped to `[0, 1]` |
| `fn to_hex(&self) -> String` / `fn from_hex(hex: &str) -> Option<Color>` | The `#rrggbb` sRGB form, and the sRGB color of one |
| `fn luminance(&self) -> f64` / `fn contrast(&self, other: &Color) -> f64` | WCAG 2.x relative luminance and contrast ratio (of the colors clipped into sRGB, 1 to 21) |
| `fn in_gamut(&self, space: ColorSpace) -> bool` / `fn clamp_to_gamut(&self, space: ColorSpace) -> Color` | Gamut test (in linear light; sRGB's gamut for HSL and HWB) and clipping |
| `fn to_json(&self) -> Value` | The DTCG JSON form; `alpha` only when it is not 1 |

Every other value model, `ShadowLayer` included, also has `to_json()`. The twelve models of the DTCG types (`Color`, `Dimension`, `Duration`, `CubicBezier`, `FontFamily`, `FontWeight`, `StrokeStyle`, `Border`, `Transition`, `Shadow`, `Gradient` and `Typography`) implement serde's `Serialize` and `Deserialize` with their DTCG JSON forms; deserializing validates the value as its DTCG type with every value extension.

## Inspection types

| Type | Members |
|---|---|
| `SourceOccurrence` | `resource() -> &ResourceId`, `value() -> &Value`, `origin() -> &SourceOrigin`, `member_origins() -> &BTreeMap<String, SourceOrigin>` |
| `SourceOrigin` | `resource() -> Option<&ResourceId>`, `pointer() -> &str` |
| `SourceReference` | `origin() -> &SourceOrigin`, `reference() -> &DocumentReference` |

Free functions `set_sources(resource, &definition) -> Result<&[Value], DtcgError>` and `modifier_contexts(resource, &definition) -> Result<&Map<String, Value>, DtcgError>` read resolver definitions with the runtime's checks.

## Expressions

The `dxn_dtcg::expressions` module exists with the `expressions` feature. See [Expressions](./expressions.md).

| Item | Members |
|---|---|
| `ExpressionOptions` (`#[non_exhaustive]`, `Default`, `Clone`, `Debug`, `Eq`) | Fields `step_budget: usize` (1,000), `memory_limit: usize` (64 MiB), `stack_limit: usize` (256 KiB), `precision: Option<u32>` (`Some(6)`); setters `with_step_budget`, `with_memory_limit`, `with_stack_limit`, `with_precision`; `with_module(specifier, ExpressionModule)`, `with_native_module(NativeModule)`, `modules() -> impl Iterator<Item = (&str, &ExpressionModule)>`, `native_modules() -> impl Iterator<Item = &NativeModule>`, `typescript_declarations() -> String` |
| `ExpressionModule` (`Clone`, `Eq`; `Debug` prints the language and length, never the source) | `javascript(source)`, `typescript(source)`; `source` is `impl Into<Arc<str>>` |
| `NativeModule` (`Clone`; equality is identity; `Debug` prints the specifier and export names) | `new(specifier)`, `specifier() -> &str`, `function(name, F) where F: NativeFunction<Args>`, `constant(name, impl Serialize)` (panics if the value does not serialize); `function` and `constant` panic when the name is not a JavaScript identifier or is a reserved word, `describe(name, typescript, docs)`, `with_docs(docs)` |
| `trait NativeFunction<Args>: Send + Sync + 'static` | Implemented for `Fn(A1, ..., An) -> R` with up to eight `DeserializeOwned` arguments and `R: NativeReturn` |
| `trait NativeReturn` | Implemented for `()`, `bool`, `char`, the integer and float types, `String`, `&'static str`, `serde_json::Value`, the twelve `values` models, `Json<T>`, `Vec<T>`, `Option<T>`, `[T; N]`, `BTreeMap<String, T>` (with `T: Serialize`), and `Result<T, E>` with `T: Serialize, E: Display` |
| `struct Json<T>(pub T)` | Wraps any `T: Serialize` as a native function's result |
| `#[expression_module(specifier = "...")]` | Attribute macro on an inherent `impl` block (re-exported from `dxn_dtcg_derive`); adds `fn expression_module() -> NativeModule`. Function attributes: `#[expression(name = "...", ts = "...")]` |

## DtcgError and diagnostics

`DtcgError` is documented variant by variant in [Errors](./errors.md), including `DtcgError::Expression { resource, path, message }` for expressions. With the `diagnostics` feature, `DtcgDiagnostic` (`from_error`, `code`, `resource`, `path`, `message`) and `DtcgDiagnosticCode` (with `Expression` for expression errors) are available; see [Limits and diagnostics](./limits.md#the-diagnostics-feature).

Next: [Conformance](./conformance.md).
