---
title: Errors
description: Missing versus error, every DtcgError variant, how to match on them and what state the runtime is left in after a failure.
group: Reference
order: 140
---

Operations that need documents return `Result<Resolution<T>, DtcgError>`, so there are three outcomes, and only one of them is an error.

| Outcome | Meaning | What to do |
|---|---|---|
| `Ok(Resolution::Ready(value))` | Done | Use the value |
| `Ok(Resolution::Missing(requests))` | Documents are needed that the catalog lacks | Provide them and call again ([protocol](./loading.md#the-requestprovide-protocol)) |
| `Err(error)` | The input is wrong or a limit was hit | Report it; retrying with the same input fails the same way |

Missing documents are reported before input errors: a selection naming a modifier that only a not-yet-loaded resolver declares is not rejected while that resolver is missing. Resolution stops at the first error. Errors are deterministic: the same catalog, options and selection always give the same error.

## DtcgError

`DtcgError` implements `std::error::Error` (through `thiserror`), `Clone`, `PartialEq` and `Debug`, and is `Send + Sync + 'static`, so it works with `?`, `Box<dyn Error>`, `anyhow` and similar crates and can be stored and compared in tests. `Display` names the document and token path where there is one. The enum is `#[non_exhaustive]`: a `match` needs a wildcard arm.

| Variant | Raised when |
|---|---|
| `InvalidResourceId(String)` | A resource id or reference path is empty, absolute, contains `\`, `#` or `:`, has empty or `.` segments, climbs above the root, or has a segment with a control character, one of `< > " \| ? *`, a trailing `.` or space, or a Windows device name |
| `InvalidPointer(String)` | A JSON Pointer is malformed, points to nothing, or is part of a circular chain; also a `resolutionOrder` step naming an undefined set or modifier, and a source fragment that does not exist |
| `InvalidJson { resource, message }` | A provided document is not well-formed JSON; `message` is the parser's, with line and column |
| `InvalidDocument { resource, message }` | Resolver structure is wrong (version, unknown member, empty `resolutionOrder`, fewer than two contexts, duplicate modifier, bad default, forbidden or cyclic source reference), a group is not an object, or no root was set (`root document is not set`) |
| `UnknownDocument(ResourceId)` | `replace_json`, `remove_document` or `replace_json_and_resolve_affected` names a document not in the catalog |
| `DuplicateDocument(ResourceId)` | `provide_json_batch` received the same id twice |
| `LimitExceeded { kind, limit }` | A [runtime limit or fixed bound](./limits.md) was exceeded; `kind` names it |
| `DocumentCycle(String)` | A resolver includes itself, directly or through others; the field is the resource id |
| `Token { resource, path, message }` | A token or group is malformed: bad name, unknown `$` property, wrongly typed metadata, a token with children, `$root` or `$extends`, a token with both `$value` and `$ref`, token/group collision, an alias at a disallowed position, an alias cycle or a group-extension cycle |
| `TokenType { resource, path, message }` | No type (`token requires a DTCG type`), an unsupported type, a value that does not match its type, or `reference type mismatch: expected X, found Y` |
| `UnresolvedReference { resource, path, target, message }` | A missing alias target (including `{group}` and `{}`), or a missing or malformed `$extends` target. `path` is the referring token or group, `target` the reference as written |
| `Projection { path, message }` | A projected token is missing or does not convert, or `ResolvedToken::typed` cannot convert a value |
| `InvalidReference { resource, reference, message }` | A reference breaks `LocalReferencePolicy` |
| `InvalidInput { message }` | `selection_from_json` got something other than an object of strings |
| `UnknownModifier { modifier, known }` | A selection names a modifier no reached resolver declares, or one matching several declared names; `known` lists them in declaration order, root first |
| `UnknownContext { modifier, context, allowed, message }` | A selection names an undeclared context, a context matching several, the same modifier twice, or omits a modifier with no default (empty `context`); `allowed` lists the contexts |

Selection errors are identical for every method that takes a selection: `resolve`, `project`, `normalize_selection`, the previews and `inspect_source`.

Some `Display` outputs:

```rust
use dxn_dtcg::{DtcgRuntime, ResourceId, selection_from_json};
use serde_json::json;

fn main() {
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
}
```

Other messages read like this:

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

## Matching on errors

Most applications only show the message. Tools that point at the problem, such as editors, language servers and CI annotations, match on the variant to get a location.

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

A `path` is a dotted token or group path; it is empty for an error about a source root, such as a non-string `$schema`.

## State after an error

- **Reading methods never change the runtime.** `resolve`, `project`, `modifiers`, `normalize_selection`, the validation methods and the inspection methods take `&self`.
- **Catalog mutations are atomic.** A failed `provide_json`, `provide_requested_json`, `provide_json_batch`, `replace_json` or `remove_document` leaves documents and revision as they were.
- **Affected resolution rolls back.** `replace_json_and_resolve_affected` restores the previous text unless the result is `Ready`; see [Rollback](./incremental.md#rollback).

Next: [Troubleshooting](./troubleshooting.md), or [Limits and diagnostics](./limits.md).
