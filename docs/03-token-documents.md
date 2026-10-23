# 3. Token documents

This chapter describes the DTCG token format as `dxn_dtcg` reads it: how documents are
structured, how token paths are formed, which reserved properties are allowed, how several sources
combine into one document, how types are inherited, what `$root`, `$extends` and the metadata
properties do, and how a resolved set is written back as a document.

- [Structure](#structure)
- [Token paths](#token-paths)
- [Names](#names)
- [Reserved properties](#reserved-properties)
- [`$schema`](#schema)
- [Several sources, one merged document](#several-sources-one-merged-document)
- [Types and inheritance](#types-and-inheritance)
- [Root tokens: `$root`](#root-tokens-root)
- [Group extension: `$extends`](#group-extension-extends)
- [Metadata: `$description`, `$deprecated`, `$extensions`](#metadata-description-deprecated-extensions)
- [Token order](#token-order)
- [Exporting a resolved document](#exporting-a-resolved-document)
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

The document's top level is itself a group, the source root. The structural rules of Format §5
and §6 are enforced:

- **The source root is never a token.** A top-level `$value` or `$ref` fails with
  `a source root must not be a token`, and a top-level `$root` with `root token is invalid`.
- **Every child is an object.** `"spacing": 4` fails with `token document groups must be objects`.
- **Tokens are leaves.** A token must not have child members or a `$root` (``token `<path>` must not
  have children``), must not carry `$extends` (`` `$extends` is only allowed on groups``), and must
  not have both `$value` and `$ref`.
- **`$root` is a token.** A group's `$root` member must be a token object (`` `$root` must be a
  token``).

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
[Several sources, one merged document](#several-sources-one-merged-document)).

## Names

Token and group names must not be empty and must not contain `{`, `}` or `.`, because those
characters delimit alias syntax and token paths. A name that starts with `$` is a reserved
property and must be one the runtime knows; any other `$` name is an error.

```text
token `color.bad.name` in `tokens.json`: token and group names must not contain `{`, `}`, or `.`
token `g.` in `tokens.json`: token and group names must not be empty
token `g` in `tokens.json`: unknown reserved property `$version`
```

## Reserved properties

| Property | Allowed on | Must be | What the runtime does with it |
|---|---|---|---|
| `$value` | tokens | any JSON | Makes the object a token. The value is resolved and validated against the token's type. |
| `$ref` | tokens | a local JSON Pointer string (`#/…`) | Makes the object a token whose value is the referenced location. See [token-level `$ref`](04-aliases-and-references.md#token-level-ref). |
| `$type` | tokens, groups | a string | Sets the token's type, or the type the group's tokens inherit. |
| `$description` | tokens, groups, the source root | a string | Exposed as `ResolvedToken::description` and `ResolvedGroup::description`. |
| `$deprecated` | tokens, groups, the source root | `true`, `false` or a reason string | Exposed as `ResolvedToken::deprecated`, inherited by a group's tokens. |
| `$extensions` | tokens, groups, the source root | an object | Exposed as `ResolvedToken::extensions` and `ResolvedGroup::extensions`; readable through JSON Pointers. |
| `$root` | groups | a token object | Defines the group's root token, path `<group>.$root`. |
| `$extends` | groups | `"{group.path}"` or `"#/json/pointer"` | Makes the group start as a copy of another group. |
| `$schema` | the root of a whole document | a string | Ignored. See [`$schema`](#schema). |

Any other `$` member fails with `unknown reserved property`. A metadata member of the wrong JSON
type fails with, for example, `` `$description` must be a string``.

## `$schema`

Token files often name their JSON Schema in a top-level `"$schema"` member. The runtime accepts a
string `$schema` at the root of a source that is a whole document (a file provided to the catalog
and referenced without a fragment) and ignores it. It must be a string (`` `$schema` must be a
string``).

Anywhere else, `$schema` is an unknown reserved property: inside a group or token, at the root of a
fragment source (`library.json#/palette`), in an inline source written inside a resolver, and in
the inline source passed to a [preview](10-source-inspection.md). Resolver documents have their own
`$schema` member; see [Resolvers and modifiers](06-resolvers-and-modifiers.md).

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r#"{
          "$schema": "https://www.designtokens.org/schemas/2025.10/format.json",
          "radius": { "$type": "number", "$value": 4 }
        }"#,
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();
    assert_eq!(tokens.get("radius").unwrap().value, 4);
    assert_eq!(tokens.len(), 1);
    Ok(())
}
```

## Several sources, one merged document

A resolver applies several sources: sets, the selected modifier contexts, fragments and nested
resolvers (see [Resolvers and modifiers](06-resolvers-and-modifiers.md)). Following Resolver §6.2
and §6.3, the runtime folds all active sources, in resolution order, into **one merged token
structure**, and then reads that structure as if it were a single document:

- **Groups merge** member by member. A group that two sources define keeps the members of both.
- **Group properties** (`$type`, `$description`, `$deprecated`, `$extends`) replace per key: a later
  source's value wins, and a key a later source omits keeps the earlier value.
- **Group and root `$extensions` merge recursively.** Objects merge member by member at every
  depth, and at any member path a later non-object value (or a non-object meeting an object)
  replaces the earlier one: `{"vendor":{"a":1,"b":1}}` followed by `{"vendor":{"b":2}}` gives
  `{"vendor":{"a":1,"b":2}}`. A pointer into an earlier source's `$extensions` therefore keeps
  resolving when a later source adds other members. `$extends` applies the same rule.
- **Tokens are replaced whole.** A later definition of a token path keeps nothing of the earlier
  one: not its value, own type, description, deprecation or extensions.
- **A path is either a token or a group.** A path that is a token in one source and a group in
  another fails with `Token`: `path is a token in <a> and a group in <b>`.

Type inheritance, `$extends`, `$deprecated` inheritance and every JSON Pointer are evaluated on the
merged structure. In particular, a token that overrides another one inherits the group `$type`
that an earlier source declared, so an override does not need to repeat the type:

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "app.resolver.json",
        r##"{
          "version": "2025.10",
          "resolutionOrder": [
            { "type": "set", "name": "base", "sources": [{
              "size": { "$type": "number", "$description": "Scale", "sm": { "$value": 4 }, "lg": { "$value": 16 } }
            }] },
            { "type": "set", "name": "compact", "sources": [{
              "size": { "$description": "Compact scale", "lg": { "$value": 12 } }
            }] }
          ]
        }"##,
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();

    let large = tokens.get("size.lg").unwrap();
    assert_eq!(large.value, 12);
    assert_eq!(large.token_type.as_deref(), Some("number")); // from the base set's group
    assert_eq!(tokens.get("size.sm").unwrap().value, 4); // the group kept both members
    assert_eq!(tokens.group("size").unwrap().description.as_deref(), Some("Compact scale"));
    Ok(())
}
```

`ResolvedToken::source` still names the document of each token's winning definition.

## Types and inheritance

Every resolved token has exactly one DTCG type, its **effective type**. The runtime determines it
in this order (Format §5.2.2):

1. the token's own `$type`;
2. otherwise, if the whole value is a reference to a token (`"{path}"`,
   `{ "$ref": "#/path/$value" }` or a token-level `$ref` to `…/$value`), the type of that target;
3. otherwise the `$type` of the nearest enclosing group in the merged structure, after `$extends`
   is applied;
4. otherwise the token fails with `token requires a DTCG type`.

An own `$type` that differs from the type of a whole-value reference's target fails with
``reference type mismatch``. [Aliases and references](04-aliases-and-references.md#types-through-references)
covers the reference cases in detail.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "size": {
            "$type": "dimension",
            "sm": { "$value": { "value": 4, "unit": "px" } },
            "base": { "$type": "number", "$value": 4 },
            "half": { "$value": "{ratio.half}" }
          },
          "ratio": { "$type": "number", "half": { "$value": 0.5 } },
          "gap": { "$value": "{size.sm}" }
        }"##,
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();

    let token_type = |path: &str| tokens.get(path).unwrap().token_type.clone().unwrap();
    assert_eq!(&*token_type("size.sm"), "dimension"); // group
    assert_eq!(&*token_type("size.base"), "number"); // own
    assert_eq!(&*token_type("size.half"), "number"); // alias target, before the group
    assert_eq!(&*token_type("gap"), "dimension"); // alias target
    Ok(())
}
```

`$type` must be a string. Any type other than the 13 DTCG 2025.10 types fails during validation,
including the legacy `string` type and custom reverse-DNS types. The RayX extension type
`dev.rayx.blur` is accepted only by a runtime created with it. See [Value types](05-value-types.md).

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
group, not a token, and fails with `UnresolvedReference` (``unresolved reference `spacing`:
missing alias target``).

## Group extension: `$extends`

`$extends` makes a group start as a copy of another group: its members and its `$type`,
`$description`, `$deprecated` and `$extensions`. The extending group's own properties and members
then apply on top. Nested groups merge member by member, tokens replace copied tokens with the same
name, and `$extensions` objects merge recursively.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "buttons.json",
        r##"{
          "button": {
            "$type": "dimension",
            "$description": "Button spacing",
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
    // The group's properties are copied too.
    assert_eq!(
        tokens.group("linkButton").unwrap().description.as_deref(),
        Some("Button spacing")
    );
    Ok(())
}
```

Rules:

- **Syntax.** Use a curly-brace group path (`"{button}"`, `"{theme.base}"`) or a local JSON Pointer
  (`"#/button"`). A pointer is turned into a dotted group path. Anything else fails with
  `UnresolvedReference`: `$extends must be a group alias or local JSON Pointer`.
- **Target.** The target must be a group. A token, including a token-level `$ref` token, fails
  with ``$extends target `{…}` must be a group``. A missing target fails with
  `UnresolvedReference`: `extended group was not found`.
- **Scope.** The target is looked up in the [merged structure](#several-sources-one-merged-document),
  so a group can extend a group defined in another source or set, and a group that exists only
  because another group's `$extends` produced it.
- **Chains and cycles.** Extended groups may themselves use `$extends`. A cycle fails with, for
  example, `group extension cycle: b -> a -> b`. A group that would copy itself into itself, by
  extending itself or one of its ancestors, is also a cycle.
- **Order and source.** Copied tokens are placed in [token order](#token-order) where the group
  occurrence that declares `$extends` begins, and their `ResolvedToken::source` is the document of
  that `$extends`.
- **Incremental resolution.** If any active group uses `$extends`, affected re-resolution
  conservatively treats every token as affected (see
  [Incremental resolution](09-incremental-resolution.md)).

## Metadata: `$description`, `$deprecated`, `$extensions`

Resolved tokens and groups carry their metadata (Format §5.2):

| Field | Meaning |
|---|---|
| `ResolvedToken::description` | The token's own `$description`, or `None`. Never inherited. |
| `ResolvedToken::deprecated` | A `Deprecation`: the token's own `$deprecated`, else the nearest group's. `false` ends an inherited deprecation. |
| `ResolvedToken::extensions` | The token's own `$extensions` object, or `Value::Null`. Never inherited. |

`Deprecation` is `NotDeprecated`, `Deprecated` or `DeprecatedWithReason(reason)`, with
`is_deprecated()`, `reason()` and `to_json()`, which returns the matching `$deprecated` value.

`ResolvedTokens::group(path)` returns a `ResolvedGroup` for every group that contains at least one
resolved token, with `""` for the merged root, and `ResolvedTokens::groups()` lists them in path
order. A `ResolvedGroup` has:

| Field | Meaning |
|---|---|
| `path` | The dotted group path; empty for the root |
| `token_type` | The type its tokens inherit: its own `$type`, else its nearest ancestor's |
| `description` | Its `$description`, after merge and `$extends` |
| `deprecated` | Its own `$deprecated`, as `Some(Deprecation)`, or `None` |
| `extensions` | Its merged `$extensions` object, or `Value::Null` |

Metadata does not contribute to the [fingerprint](09-incremental-resolution.md#fingerprints).
Group-level and document-level `$extensions` can also be read through JSON Pointer references (see
[references to arbitrary locations](04-aliases-and-references.md#references-to-arbitrary-locations)).

```rust
use dxn_dtcg::{Deprecation, DtcgError, DtcgRuntime};

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
            },
            "overlay": { "$value": 0.6, "$deprecated": false }
          }
        }"##,
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();

    let disabled = tokens.get("opacity.disabled").unwrap();
    assert_eq!(disabled.description.as_deref(), Some("Disabled controls"));
    assert_eq!(disabled.deprecated.reason(), Some("Use emphasis.* instead")); // from the group
    assert_eq!(disabled.extensions["com.example.figma"]["variableId"], "VariableID:12:7");

    assert_eq!(tokens.get("opacity.overlay").unwrap().deprecated, Deprecation::NotDeprecated);

    let group = tokens.group("opacity").unwrap();
    assert_eq!(group.token_type.as_deref(), Some("number"));
    assert!(group.deprecated.as_ref().is_some_and(Deprecation::is_deprecated));
    Ok(())
}
```

## Token order

`ResolvedTokens::iter()` returns tokens in **resolution order**, which is deterministic. DTCG leaves
token order to implementations; this runtime uses the authored order:

1. Sources are visited in resolver order (see [Resolvers and modifiers](06-resolvers-and-modifiers.md)).
2. Within a source, members, `$root` included, are visited in authored JSON order. The crate
   preserves JSON member order, so this never depends on hashing.
3. Tokens copied by `$extends` are placed where the group occurrence that declares the `$extends`
   begins, before that occurrence's own tokens.
4. Each path appears once, at the position of its **last** definition. A token overridden by a
   later source moves to where the override is.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

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
              { "g": { "b": { "$value": 20 } } }
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

## Exporting a resolved document

`ResolvedTokens::to_document()` writes a resolution back as a nested DTCG 2025.10 token document
(Resolver §8), for example to hand a theme's final tokens to a tool that does not read resolvers:

- tokens are written in resolution order, each with an explicit `$type` and its resolved `$value`
  (no aliases or references), plus its own `$description` and `$extensions`;
- `$deprecated` is written on a token only where it differs from what the written groups imply;
- `$root` tokens are written as `$root` members;
- groups keep their own `$description`, `$deprecated` and `$extensions`, and the root keeps its
  merged `$extensions`. No `$schema` is written.

Resolving the exported document again yields the same paths, types, values and metadata. Because a
nested document keeps each group's tokens together, a resolution order that interleaves groups
(after a later source redefined a token) comes back grouped.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};
use serde_json::json;

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "size": { "$type": "dimension", "sm": { "$value": { "value": 4, "unit": "px" } } },
          "gap": { "$value": "{size.sm}", "$description": "Default gap" }
        }"##,
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();

    let exported = tokens.to_document();
    assert_eq!(
        exported["gap"],
        json!({ "$type": "dimension", "$value": { "value": 4, "unit": "px" }, "$description": "Default gap" })
    );

    let again = DtcgRuntime::from_json("export.json", exported.to_string())?
        .resolve(&[])?
        .ready()
        .unwrap();
    assert_eq!(again.get("gap").unwrap().value, tokens.get("gap").unwrap().value);
    Ok(())
}
```

## Errors

| Situation | Error |
|---|---|
| `$value` or `$ref` at the source root | `Token`: `a source root must not be a token` |
| `$root` at the source root | `InvalidDocument`: `root token is invalid` |
| A child that is not an object | `InvalidDocument`: `token document groups must be objects` |
| A token with child members or a `$root` | `Token`: ``token `<path>` must not have children`` |
| A token with `$extends` | `Token`: `` `$extends` is only allowed on groups`` |
| A token with both `$value` and `$ref` | `Token`: `` a token must not have both `$value` and `$ref` `` |
| A `$root` that is not a token | `Token`: `` `$root` must be a token`` |
| An empty name, or a name containing `{`, `}` or `.` | `Token`: `token and group names must not be empty` / `… must not contain …` |
| An unknown `$` member, including `$schema` outside a whole-document root | `Token`: ``unknown reserved property `$…` `` |
| `$type`, `$extends`, `$description` or `$schema` that is not a string | `Token`: `` `$type` must be a string`` and so on |
| `$deprecated` that is not a boolean or string | `Token`: `` `$deprecated` must be a boolean or a string`` |
| `$extensions` that is not an object | `Token`: `` `$extensions` must be an object`` |
| A path that is a token in one source and a group in another | `Token`: `path is a token in <a> and a group in <b>` |
| A token with no type from any source | `TokenType`: `token requires a DTCG type` |
| An own `$type` that differs from a whole-value reference's target type | `TokenType`: ``reference type mismatch: expected …, found …`` |
| A value that does not match its type | `TokenType`: ``resolved value does not match DTCG type `…` `` |
| An unsupported type | `TokenType`: ``unsupported DTCG type `…` for final 2025.10`` |
| An `$extends` target that does not exist | `UnresolvedReference`: `extended group was not found` |
| An `$extends` value that is neither `{…}` nor `#/…` | `UnresolvedReference`: `$extends must be a group alias or local JSON Pointer` |
| An `$extends` target that is a token | `Token`: ``$extends target `…` must be a group`` |
| An `$extends` cycle | `Token`: `group extension cycle: …` |
| More than `RuntimeLimits::max_tokens` flattened tokens, or `$extends` copies that exceed it | `LimitExceeded { kind: "tokens", .. }` |
| A flattened path deeper than 128 segments | `LimitExceeded { kind: "path depth", .. }` |

All error variants are described in [Errors, limits and diagnostics](11-errors-limits-and-diagnostics.md).
