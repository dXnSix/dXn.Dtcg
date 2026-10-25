---
title: Which API do I use
description: Decision tables for picking the right constructor, resolution method, value representation and editor API in dxn_dtcg.
group: Start
order: 30
---

The crate has one entry point, `DtcgRuntime`, and a handful of methods that differ in how much they resolve and what they return. Find your task in the tables below.

## Creating the runtime

| You have | Use |
|---|---|
| One token document, or one resolver with inline sources, as a string | `DtcgRuntime::from_json(id, json)` |
| A resolver that references other files | `DtcgRuntime::default()` then `set_root(id)`, and load on demand |
| All documents up front (embedded assets, tests) | `from_json` for the root, then `provide_json_batch([...])` |
| Custom limits, or the value extensions | the `*_with_options` variants or `DtcgRuntime::with_options(options)` |
| Tokens computed by [expressions](./expressions.md) | the `expressions` feature, then `RuntimeOptions::default().with_expressions(ExpressionOptions::default())` passed to those constructors |

Options are fixed at construction. `default()` and `from_json` use default limits and strict DTCG 2025.10 values.

## Getting tokens out

| You want | Use | Returns |
|---|---|---|
| Every token for a selection | `runtime.resolve(&selection)` | `Resolution<ResolvedTokens>` |
| A struct of just the tokens you need | `runtime.project::<T>(&selection)` | `Resolution<T>` |
| Several structs from one selection | `resolve` once, then `T::project(&tokens, runtime.registry())` per struct | `Result<T, DtcgError>` |
| One token as typed Rust data | `tokens.get(path)?.typed(&runtime.options().extensions)` | `TypedValue` |
| A nested plain DTCG document of the final values | `tokens.to_document()` | `serde_json::Value` |

## Representing values

| You need | Use |
|---|---|
| Raw JSON, to feed another library | `ResolvedToken::value` |
| A Rust model per DTCG type (`Color`, `Dimension`, `Shadow`, ...) | `dxn_dtcg::values` types as struct fields, or `ResolvedToken::typed` |
| Your own type (a renderer's color, `std::time::Duration`) | `impl FromDtcgValue for YourType` when you own the type, else `#[dtcg(with = "convert_fn")]` on the field |
| The value as authored, aliases intact (to show or preserve them) | `ResolvedToken::authored_value` |
| The authored `$value` of a token whose expression was evaluated | `ResolvedToken::snapshot` |
| A color in another space, mixed, lightened, as hex, or its contrast | the `values::Color` methods ([Color conversions](./value-types.md#color-conversions)) |

## Selections and pickers

| You want | Use |
|---|---|
| The modifiers and contexts to build a theme picker | `runtime.modifiers()` (root resolver only) |
| A validated, complete selection with defaults filled in, usable as a cache key | `runtime.normalize_selection(&selection)` |
| A selection from a settings file or URL as JSON | `selection_from_json(&json)` then `selection_refs(&owned)` |

## Loading and checking

| You want | Use |
|---|---|
| Load only what one selection needs | loop on `resolve` until `Ready` |
| Load the JavaScript or TypeScript modules expressions import | the same loop; answer `DocumentKind::Module` requests with `provide_requested_module` ([module documents](./loading.md#module-documents)) |
| Fetch every context's documents (offline cache, bundle) | loop on `validate_complete()` |
| Check that every theme combination resolves (CI) | resolve every combination of `modifiers()`; see the [cookbook](./cookbook.md#validate-every-theme-combination-in-ci) |
| Validate one value without a runtime | `validate_value(type, &value)` or `validate_value_with` |

## Editors and tools

| You want | Use |
|---|---|
| Re-resolve after an edit, touching only affected tokens | `replace_json_and_resolve_affected`, then `ResolvedTokens::apply_affected` |
| Preview an unsaved source on top of the catalog | `preview_source` (or `preview_occurrence` for one occurrence) |
| Check a draft set or modifier before saving it | `validate_source` |
| See the authored sources, their origins, dormant and shadowed ones | `inspect_source` |
| Find every `$ref` between files (move a file safely) | `source_references()` |
| Detect "nothing changed" cheaply | compare `ResolvedTokens::fingerprint()` |

## Pitfalls

:::warning Missing is not an error
Most methods return `Result<Resolution<T>, DtcgError>`. Calling `.ready().unwrap()` is fine when you provided everything up front, but with a root you set by name the first call is `Missing`. Loop until `Ready`, as in [Loading documents](./loading.md).
:::

:::note Fingerprints ignore metadata
`fingerprint()` hashes path, type, value, source and order. Changing a `$description` or `$deprecated` does not change it. Compare `ResolvedTokens` with `==` to notice metadata changes.
:::

Next: [Cookbook](./cookbook.md) for worked recipes, or [API reference](./api.md).
