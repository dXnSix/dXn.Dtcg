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
| `InvalidDocument { resource, message }` | Resolver structure is wrong (version, unknown member, empty `resolutionOrder`, fewer than two contexts, duplicate modifier, bad default, forbidden or cyclic source reference), a group is not an object, or no root was set (`root document is not set`). Also a catalog kind mismatch: JSON provided where the catalog holds a module or the reverse, a request answered with the other kind, or a JSON reference to a module document |
| `UnknownDocument(ResourceId)` | `replace_json`, `replace_module`, `remove_document` or `replace_json_and_resolve_affected` names a document not in the catalog |
| `DuplicateDocument(ResourceId)` | `provide_json_batch` received the same id twice |
| `LimitExceeded { kind, limit }` | A [runtime limit or fixed bound](./limits.md) was exceeded; `kind` names it. With expressions, also `expression steps` and `expression memory` |
| `DocumentCycle(String)` | A resolver includes itself, directly or through others; the field is the resource id |
| `Token { resource, path, message }` | A token or group is malformed: bad name, unknown `$` property, wrongly typed metadata, a token with children, `$root` or `$extends`, a token with both `$value` and `$ref`, token/group collision, an alias at a disallowed position, an alias cycle or a group-extension cycle |
| `TokenType { resource, path, message }` | No type (`token requires a DTCG type`), an unsupported type, a value that does not match its type, or `reference type mismatch: expected X, found Y` |
| `UnresolvedReference { resource, path, target, message }` | A missing alias target (including `{group}` and `{}`), or a missing or malformed `$extends` target. `path` is the referring token or group, `target` the reference as written |
| `Projection { path, message }` | A projected token is missing or does not convert, or `ResolvedToken::typed` cannot convert a value |
| `InvalidReference { resource, reference, message }` | A reference breaks `LocalReferencePolicy` |
| `Expression { resource, path, message }` | With [expressions](./expressions.md) enabled: a malformed expression object or module declaration, a syntax error or uncaught exception, a result that is not a JSON value, a stack overflow, an unknown module or export, or a module that does not compile or evaluate; see [below](#expression-errors) |
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
        DtcgError::Expression { resource, path, .. } => (Some(resource), Some(path)),
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

A `path` is a dotted token or group path; it is empty for an error about a source root, such as a non-string `$schema`. For `Expression`, `path` is the token, or the module specifier when the error is about a module declaration or a document module.

## Expression errors

`DtcgError::Expression { resource, path, message }` exists in every build, but only a runtime with the `expressions` feature and `RuntimeOptions::with_expressions` raises it. `Display` reads ``expression `<path>` in `<resource>`: <message>``:

```text
expression `space.large` in `tokens.json`: ReferenceError: bas is not defined at line 1, column 12
```

| Cause | `resource` and `path` | `message` |
|---|---|---|
| A malformed expression object (unknown member, both or neither of `js` and `ts`, an invalid input or import name, a relative import specifier) | the token's document and path | names the member |
| A malformed module declaration at a document's root | the document; the specifier, or empty for the object itself | names the problem |
| A syntax error, including early errors, or an uncaught exception | the token | the JavaScript error with, when QuickJS reports one, its line and column in the authored text (mapped back through removed TypeScript types) |
| A result that is not JSON | the token | where in the result: ``the result at `[3].a` is a function, not a JSON value`` |
| Runaway recursion past `stack_limit`, or source text that nests too deeply | the token | `stack overflow ...`, `the source nests too deeply` |
| An unknown specifier, an import of a missing export, a host or native module in the reserved `dxn.dtcg` namespace, or a specifier both registered by the host and declared by a document | the importing token, or the declaring document | names the module |
| A document module that does not compile or throws in its top-level code, or a declared module id that holds JSON | the module's resource id and the specifier it was imported by | the error, with its position when known |

Expression code can catch its own exceptions, including errors thrown by native functions, but not the limits: exceeding `step_budget` or `memory_limit` fails with `LimitExceeded { kind: "expression steps" }` or `LimitExceeded { kind: "expression memory" }` ([expression limits](./limits.md#expression-limits)). A result of the wrong DTCG type is a `TokenType` error, a missing input target an `UnresolvedReference`, and a cycle through inputs an alias cycle (`Token`), exactly as for `$value`.

With the `diagnostics` feature, `DtcgDiagnosticCode::Expression` keeps the resource and path, and its message keeps only the error's kind and, when known, authored position (`TypeError at line 2, column 9`), never the JavaScript message, which may quote document text or thrown values ([diagnostics](./limits.md#the-diagnostics-feature)).

## State after an error

- **Reading methods never change the runtime.** `resolve`, `project`, `modifiers`, `normalize_selection`, the validation methods and the inspection methods take `&self`.
- **Catalog mutations are atomic.** A failed `provide_json`, `provide_requested_json`, `provide_json_batch`, `replace_json`, `provide_module`, `provide_requested_module`, `replace_module` or `remove_document` leaves documents and revision as they were.
- **Expressions leave nothing behind.** Every resolution starts a fresh JavaScript engine, so after an expression error or limit the same runtime resolves again normally.
- **Affected resolution rolls back.** `replace_json_and_resolve_affected` restores the previous text unless the result is `Ready`; see [Rollback](./incremental.md#rollback).

Next: [Troubleshooting](./troubleshooting.md), or [Limits and diagnostics](./limits.md).
