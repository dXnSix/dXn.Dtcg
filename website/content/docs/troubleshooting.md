---
title: Troubleshooting
description: Common dxn_dtcg error messages and surprises, with the cause and the fix for each.
group: Reference
order: 150
---

Find your message, read the cause, apply the fix. Messages are matched by their stable text; `<...>` stands for the part that varies. Each entry names the `DtcgError` variant so you can match on it in code.

## Messages from resolve

### `token requires a DTCG type`

`TokenType`. The token has no `$type`, no whole-value alias to a typed token, and no typed group above it. Put `$type` on the token or its group. In an override file the type is inherited from the group of an earlier source, so an override needs `$type` only if no earlier source declared one.

### `resolved value does not match DTCG type <type>`

`TokenType`. The value has the wrong shape. Frequent causes: a dimension with `"unit": "em"`, `"pt"` or `"%"` (only `px` and `rem`), a number written as a string (`"16"`), a color given as `"#ff0000"` or with four components, extra members in an object (validation is exact), a `fontWeight` outside 1 to 1000, an `hsl` hue of 360. See [Value types](./value-types.md).

### `unsupported DTCG type <type> for final 2025.10`

`TokenType`. Only the 13 DTCG types are valid. The old `string` type is gone (use `fontFamily`), reverse-DNS custom types are not accepted, and type names are case-sensitive. `dxn.dtcg.blur` is accepted only with the `blur` value extension (`ValueExtensions::all()` or `with_blur(true)`); see [Value types](./value-types.md#strict-values-and-the-value-extensions).

### `reference type mismatch: expected <A>, found <B>`

`TokenType`. Either the token's own `$type` differs from the whole-value alias's target (compared by type name, so `fontWeight` aliasing a `number` fails), or an alias at a composite member targets the wrong type (a border `width` must alias a `dimension`). Remove the own `$type` so it follows the target, or alias a token of the right type. A typography `lineHeight` that aliases a `dimension` needs the `line_height_dimension` extension.

### `unresolved reference <path>: missing alias target`

`UnresolvedReference`. The alias names no token. Check the spelling and case, whether the token lives in a context you did not select, and whether you aliased a group (`{spacing}`) instead of its root token (`{spacing.$root}`). An empty `{}` is also this error.

### `curly-brace references are not allowed inside <type> values`

`Token`. An alias sits where DTCG does not allow one: inside a color's members, a dimension, a `cubicBezier`, a `fontFamily` array element, `lineCap`, shadow `inset`. Use a [JSON Pointer reference](./aliases.md#json-pointer-references) to reuse part of another value, or alias the whole token.

### `alias cycle: a -> b -> a`

`Token`. Tokens alias each other. Break the loop. Pointer loops report ``circular JSON Pointer reference`` (`InvalidPointer`).

### `unknown reserved property <name>`

`Token`. A member starting with `$` that DTCG does not define. `$schema` is allowed only at the root of a whole document; in a fragment source, an inline resolver source or a preview source it is this error. Legacy `value`/`type` keys without `$` are not read at all.

### `path is a token in <a> and a group in <b>`

`Token`. Two sources disagree about a path: one defines it as a token, another gives it children. Rename one.

### `a source root must not be a token`, `token document groups must be objects`

`Token` and `InvalidDocument`. A file has a top-level `$value`, or a child that is not an object (`"spacing": 4`). Wrap tokens in groups.

### `LimitExceeded` with kind `alias depth`, `documents`, `tokens`, ...

A [limit](./limits.md) was hit. A real design system rarely reaches the defaults; if you do, raise the limit with `RuntimeLimits`. A "documents" error on a tiny resolver usually means sets referencing each other many times: every expansion step counts, even for empty sets.

## Messages from the resolver

### `resolver version must be 2025.10`, `unknown resolver member <name> in <location>`

`InvalidDocument`. Resolvers must declare exactly `"version": "2025.10"` and may contain only the members the specification allows. The root does not accept `$extensions` or `x-*`; put shared JSON under `$defs`.

### `resolutionOrder must not be empty`, `modifier requires at least two contexts`

`InvalidDocument`. A resolver needs at least one step, and a modifier needs two or more contexts. A single-context modifier should be a set.

### A document is treated as a token file although it is a resolver

A document is a resolver only if it has a `resolutionOrder` member. Without it, `sets` and `modifiers` are plain groups of tokens.

### `modifier has no default and was not selected`

`UnknownContext` with an empty `context`. Add a `default` or pass the modifier in the selection. Every root modifier counts, even one `resolutionOrder` never references.

### `unknown modifier <name>; known modifiers: ...` / `selected modifier context is unknown`

`UnknownModifier` / `UnknownContext`. The selection names something no resolver declares. The message lists what is allowed. Names match case-insensitively; a name that differs from two declared names only by case is ambiguous and also an error. With nested resolvers, a modifier only a not-yet-loaded resolver declares is not reported until that resolver is provided.

### A selection is silently ignored

If the graph declares no modifiers (for example the root is a plain token document), selections are ignored by design. Check `runtime.modifiers()` to see which axes exist.

## Loading and catalog

### `resolve` returns `Missing` forever

Each round must provide the requested documents. Typical bugs: providing under a different id than `request.resource_id` (use `provide_requested_json`), reading the wrong directory so the loader returns an error that you swallow, or `set_root` naming a document your loader never provides under that exact id.

### `invalid logical resource id <id>`

`InvalidResourceId`. Ids are relative, `/`-separated and portable: no leading `/`, no `\`, no `:` (so no drive letters or URLs), no `#`, no `..` above the root, no Windows device names such as `con.json`. References to URLs are not supported; map them to relative ids and fetch them yourself. See [Resource ids](./loading.md#resource-ids).

### `document <id> was not previously provided`

`UnknownDocument`. `replace_json`, `remove_document` and `replace_json_and_resolve_affected` need an existing document; use `provide_json` first.

### `invalid JSON in <id>: ...`

`InvalidJson`. The text is not well-formed JSON, reported with line and column when you provide it. Comments and trailing commas are not accepted (no JSONC or JSON5).

### `root document is not set`

`InvalidDocument`. You created the runtime with `DtcgRuntime::default()` and did not call `set_root`.

## Projection

### `projection <path>: required token is missing`

`Projection`. The struct field's token does not exist at that full path. Check `root`, `path` and `rename`: names are not case-converted, so a `padding_x` field reads `padding_x`, not `paddingX`. Make the field an `Option` if the token may be absent.

### `projection <path>: expected string value` (or `numeric`, `a color value`, ...)

`Projection`. The token exists but its value has another shape. Conversions look at JSON shape, not the DTCG type: `f64` for a dimension fails because a dimension is an object. Use `values::Dimension` or a custom conversion.

### Compile error: `unknown dtcg field attribute`, `path and rename cannot be combined`

The derive parses attributes strictly and reports mistakes at the attribute. Valid keys: `path`, `rename`, `with`, `nested`, `rest` on fields; `root`, `id` on structs. See [Typed projection](./typed-projection.md#errors).

## Surprises

### Two themes have the same fingerprint although a description changed

Fingerprints cover path, type, value, source and order, not metadata. Compare `ResolvedTokens` with `==` for metadata too.

### An overridden token lost its description

A later definition of a token replaces the earlier one whole, metadata included. Repeat `$description` in the override.

### An override in a dark file does not change a composite token

Composite values are replaced, never merged. Override the whole token, or override the palette tokens it aliases.

### `cargo add dxn_dtcg` finds no crate

The crates are not on crates.io. Depend on a release tag with a `git` dependency; see [Get started](./getting-started.md#install).

Next: [Errors](./errors.md), or [Limits and diagnostics](./limits.md).
