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

Missing documents are reported before input errors: a selection naming a modifier that only a
not-yet-loaded resolver declares is not rejected while that resolver is missing. Resolution stops
at the first error. Errors are deterministic: the same catalog, options and selection always
produce the same error.

## `DtcgError`

`DtcgError` implements `std::error::Error` (through `thiserror`), `Clone`, `PartialEq` and
`Debug`, and is `Send + Sync + 'static`, so it works with `?`, `Box<dyn Error>`, `anyhow` and
similar crates, and can be stored and compared in tests. Its `Display` output names the resource
and token path where there is one.

The enum is `#[non_exhaustive]`: new variants may be added in minor releases, so a `match` on it
needs a wildcard arm.

| Variant | Raised when |
|---|---|
| `InvalidResourceId(String)` | A resource id or document reference path is invalid: empty, absolute, contains `\`, contains `#` (ids name whole documents), contains `:` in any segment (a drive letter, URI scheme or NTFS stream), has empty or `.` segments, climbs above the root, or has a segment that contains a control character or one of `< > " \| ? *`, ends with `.` or a space, or is a Windows device name (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`, in any case, with or without an extension). |
| `InvalidPointer(String)` | A JSON Pointer is malformed (no `#/`, a bad `~` escape, a bad `%` escape, an array index with a sign or a leading zero), points to nothing, or is part of a circular chain of document pointers. Also raised by a `resolutionOrder` step whose `$ref` names an undefined set or modifier, and by a source reference whose fragment does not exist. |
| `InvalidJson { resource, message }` | A provided document is not well-formed JSON. `message` is the parser's, with line and column. |
| `InvalidDocument { resource, message }` | The resolver structure is wrong: a version other than `2025.10`, an unknown member in a resolver, set, modifier or inline step, a missing or empty `resolutionOrder`, a modifier with fewer than two contexts, an inline step without `type` or `name`, a modifier defined by two steps, a default that names no context, a source reference into `resolutionOrder` or `modifiers`, a local reference cycle. Also a non-object group, and resource `catalog` with message `root document is not set` when no root was set. |
| `UnknownDocument(ResourceId)` | `replace_json`, `replace_module`, `remove_document` or `replace_json_and_resolve_affected` names a document that is not in the catalog |
| `DuplicateDocument(ResourceId)` | `provide_json_batch` received the same id twice |
| `LimitExceeded { kind, limit }` | A [runtime limit](#runtime-limits) or one of the [fixed bounds](#fixed-bounds) was exceeded. `kind` names it. |
| `DocumentCycle(String)` | A resolver includes itself, directly or through other resolvers. The field is the resource id. |
| `Token { resource, path, message }` | A token or group is malformed: a bad or empty name, an unknown `$` property (including `$schema` anywhere but a document root), `$type`/`$extends`/`$description` that is not a string, `$deprecated` that is not a boolean or string, `$extensions` that is not an object, a token with children, `$root` or `$extends`, a token with both `$value` and `$ref`, a `$root` that is not a token, a source root that is a token, an `$extends` target that is a token, a path that is a token in one source and a group in another, an alias in a position that does not allow one, an alias cycle or a group-extension cycle. |
| `TokenType { resource, path, message }` | A token has no type (`token requires a DTCG type`), an unsupported type, or a value that does not match its type; or a reference names a token of the wrong type (`reference type mismatch: expected X, found Y`), either through an own `$type` that differs from the target's or through an alias at a composite member. |
| `UnresolvedReference { resource, path, target, message }` | A reference names no resolvable target: a missing alias target, an alias to a group (`{group}`) or an empty alias (`{}`), or a missing or malformed `$extends` target. `path` is the referring token or group, `target` the reference as written. |
| `Projection { path, message }` | A projected token is missing or does not convert into the requested Rust type, or `ResolvedToken::typed` cannot convert a value. Used only for projection. |
| `InvalidReference { resource, reference, message }` | A reference breaks `LocalReferencePolicy` |
| `Expression { resource, path, message }` | With [expressions](15-expressions.md) enabled: a malformed expression object (`path` is the token) or module declaration (`resource` is the document, `path` the specifier); a syntax error or an uncaught exception, with the authored line and column when QuickJS reports one; a result that is not a JSON value; a stack overflow; an unknown module or export; a module that does not compile or evaluate (a document module names its resource and the specifier that imported it). |
| `InvalidInput { message }` | `selection_from_json` got something other than a JSON object of strings |
| `UnknownModifier { modifier, known }` | A selection names a modifier that no reached resolver declares (`known` lists the declared ones in declaration order, root first), or a name that matches several declared modifiers case-insensitively (`known` lists the candidates). |
| `UnknownContext { modifier, context, allowed, message }` | A selection names a context the modifier does not declare (`selected modifier context is unknown`), a context that matches several case-insensitively, the same modifier twice (`modifier is selected twice`), or omits a modifier without a default (`modifier has no default and was not selected`, with an empty `context`). `allowed` lists the modifier's contexts. |

Selection errors are the same for every method that takes a selection: `resolve`, `project`,
`normalize_selection`, the previews and `inspect_source` all report `UnknownModifier` and
`UnknownContext` as described above.

Some `Display` outputs:

```rust
use dxn_dtcg::{DtcgRuntime, ResourceId, selection_from_json};
use serde_json::json;

let theme = DtcgRuntime::from_json(
    "theme.resolver.json",
    json!({
        "version": "2025.10",
        "modifiers": { "density": { "contexts": { "compact": [], "comfortable": [] } } },
        "resolutionOrder": [{ "$ref": "#/modifiers/density" }]
    })
    .to_string(),
)
.unwrap();
assert_eq!(
    theme.resolve(&[]).unwrap_err().to_string(),
    "modifier `density` context ``: modifier has no default and was not selected; \
     allowed contexts: compact, comfortable"
);
assert_eq!(
    theme.resolve(&[("size", "large"), ("density", "compact")]).unwrap_err().to_string(),
    "unknown modifier `size`; known modifiers: density"
);

let tokens = DtcgRuntime::from_json(
    "tokens.json",
    json!({ "link": { "$type": "color", "$value": "{color.missing}" } }).to_string(),
)
.unwrap();
assert_eq!(
    tokens.resolve(&[]).unwrap_err().to_string(),
    "token `link` in `tokens.json`: unresolved reference `color.missing`: missing alias target"
);

assert_eq!(
    ResourceId::parse("tokens.json#/a").unwrap_err().to_string(),
    "invalid logical resource id `tokens.json#/a`"
);
assert_eq!(
    selection_from_json(&json!({ "theme": 1 })).unwrap_err().to_string(),
    "invalid resolver inputs: input `theme` must be a string"
);
```

Other variants read, for example:

```text
invalid JSON in `tokens.json`: EOF while parsing an object at line 1 column 1
document `nope.json` was not previously provided
document graph exceeds 64 alias depth
token `a` in `tokens.json`: alias cycle: a -> b -> a
token `spacing.md` in `tokens.json`: resolved value does not match DTCG type `dimension`
token `weight` in `tokens.json`: reference type mismatch: expected fontWeight, found number
invalid DTCG document `theme.resolver.json`: resolutionOrder must not be empty
projection `color.accent`: expected string value
reference `https://example.com/a.json` in `a.json`: URI schemes are not supported
```

## Handling errors

Most applications only need to show the message. Tools that point at the problem, such as editors,
language servers and CI annotations, can match on the variant to get a location. Because
`DtcgError` is `#[non_exhaustive]`, the match ends with a wildcard arm:

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime, ResourceId};

/// The resource and token path an error is about, where it has them.
fn location(error: &DtcgError) -> (Option<&ResourceId>, Option<&str>) {
    match error {
        DtcgError::Token { resource, path, .. }
        | DtcgError::TokenType { resource, path, .. }
        | DtcgError::UnresolvedReference { resource, path, .. } => (Some(resource), Some(path)),
        DtcgError::Projection { path, .. } => (None, Some(path)),
        DtcgError::InvalidJson { resource, .. }
        | DtcgError::InvalidDocument { resource, .. }
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
    // Errors are `Clone` and `PartialEq`.
    assert_eq!(error.clone(), runtime.resolve(&[]).unwrap_err());
    eprintln!("error: {error}");
}
```

A `path` is a dotted token or group path. It is empty for an error about a source root, such as a
non-string `$schema`.

## State after an error

- **Reading methods never change the runtime.** `resolve`, `project`, `modifiers`,
  `normalize_selection`, the validation methods and the inspection methods take `&self`.
- **Catalog mutations are atomic.** A failed `provide_json`, `provide_requested_json`,
  `provide_json_batch`, `replace_json` or `remove_document` leaves the
  documents and the revision as they were.
- **Affected resolution rolls back.** `replace_json_and_resolve_affected` restores the previous
  document text when the result is not `Ready`, for example when an edit deletes a token that
  another token aliases (`UnresolvedReference`). See
  [Rollback](09-incremental-resolution.md#rollback).

## Runtime limits

`RuntimeLimits` bounds the work any set of documents can cause, so cycles that slip past cycle
detection, accidents and hostile inputs all end in an error instead of exhausting memory or the
stack. The struct is `#[non_exhaustive]`: build it from `RuntimeLimits::default()` with the
chainable setters, not with a struct literal.

| Limit | Default | Setter | Bounds | Error `kind` |
|---|---|---|---|---|
| `max_documents` | 256 | `with_max_documents` | Two counts per discovery, checked separately (and, with expressions, a third: the module documents one resolution loads): the source occurrences it collects (loaded and inline sources and missing-document requests), and its expansion steps (each set reference, local reference, document load and nested resolver, even one that yields no source). Every source counts, inline ones included, so one set with 300 inline sources exceeds the default. Preview occurrences count together with the root's sources. For `source_references`, it bounds the distinct documents loaded and, in discovery's units, the steps and the inline sources of repeated walks (a location reached again under different reference-object overrides); first walks are free and token-document targets are never walked. | `"documents"` |
| `max_reference_depth` | 64 | `with_max_reference_depth` | How deeply resolver and source references nest: each `$ref` hop and each nested resolver adds a level | `"reference depth"` |
| `max_alias_depth` | 64 | `with_max_alias_depth` | Reference hops in one resolution chain, counting curly-brace aliases, pointers to tokens and pointers to other document locations alike. The count does not depend on authored order. Literal values cost nothing, so a limit of 0 still resolves tokens without references. | `"alias depth"` |
| `max_value_nodes` | 4,000,000 | `with_max_value_nodes` | JSON nodes that references copy into resolved values in one resolution, checked before each copy. This stops pointer fan-out that would build exponentially large values. | `"value nodes"` |
| `max_tokens` | 100,000 | `with_max_tokens` | Flattened tokens in one resolution, including tokens copied by `$extends`; and, counted separately, the JSON nodes that `$extends` copies | `"tokens"` |

### Fixed bounds

Two further bounds are not configurable. They match `serde_json`'s parse recursion limit, so no
authored document reaches them except through references or `$extends`:

| Bound | Value | Error `kind` |
|---|---|---|
| Nesting depth of a resolved value, root level included (an expression's result too) | 128 | `"value depth"` |
| Segments of a flattened token or group path | 128 | `"path depth"` |

Together with the limits, they keep every algorithm iterative or bounded: nothing recurses without
a bound, and a chain of 100,000 pointers fails with `alias depth` instead of overflowing the stack.

### Expression limits

With the `expressions` feature, `ExpressionOptions` bounds the JavaScript engine of each
resolution ([expressions](15-expressions.md#the-sandbox-and-its-limits)). Expression code cannot
catch any of these limits, and the runtime resolves normally afterwards.

| Option | Default | Bounds | Error |
|---|---|---|---|
| `step_budget` | 1,000 | QuickJS interrupt checks (about every 10,000 calls and loop iterations) of one evaluation: an expression call, a module's top-level code, or an import cycle's as a whole; at least 1 | `LimitExceeded { kind: "expression steps" }` |
| `memory_limit` | 64 MiB | Bytes the engine of one resolution allocates | `LimitExceeded { kind: "expression memory" }` |
| `stack_limit` | 256 KiB | Native stack JavaScript code uses below the resolving call; at least 16 KiB | `Expression` with a stack overflow message |
| (fixed) | about 120 brackets, 40 function bodies or 300 chained operators | How deeply the text of an expression or module nests, bounded before the compiler sees it | `Expression`: `the source nests too deeply` |

Expression results also count toward `max_value_nodes` and the 128-level value depth, and
expression inputs toward `max_alias_depth`.

### Setting limits

Limits are fixed when the runtime is created, through `with_limits` or the options constructors,
and `DtcgRuntime::limits()` returns them:

```rust
use dxn_dtcg::{DtcgRuntime, RuntimeLimits, RuntimeOptions, ValueExtensions};

let limits = RuntimeLimits::default()
    .with_max_documents(2048)
    .with_max_alias_depth(16);

// An empty runtime: provide documents and set the root afterwards.
let runtime = DtcgRuntime::with_limits(limits.clone());
assert_eq!(runtime.limits().max_documents, 2048);

// Or with options, which also carry the value extensions.
let runtime = DtcgRuntime::from_json_with_options(
    "tokens.json",
    r#"{ "gap": { "$type": "number", "$value": 4 } }"#,
    RuntimeOptions::default()
        .with_limits(limits)
        .with_extensions(ValueExtensions::all()),
)
.unwrap();
assert_eq!(runtime.limits().max_alias_depth, 16);
assert!(runtime.resolve(&[]).unwrap().is_ready());
```

`from_json` uses the default limits; `from_json_with_options` takes `RuntimeOptions`. Raise `max_documents` for design systems
assembled from many small sources. Lower the limits when resolving documents from untrusted users
on a server.

## The `diagnostics` feature

```toml
[dependencies]
dxn_dtcg = { git = "https://github.com/dXnSix/dXn.Dtcg", tag = "v1.1.0", features = ["diagnostics"] }
```

The feature adds two things.

### `DtcgDiagnostic`

`DtcgDiagnostic::from_error(&error)` turns an error into a stable code plus the logical location,
for test harnesses, telemetry and crash reports. It keeps the resource id, the path and the
message, and never copies document text or JSON values.

```rust
use dxn_dtcg::{DtcgDiagnostic, DtcgDiagnosticCode, DtcgRuntime};

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

let runtime = DtcgRuntime::from_json(
    "tokens.json",
    r#"{ "a": { "$type": "number", "$value": "{gone}" } }"#,
)
.unwrap();
let diagnostic = DtcgDiagnostic::from_error(&runtime.resolve(&[]).unwrap_err());
assert_eq!(diagnostic.code(), DtcgDiagnosticCode::UnresolvedReference);
assert_eq!(diagnostic.path(), Some("a"));
assert_eq!(diagnostic.message(), "unresolved reference `gone`: missing alias target");
```

`DtcgDiagnosticCode` is `#[non_exhaustive]`; new codes arrive together with new `DtcgError`
variants.

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
| `UnresolvedReference` | `UnresolvedReference` | resource | referring token or group path | ``unresolved reference `<target>`: <message>`` |
| `Projection` | `Projection` | none | token path | message |
| `Reference` | `InvalidReference` | resource | the reference | message |
| `Selection` | `UnknownModifier` and `UnknownContext` | none | the modifier | the full `Display` text |
| `InvalidInput` | `InvalidInput` | none | none | message |
| `Expression` | `Expression` | resource | token path (or specifier) | the error's kind and, when known, authored position only, such as `TypeError at line 2, column 9`: never the JavaScript message, which may quote document text or thrown values |

The messages the runtime writes contain ids, paths and short reasons, never document bodies. A
message from your own conversion function is passed through as is.

### `AffectedResolution::nodes_visited()`

Returns how many tokens an affected re-resolution resolved, including unaffected tokens that
affected ones depend on. Tests use it to check that an edit stayed local. See
[Incremental resolution](09-incremental-resolution.md#affected-re-resolution).
