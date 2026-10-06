# 3. Token documents

This chapter describes the DTCG token format as `dtcg_runtime` reads it: how documents are
structured, how token paths are formed, which reserved properties are allowed, how types are
inherited, and what `$root`, `$extends` and the metadata properties do.

- [Structure](#structure)
- [Token paths](#token-paths)
- [Names](#names)
- [Reserved properties](#reserved-properties)
- [Types and inheritance](#types-and-inheritance)
- [Root tokens: `$root`](#root-tokens-root)
- [Group extension: `$extends`](#group-extension-extends)
- [Metadata: `$description`, `$deprecated`, `$extensions`](#metadata-description-deprecated-extensions)
- [Token order](#token-order)
- [Errors](#errors)

## Structure

A token document is a JSON object. Each member is either a reserved property (its name starts
with `$`) or a child. A child object is:

- a **token** if it has a `$value` member, or a token-level `$ref` (see
  [Aliases and references](04-aliases-and-references.md#token-level-ref));
- a **group** otherwise. Groups contain more groups and tokens.

```json
{
  "color": {
    "$type": "color",
    "$description": "Brand and UI colors",
    "brand": {
      "primary": { "$value": { "colorSpace": "srgb", "components": [0.15, 0.39, 0.92] } },
      "secondary": { "$value": { "colorSpace": "srgb", "components": [0.49, 0.23, 0.93] } }
    }
  },
  "radius": {
    "md": { "$type": "dimension", "$value": { "value": 6, "unit": "px" } }
  }
}
```

The document's top level is itself a group, the source root. Two rules follow from that:

- the top level cannot be a token. A `$value` (or a `$root`) at the top level fails with
  `root token is invalid`;
- every child must be an object. `"spacing": 4` fails with `token document groups must be
  objects`.

A token's members other than its reserved properties are ignored. A token has no children.

## Token paths

The runtime identifies each token by its **token path**: the names from the source root to the
token, joined with `.`.

| Authored location | Token path |
|---|---|
| `{"color": {"brand": {"primary": {"$value": …}}}}` | `color.brand.primary` |
| `{"spacing": {"$root": {"$value": …}}}` | `spacing.$root` |
| `{"größe": {"$value": …}}` | `größe` (paths are case-sensitive Unicode) |
| `palette.primary` inside `library.json`, used as source `library.json#/palette` | `primary` (the fragment is the source root) |

Paths do not include the resource id. Two documents that both define `color.brand.primary` define
the same token, and the later one in resolution order wins (see
[overlay semantics](06-resolvers-and-modifiers.md#overlay-semantics)).

## Names

Token and group names must not contain `{`, `}` or `.`, because those characters delimit alias
syntax and token paths. A name that starts with `$` is a reserved property and must be one the
runtime knows; any other `$` name is an error.

```text
token `color.bad.name` in `tokens.json`: token and group names must not contain `{`, `}`, or `.`
token `` in `tokens.json`: unknown reserved property `$schema`
```

> **Note**: many tools write a `"$schema"` member at the top of token files. The runtime rejects
> it like any other unknown reserved property. Strip it before providing the document, or have the
> exporter omit it. Resolver documents are not affected; only token documents are checked.

## Reserved properties

| Property | Allowed on | Must be | What the runtime does with it |
|---|---|---|---|
| `$value` | tokens | any JSON | Makes the object a token. The value is resolved and validated against the token's type. |
| `$ref` | tokens | a local JSON Pointer string (`#/…`) | Makes the object a token whose value is the referenced location. See [token-level `$ref`](04-aliases-and-references.md#token-level-ref). |
| `$type` | tokens, groups | a string | Sets the token's type, or the default type for everything in the group. |
| `$description` | tokens, groups | anything | Accepted and ignored. |
| `$deprecated` | tokens, groups | anything | Accepted and ignored. |
| `$extensions` | tokens, groups, the source root | anything | Token-level: exposed as `ResolvedToken::extensions`. Elsewhere: readable through JSON Pointer references only. |
| `$root` | groups | a token object | Defines the group's root token, path `<group>.$root`. |
| `$extends` | groups | `"{group.path}"` or `"#/json/pointer"` | Copies another group's tokens into this group. |

Any other `$` member on a token, group or source root fails with `unknown reserved property`.

## Types and inheritance

Every resolved token has exactly one DTCG type, its **effective type**. The runtime determines it
in this order:

1. the token's own `$type`;
2. otherwise the effective type of the nearest enclosing group. A group's effective type is its own
   `$type`; failing that, the effective type of the group it `$extends`; failing that, its parent
   group's effective type;
3. otherwise, if the whole value is an alias (`"{path}"` or `{ "$ref": "#/path/$value" }`), the
   type of the alias target;
4. otherwise the token fails with `token requires a DTCG type`.

Two consequences:

- An inherited group type wins over an alias target's type. In a group with
  `"$type": "dimension"`, a token aliasing a `number` token is validated as a dimension and fails.
- A token that overrides another one in a later source does **not** inherit anything from the
  token it replaces. Its type comes only from its own document: its `$type`, a group `$type` in
  that document, or its alias target.

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "size": {
            "$type": "dimension",
            "sm": { "$value": { "value": 4, "unit": "px" } },
            "base": { "$type": "number", "$value": 4 }
          },
          "gap": { "$value": "{size.sm}" }
        }"##,
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();

    assert_eq!(tokens.get("size.sm").unwrap().token_type.as_deref(), Some("dimension")); // group
    assert_eq!(tokens.get("size.base").unwrap().token_type.as_deref(), Some("number")); // own
    assert_eq!(tokens.get("gap").unwrap().token_type.as_deref(), Some("dimension")); // alias target
    Ok(())
}
```

`$type` must be a string. Any type other than the 13 DTCG 2025.10 types and the RayX extension
type `dev.rayx.blur` fails during validation, including the legacy `string` type and custom
reverse-DNS types. See [Value types](05-value-types.md).

## Root tokens: `$root`

A group can have a token of its own, written as its `$root` member. It sits at the path
`<group>.$root` and inherits the group's type like any child.

```json
{
  "spacing": {
    "$type": "dimension",
    "$root": { "$value": { "value": 8, "unit": "px" } },
    "lg": { "$value": { "value": 16, "unit": "px" } }
  },
  "gap": { "$value": "{spacing.$root}" }
}
```

Refer to a root token with its full path, `{spacing.$root}`. The alias `{spacing}` names the
group, not a token, and fails with ``projection `spacing`: missing alias target``.

## Group extension: `$extends`

`$extends` makes a group start as a copy of another group. The extending group's own `$root` and
children are then added on top, replacing copied tokens with the same name.

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "buttons.json",
        r##"{
          "button": {
            "$type": "dimension",
            "paddingX": { "$value": { "value": 16, "unit": "px" } },
            "paddingY": { "$value": { "value": 8, "unit": "px" } }
          },
          "iconButton": {
            "$extends": "{button}",
            "paddingX": { "$value": { "value": 8, "unit": "px" } }
          },
          "linkButton": { "$extends": "#/button" }
        }"##,
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();

    // Copied tokens keep the extended group's type; own children override copies.
    assert_eq!(tokens.get("iconButton.paddingY").unwrap().value["value"], 8);
    assert_eq!(tokens.get("iconButton.paddingX").unwrap().value["value"], 8);
    assert_eq!(
        tokens.get("iconButton.paddingX").unwrap().token_type.as_deref(),
        Some("dimension")
    );
    assert_eq!(tokens.get("linkButton.paddingX").unwrap().value["value"], 16);
    Ok(())
}
```

Rules:

- **Syntax.** Use a curly-brace group path (`"{button}"`, `"{theme.base}"`) or a local JSON Pointer
  (`"#/button"`). A pointer is turned into a dotted group path. Anything else fails with
  `$extends must be a group alias or local JSON Pointer`.
- **Target.** The target must be a group. Extending a token fails with
  ``$extends target `{…}` must be a group``.
- **Scope.** The target is looked up **in the same source** as the extending group: the same
  document, or the same fragment when the source is `file.json#/fragment`. A group defined in
  another file or another set cannot be extended, and the attempt fails with
  `extended group was not found`.
- **Types.** An extending group without its own `$type` takes the extended group's effective type
  before its parent's.
- **Chains and cycles.** Extended groups may themselves use `$extends`. A cycle fails with, for
  example, `group extension cycle: b -> a -> b`.
- **Incremental resolution.** If any active source uses `$extends`, affected re-resolution
  conservatively treats every token as affected (see
  [Incremental resolution](09-incremental-resolution.md)).

## Metadata: `$description`, `$deprecated`, `$extensions`

- `$description` and `$deprecated` are accepted on tokens and groups without validation. They are
  not part of `ResolvedToken`. Tools that display them can read the authored JSON, for example
  through [`inspect_source`](10-source-inspection.md).
- A token's `$extensions` object is copied to `ResolvedToken::extensions` (`Value::Null` when there
  is none). Since overrides replace whole tokens, the extensions of an overridden token are not
  merged into the winner.
- Group-level and document-level `$extensions` are not surfaced, but JSON Pointer references can
  read values from them (see
  [references to arbitrary locations](04-aliases-and-references.md#references-to-arbitrary-locations)).

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "opacity": {
            "$type": "number",
            "$deprecated": "Use emphasis.* instead",
            "disabled": {
              "$value": 0.4,
              "$description": "Disabled controls",
              "$extensions": { "com.example.figma": { "variableId": "VariableID:12:7" } }
            }
          }
        }"##,
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();
    let disabled = tokens.get("opacity.disabled").unwrap();

    assert_eq!(disabled.extensions["com.example.figma"]["variableId"], "VariableID:12:7");
    Ok(())
}
```

## Token order

`ResolvedTokens::iter()` returns tokens in **resolution order**, which is deterministic:

1. Sources are visited in resolver order (see [Resolvers and modifiers](06-resolvers-and-modifiers.md)).
2. Within a source, members are visited in authored JSON order. The crate preserves JSON member
   order, so this never depends on hashing.
3. Within a group, tokens copied by `$extends` come first, then the `$root` token, then the
   children.
4. Each path appears once, at the position of its **last** definition. A token overridden by a
   later source moves to where the override is.

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "order.resolver.json",
        r##"{
          "version": "2025.10",
          "resolutionOrder": [
            { "type": "set", "name": "base", "sources": [
              { "g": { "$type": "number", "b": { "$value": 2 }, "$root": { "$value": 0 }, "a": { "$value": 1 } } }
            ] },
            { "type": "set", "name": "brand", "sources": [
              { "g": { "$type": "number", "b": { "$value": 20 } } }
            ] }
          ]
        }"##,
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();

    let order: Vec<&str> = tokens.iter().map(|token| &*token.path).collect();
    assert_eq!(order, ["g.$root", "g.a", "g.b"]); // g.b moved to its override
    assert_eq!(tokens.get("g.b").unwrap().value, 20);
    Ok(())
}
```

The order is part of the [fingerprint](09-incremental-resolution.md#fingerprints), so reordering
members in a file changes it even if no value changes.

## Errors

| Situation | Error |
|---|---|
| `$value` or `$root` at the source root | `InvalidDocument`: `root token is invalid` |
| A child that is not an object | `InvalidDocument`: `token document groups must be objects` |
| A name containing `{`, `}` or `.` | `Token`: `token and group names must not contain …` |
| An unknown `$` member (including `$schema`) | `Token`: ``unknown reserved property `$…` `` |
| `$type` or `$extends` that is not a string | `Token`: `` `$type` must be a string`` / `` `$extends` must be a string`` |
| A token with no type from any source | `TokenType`: `token requires a DTCG type` |
| A value that does not match its type | `TokenType`: ``resolved value does not match DTCG type `…` `` |
| An unsupported type | `TokenType`: ``unsupported DTCG type `…` for final 2025.10`` |
| An `$extends` target that is missing or in another source | `Projection` (or `InvalidDocument` when the extending group has its own `$type`): `extended group was not found` |
| An `$extends` value that is neither `{…}` nor `#/…` | `Projection`: `$extends must be a group alias or local JSON Pointer` |
| An `$extends` target that is a token | `Token`: ``$extends target `…` must be a group`` |
| An `$extends` cycle | `Token`: `group extension cycle: …` |

All error variants are described in [Errors, limits and diagnostics](11-errors-limits-and-diagnostics.md).
