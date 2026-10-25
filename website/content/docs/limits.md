---
title: Limits and diagnostics
description: RuntimeLimits and fixed bounds for untrusted input, how to set them, and the optional diagnostics feature for redacted error reports.
group: Reference
order: 160
---

Resolution is bounded: a hostile or accidental document set ends in `DtcgError::LimitExceeded`, never in unbounded memory, time or stack use. This page lists the bounds, the expression limits, how to change them and the optional `diagnostics` feature.

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
| Nesting depth of a resolved value, an expression's result included | 128 | `"value depth"` |
| Segments of a flattened token or group path | 128 | `"path depth"` |

Together with the limits they keep every algorithm iterative or bounded: a chain of 100,000 pointers fails with `alias depth` instead of overflowing the stack.

### Expression limits

With the `expressions` feature and `RuntimeOptions::with_expressions`, `ExpressionOptions` bounds the JavaScript engine of each resolution ([the sandbox](./expressions.md#the-sandbox-and-its-limits)). It is `#[non_exhaustive]`: start from `ExpressionOptions::default()` and use the `with_*` setters. Expression code cannot catch any of these limits, and the same runtime resolves normally afterwards.

| Option | Default | Setter | Bounds | Error |
|---|---|---|---|---|
| `step_budget` | 1,000 | `with_step_budget` | QuickJS interrupt checks (about every 10,000 function calls and loop iterations) of one evaluation: an expression call, a module's top-level code, or an import cycle's as a whole. Counted, not timed, so it is deterministic; below 1 counts as 1 | `LimitExceeded { kind: "expression steps" }` |
| `memory_limit` | 64 MiB | `with_memory_limit` | Bytes the JavaScript engine of one resolution allocates | `LimitExceeded { kind: "expression memory" }` |
| `stack_limit` | 256 KiB | `with_stack_limit` | Native stack JavaScript code uses below the resolving call; at least 16 KiB | `Expression` (stack overflow) |
| `precision` | `Some(6)` | `with_precision` | Decimal places every non-integral number of a result is rounded to; `None` keeps results unrounded | none |

One bound is fixed. The JavaScript and TypeScript compiler works recursively, so the text of every expression and module may nest about 120 brackets, 40 function bodies or 300 chained operators deep, and regular expression literals 20 groups; deeper text fails with `Expression` (`the source nests too deeply`) before the compiler sees it. Patterns built at run time nest at most 20 groups too; a deeper one throws a `SyntaxError` the code can catch, whatever stack is left.

Expressions also charge the runtime limits: input references count toward `max_alias_depth`, inputs and results toward `max_value_nodes`, results toward the 128-level value depth. Module documents are a separate count against `max_documents`: one resolution loads at most that many, besides its JSON documents.

:::warning Leave room on the resolving thread's stack
`stack_limit` must stay well below the stack of the thread that resolves. A limit near that stack's size lets QuickJS overflow the real stack before its own check, which aborts the process. The default (256 KiB) fits the default 1 MiB stack of `wasm32-unknown-unknown`; raise it only for threads you give a larger stack.
:::

Unoptimized QuickJS uses several times more stack per call, so in a debug build the same `stack_limit` allows much shallower recursion. Build the engine optimized in every profile, in your workspace's `Cargo.toml`:

```toml
[profile.dev.package.quickjs-jit-sys]
opt-level = 3
```

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

`from_json` uses the default limits; `from_json_with_options` takes `RuntimeOptions`. Expression limits travel inside `RuntimeOptions::with_expressions(ExpressionOptions::default().with_step_budget(..))`.

:::tip Resolving untrusted documents
On a server that resolves documents from users, lower the limits (documents, tokens, value nodes, and with expressions the step budget and memory) to what your largest legitimate design system needs, load only through a loader that applies `LocalReferencePolicy` and a base-directory check ([path safety](./loading.md#path-safety)), and treat every `DtcgError` as a user-facing validation failure. Raise `max_documents` for systems assembled from many small sources.
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
| `Expression` | `Expression` | resource | token path, or module specifier | only the error's kind and authored position, such as `TypeError at line 2, column 9` |

Messages the runtime writes contain ids, paths and short reasons, never document bodies. An expression error's JavaScript message may quote document text or thrown values, so its diagnostic keeps only the kind and position. A message from your own conversion function passes through unchanged.

### AffectedResolution::nodes_visited

Returns how many tokens an [affected re-resolution](./incremental.md) resolved, including unaffected tokens that affected ones depend on. Tests use it to check that an edit stayed local.

Next: [API reference](./api.md), or [Conformance](./conformance.md).
