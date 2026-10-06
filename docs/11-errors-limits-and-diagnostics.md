# 11. Errors, limits and diagnostics

This chapter is the reference for everything that can go wrong: the two ways an operation can fail
to produce a result, every `DtcgError` variant, the guarantees about state after an error, the
resource limits, and the optional `diagnostics` feature.

- [Missing is not an error](#missing-is-not-an-error)
- [`DtcgError`](#dtcgerror)
- [Handling errors](#handling-errors)
- [State after an error](#state-after-an-error)
- [Runtime limits](#runtime-limits)
- [The `diagnostics` feature](#the-diagnostics-feature)

## Missing is not an error

Operations that need documents return `Result<Resolution<T>, DtcgError>`, which has three outcomes:

| Outcome | Meaning | What to do |
|---|---|---|
| `Ok(Resolution::Ready(value))` | Done | Use the value |
| `Ok(Resolution::Missing(requests))` | Documents are needed that are not in the catalog | Provide them and call again ([protocol](07-document-loading.md#the-requestprovide-protocol)) |
| `Err(error)` | The input is wrong, or a limit was hit | Report it; retrying with the same input fails the same way |

Resolution stops at the first error. Errors are deterministic: the same catalog and selection
always produce the same error.

## `DtcgError`

`DtcgError` implements `std::error::Error` (through `thiserror`) and is `Send + Sync + 'static`, so
it works with `?`, `Box<dyn Error>`, `anyhow` and similar crates. Its `Display` output names the
resource and token path where there is one.

| Variant | Raised when |
|---|---|
| `InvalidResourceId(String)` | A resource id or document reference path is invalid: absolute, contains `\`, contains `:` in any segment (a drive letter, URI scheme or NTFS stream), has empty or `.` segments, or climbs above the root. |
| `InvalidPointer(String)` | A JSON Pointer is malformed (no `#/`, a bad `~` escape, more than one `#`), points to nothing, or is part of a circular chain of pointers. Also raised by a `resolutionOrder` step referencing an undefined set or modifier. |
| `InvalidJson { resource, message }` | A provided document is not well-formed JSON. `message` is the parser's, with line and column. |
| `InvalidDocument { resource, message }` | The document structure is wrong: resolver structure, sources, sets, modifiers, a `$value` at the source root, a non-object group, or a selection that `resolve` rejects. Also used with resource `catalog` and message `root document is not set` when no root was set. |
| `UnknownDocument(ResourceId)` | `replace_json`, `remove_document` or `replace_json_and_resolve_affected` names a document that is not in the catalog |
| `DuplicateDocument(ResourceId)` | `provide_json_batch` received the same id twice |
| `LimitExceeded { kind, limit }` | A [runtime limit](#runtime-limits) was exceeded. `kind` is `"documents"`, `"reference depth"` or `"alias depth"`. |
| `DocumentCycle(String)` | A resolver includes itself, directly or through other resolvers. The field is the resource id. |
| `Token { resource, path, message }` | A token or group is malformed: a bad name, an unknown `$` property, a non-string `$type` or `$extends`, a bad token-level `$ref`, an alias cycle or a group-extension cycle. `resolve` also uses it for an unknown selected context, with `path` like `#/modifiers/theme`. |
| `TokenType { resource, path, message }` | A token has no type, an unsupported type, or a value that does not match its type |
| `Projection { path, message }` | A projected token is missing or does not convert. Despite the name, resolution also uses it for a missing alias target and for `$extends` targets that are missing or malformed. |
| `InvalidEnvelope { resource, message }` | A `.tokeneditor` envelope lacks an object `data` member or has the wrong version |
| `InvalidReference { resource, reference, message }` | A reference breaks `LocalReferencePolicy` |
| `UnknownModifier { modifier, known }` | `normalize_selection` got a modifier the root resolver does not declare. `known` lists the declared ones. |
| `UnknownContext { modifier, context, allowed, message }` | `normalize_selection` got an unknown context, the same modifier twice, or no context for a modifier without a default. `allowed` lists the modifier's contexts. |

Examples of `Display` output:

```text
invalid JSON in `tokens.json`: EOF while parsing an object at line 1 column 1
invalid DTCG document `theme.resolver.json`: missing input for modifier `density`
document `nope.json` was not previously provided
document graph exceeds 64 alias depth
token `a` in `tokens.json`: alias cycle: a -> b -> a
token `spacing.md` in `tokens.json`: resolved value does not match DTCG type `dimension`
projection `color.missing`: missing alias target
reference `https://example.com/a.json` in `a.json`: URI schemes are not supported
unknown modifier `size`; known modifiers: density, theme
```

## Handling errors

Most applications only need to show the message. Tools that point at the problem, such as editors,
language servers and CI annotations, can match on the variant to get a location:

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime, ResourceId};

/// The resource and token path an error is about, where it has them.
fn location(error: &DtcgError) -> (Option<&ResourceId>, Option<&str>) {
    match error {
        DtcgError::Token { resource, path, .. } | DtcgError::TokenType { resource, path, .. } => {
            (Some(resource), Some(path))
        }
        DtcgError::Projection { path, .. } => (None, Some(path)),
        DtcgError::InvalidJson { resource, .. }
        | DtcgError::InvalidDocument { resource, .. }
        | DtcgError::InvalidEnvelope { resource, .. }
        | DtcgError::InvalidReference { resource, .. } => (Some(resource), None),
        DtcgError::UnknownDocument(resource) | DtcgError::DuplicateDocument(resource) => {
            (Some(resource), None)
        }
        _ => (None, None),
    }
}

fn main() {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r#"{ "radius": { "$type": "dimension", "$value": { "value": 4, "unit": "pt" } } }"#,
    )
    .unwrap();
    let error = runtime.resolve(&[]).unwrap_err();

    let (resource, path) = location(&error);
    assert_eq!(resource.map(ResourceId::as_str), Some("tokens.json"));
    assert_eq!(path, Some("radius"));
    eprintln!("error: {error}");
}
```

A `path` is a dotted token path, except for selection errors from `resolve`, where it is a
resolver pointer such as `#/modifiers/theme`.

## State after an error

- **Reading methods never change the runtime.** `resolve`, `project`, `modifiers`, the validation
  methods and the inspection methods take `&self`.
- **Catalog mutations are atomic.** A failed `provide_json`, `provide_requested_json`,
  `provide_json_batch`, `replace_json` or `remove_document` leaves the documents and the revision as
  they were.
- **Affected resolution rolls back.** `replace_json_and_resolve_affected` restores the previous
  document text when the result is not `Ready` (see [Rollback](09-incremental-resolution.md#rollback)).

## Runtime limits

`RuntimeLimits` bounds the work any set of documents can cause, so cycles that slip past cycle
detection, accidents and hostile inputs all end in an error.

```rust,ignore
pub struct RuntimeLimits {
    pub max_documents: usize,       // default 256
    pub max_reference_depth: usize, // default 64
    pub max_alias_depth: usize,     // default 64
}
```

| Limit | Bounds | Error `kind` |
|---|---|---|
| `max_documents` | The number of source occurrences one discovery gathers: terminal sources, sources waiting to be expanded, and missing documents. Every source counts, inline ones included, so one set with 300 inline sources exceeds the default. Preview occurrences count together with the root's sources. For `source_references`, it bounds the distinct documents loaded. | `"documents"` |
| `max_reference_depth` | How deeply references and nested resolvers nest: each `$ref` hop and each nested resolver adds a level | `"reference depth"` |
| `max_alias_depth` | How many tokens can be on one alias-resolution chain at the same time | `"alias depth"` |

Limits are fixed when the runtime is created:

```rust
use dtcg_runtime::{DtcgRuntime, RuntimeLimits};

let runtime = DtcgRuntime::with_limits(RuntimeLimits {
    max_documents: 2048,
    ..RuntimeLimits::default()
});
assert!(runtime.contains_document("tokens.json").is_ok());
```

`from_json` and `from_envelope` always use the defaults. With custom limits, start from
`with_limits`, then `provide_json` and `set_root`. Raise `max_documents` for design systems
assembled from many small sources. Lower all three when resolving documents from untrusted users
on a server.

## The `diagnostics` feature

```toml
[dependencies]
dtcg_runtime = { git = "https://github.com/rayx-ui/RayX.Dtcg", features = ["diagnostics"] }
```

The feature adds two things.

### `DtcgDiagnostic`

`DtcgDiagnostic::from_error(&error)` turns an error into a stable code plus the logical location,
for test harnesses, telemetry and crash reports. It keeps the resource id, the path and the
message, and never copies document text or JSON values.

```rust
use dtcg_runtime::{DtcgDiagnostic, DtcgDiagnosticCode, DtcgRuntime};

let runtime = DtcgRuntime::from_json(
    "tokens.json",
    r#"{ "a": { "$type": "number", "$value": "{b}" }, "b": { "$type": "number", "$value": "{a}" } }"#,
)
.unwrap();
let diagnostic = DtcgDiagnostic::from_error(&runtime.resolve(&[]).unwrap_err());

assert_eq!(diagnostic.code(), DtcgDiagnosticCode::Token);
assert_eq!(diagnostic.resource().unwrap().as_str(), "tokens.json");
assert_eq!(diagnostic.path(), Some("a"));
assert_eq!(diagnostic.message(), "alias cycle: a -> b -> a");
```

| `DtcgDiagnosticCode` | From | `resource()` | `path()` | `message()` |
|---|---|---|---|---|
| `InvalidResourceId` | `InvalidResourceId` | none | none | the rejected id |
| `InvalidPointer` | `InvalidPointer` | none | none | the pointer or description |
| `InvalidJson` | `InvalidJson` | resource | none | parser message |
| `InvalidDocument` | `InvalidDocument` | resource | none | message |
| `UnknownDocument` | `UnknownDocument` | resource | none | empty |
| `DuplicateDocument` | `DuplicateDocument` | resource | none | empty |
| `LimitExceeded` | `LimitExceeded` | none | the limit kind | the limit value |
| `DocumentCycle` | `DocumentCycle` | none | the resource id | empty |
| `Token` | `Token` and `TokenType` | resource | token path | message |
| `Projection` | `Projection` | none | token path | message |
| `Envelope` | `InvalidEnvelope` | resource | none | message |
| `Reference` | `InvalidReference` | resource | the reference | message |
| `Selection` | `UnknownModifier` and `UnknownContext` | none | the modifier | the full `Display` text |

The messages the runtime writes contain ids, paths and short reasons, never document bodies. A
message from your own conversion function is passed through as is.

### `AffectedResolution::nodes_visited()`

Returns how many tokens an affected re-resolution resolved, including unaffected tokens that
affected ones depend on. Tests use it to check that an edit stayed local. See
[Incremental resolution](09-incremental-resolution.md#affected-re-resolution).
