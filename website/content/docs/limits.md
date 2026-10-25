---
title: Limits and diagnostics
description: RuntimeLimits and fixed bounds for untrusted input, how to set them, and the optional diagnostics feature for redacted error reports.
group: Reference
order: 160
---

Resolution is bounded: a hostile or accidental document set ends in `DtcgError::LimitExceeded`, never in unbounded memory, time or stack use. This page lists the bounds, how to change them and the optional `diagnostics` feature.

## Runtime limits

`RuntimeLimits` is `#[non_exhaustive]`: start from `RuntimeLimits::default()` and use the chainable `with_*` setters. Limits are fixed when the runtime is created.

| Limit | Default | Setter | Bounds | Error `kind` |
|---|---|---|---|---|
| `max_documents` | 256 | `with_max_documents` | Per discovery, counted separately: the source occurrences collected (loaded and inline sources, and missing-document requests) and the expansion steps (each set reference, local reference, document load and nested resolver, even one yielding no source) | `"documents"` |
| `max_reference_depth` | 64 | `with_max_reference_depth` | Nesting of resolver and source references | `"reference depth"` |
| `max_alias_depth` | 64 | `with_max_alias_depth` | Reference hops in one chain: aliases, token pointers and document pointers. Literal values cost nothing, so a limit of 0 still resolves tokens without references | `"alias depth"` |
| `max_value_nodes` | 4,000,000 | `with_max_value_nodes` | JSON nodes that references copy into resolved values in one resolution, checked before each copy | `"value nodes"` |
| `max_tokens` | 100,000 | `with_max_tokens` | Flattened tokens, `$extends` copies included, and separately the JSON nodes `$extends` copies | `"tokens"` |

Every source counts toward `max_documents`, inline ones included, so one set with 300 inline sources exceeds the default. A small resolver whose sets reference each other many times also fails even when every set is empty. Inspection and preview calls apply the same limits; preview occurrences count together with the root's sources.

### Fixed bounds

Two bounds are not configurable. They match `serde_json`'s parse recursion limit, so no authored document reaches them except through references or `$extends`:

| Bound | Value | Error `kind` |
|---|---|---|
| Nesting depth of a resolved value | 128 | `"value depth"` |
| Segments of a flattened token or group path | 128 | `"path depth"` |

Together with the limits they keep every algorithm iterative or bounded: a chain of 100,000 pointers fails with `alias depth` instead of overflowing the stack.

### Setting limits

Pass limits through `DtcgRuntime::with_limits`, or together with value extensions through `RuntimeOptions`. `DtcgRuntime::limits()` returns what the runtime uses.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime, RuntimeLimits, RuntimeOptions, ValueExtensions};

fn main() -> Result<(), DtcgError> {
    let mut runtime = DtcgRuntime::with_limits(RuntimeLimits::default().with_max_documents(4));
    assert_eq!(runtime.limits().max_documents, 4);
    runtime.provide_json(
        "r.json",
        r##"{
          "version": "2025.10",
          "sets": { "empty": { "sources": [] } },
          "resolutionOrder": [{ "type": "set", "name": "fan", "sources": [
            { "$ref": "#/sets/empty" }, { "$ref": "#/sets/empty" }, { "$ref": "#/sets/empty" },
            { "$ref": "#/sets/empty" }, { "$ref": "#/sets/empty" }
          ] }]
        }"##,
    )?;
    runtime.set_root("r.json")?;
    // The resolver and five set references are six expansion steps.
    assert!(matches!(
        runtime.resolve(&[]),
        Err(DtcgError::LimitExceeded { kind: "documents", limit: 4 })
    ));

    // Limits and value extensions together.
    let options = RuntimeOptions::default()
        .with_limits(RuntimeLimits::default().with_max_alias_depth(16))
        .with_extensions(ValueExtensions::all());
    let runtime = DtcgRuntime::with_options(options);
    assert_eq!(runtime.limits().max_alias_depth, 16);
    assert!(runtime.options().extensions.blur);
    Ok(())
}
```

`from_json` uses the default limits; `from_json_with_options` takes `RuntimeOptions`.

:::tip Resolving untrusted documents
On a server that resolves documents from users, lower the limits (documents, tokens, value nodes) to what your largest legitimate design system needs, load only through a loader that applies `LocalReferencePolicy` and a base-directory check ([path safety](./loading.md#path-safety)), and treat every `DtcgError` as a user-facing validation failure. Raise `max_documents` for systems assembled from many small sources.
:::

## The diagnostics feature

```toml
[dependencies]
dxn_dtcg = { git = "https://github.com/dXnSix/dXn.Dtcg", tag = "v1.1.0", features = ["diagnostics"] }
```

The feature is off by default and adds two things for test harnesses, telemetry and crash reports.

### DtcgDiagnostic

`DtcgDiagnostic::from_error(&error)` turns an error into a stable code plus the logical location. It keeps the resource id, path and message and never copies document text or JSON values.

```rust
use dxn_dtcg::{DtcgDiagnostic, DtcgDiagnosticCode, DtcgRuntime};

fn main() {
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
}
```

`DtcgDiagnosticCode` is `#[non_exhaustive]` and maps from `DtcgError` like this:

| Code | From | `resource()` | `path()` | `message()` |
|---|---|---|---|---|
| `InvalidResourceId` | `InvalidResourceId` | none | none | the rejected id |
| `InvalidPointer` | `InvalidPointer` | none | none | pointer or description |
| `InvalidJson`, `InvalidDocument` | same-named variants | resource | none | message |
| `UnknownDocument`, `DuplicateDocument` | same-named variants | resource | none | empty |
| `LimitExceeded` | `LimitExceeded` | none | the limit kind | the limit value |
| `DocumentCycle` | `DocumentCycle` | none | the resource id | empty |
| `Token` | `Token` and `TokenType` | resource | token path | message |
| `UnresolvedReference` | `UnresolvedReference` | resource | referring path | ``unresolved reference `<target>`: <message>`` |
| `Projection` | `Projection` | none | token path | message |
| `Reference` | `InvalidReference` | resource | the reference | message |
| `Selection` | `UnknownModifier`, `UnknownContext` | none | the modifier | the full `Display` text |
| `InvalidInput` | `InvalidInput` | none | none | message |

Messages the runtime writes contain ids, paths and short reasons, never document bodies. A message from your own conversion function passes through unchanged.

### AffectedResolution::nodes_visited

Returns how many tokens an [affected re-resolution](./incremental.md) resolved, including unaffected tokens that affected ones depend on. Tests use it to check that an edit stayed local.

Next: [API reference](./api.md), or [Conformance](./conformance.md).
