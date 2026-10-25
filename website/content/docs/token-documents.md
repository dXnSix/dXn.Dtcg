---
title: Token documents
description: How dxn_dtcg reads token files - structure, paths, names, reserved properties, merging, type inheritance, $root, $extends, metadata, ordering and export.
group: Concepts
order: 50
---

A token document is a JSON object whose members are groups, tokens and reserved properties. This page covers the rules the runtime enforces, what happens when several sources merge, and how to write a resolution back out as a document.

## Structure

A member whose name starts with `$` is a reserved property. Any other child must be an object, and it is:

- a **token** if it has a `$value` member (or a token-level `$ref`, see [Aliases](./aliases.md#token-level-ref));
- a **group** otherwise.

```json
{
  "color": {
    "$type": "color",
    "$description": "Brand and UI colors",
    "brand": {
      "primary": { "$value": { "colorSpace": "srgb", "components": [0.15, 0.39, 0.92] } }
    }
  },
  "radius": {
    "md": { "$type": "dimension", "$value": { "value": 6, "unit": "px" } }
  }
}
```

The top level is itself a group, the source root. Rules the runtime enforces:

| Rule | Error message |
|---|---|
| The source root is never a token | `a source root must not be a token` (`root token is invalid` for a top-level `$root`) |
| Every child is an object | `token document groups must be objects` |
| Tokens are leaves: no child members, no `$root` | ``token `<path>` must not have children`` |
| `$extends` is only for groups | ``` `$extends` is only allowed on groups ``` |
| A token has `$value` or `$ref`, not both | ``a token must not have both `$value` and `$ref` `` |
| `$root` must be a token object | ``` `$root` must be a token ``` |

## Token paths and names

The runtime identifies a token by the names from the source root joined with `.`: `color.brand.primary`. A `$root` token has the path `spacing.$root`. Paths are case-sensitive Unicode and do not include the document name, so two documents that define `color.brand.primary` define the same token.

Names must not be empty and must not contain `{`, `}` or `.`. A name starting with `$` must be a known reserved property.

```text
token `color.bad.name` in `tokens.json`: token and group names must not contain `{`, `}`, or `.`
token `g` in `tokens.json`: unknown reserved property `$version`
```

## Reserved properties

| Property | On | Value | Effect |
|---|---|---|---|
| `$value` | tokens | any JSON | Makes the object a token |
| `$ref` | tokens | local JSON Pointer `#/...` | A token whose value is the referenced location |
| `$type` | tokens, groups | string | The token type, or the type the group's tokens inherit |
| `$description` | tokens, groups, root | string | `ResolvedToken::description`, `ResolvedGroup::description` |
| `$deprecated` | tokens, groups, root | `true`, `false` or a reason string | `ResolvedToken::deprecated`, inherited from groups |
| `$extensions` | tokens, groups, root | object | `ResolvedToken::extensions`, `ResolvedGroup::extensions` |
| `$root` | groups | token object | The group's own token, path `<group>.$root` |
| `$extends` | groups | `"{group.path}"` or `"#/pointer"` | The group starts as a copy of another group |
| `$schema` | root of a whole document | string | Accepted and ignored |

`$schema` is accepted only at the root of a document you provide to the catalog and reference without a fragment. Inside a group, in a fragment source, in an inline source of a resolver, or in a [preview](./inspection.md) source it is an unknown reserved property.

## Several sources, one merged document

A resolver applies several sources. The runtime folds them, in resolution order, into one merged structure and reads that as a single document:

- **Groups merge** member by member; both sources' members survive.
- **Group properties** (`$type`, `$description`, `$deprecated`, `$extends`) replace per key: a later value wins, an omitted key keeps the earlier value.
- **Group and root `$extensions` merge recursively**: objects merge member by member at every depth, and a later non-object value replaces the earlier one.
- **Tokens are replaced whole.** A later definition of a path keeps nothing of the earlier one: not its value, type, description, deprecation or extensions. Composite values are never deep-merged.
- **A path is a token or a group**, not both. A path that is a token in one source and a group in another fails with `path is a token in <a> and a group in <b>`.

Type inheritance, `$extends`, `$deprecated` inheritance and every JSON Pointer are evaluated on the merged structure. An override therefore inherits the group `$type` that an earlier source declared:

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

:::warning Tokens cannot be removed
Later sources add and replace tokens but cannot delete them. To switch a token off, replace it with a neutral value.
:::

## Types and inheritance

Every resolved token has one effective type, determined in this order:

1. the token's own `$type`;
2. otherwise, if the whole value is a reference to a token (`"{path}"`, `{ "$ref": "#/path/$value" }` or a token-level `$ref` to `.../$value`), the target's type;
3. otherwise the `$type` of the nearest enclosing group in the merged structure, after `$extends`;
4. otherwise the token fails with `token requires a DTCG type`.

An own `$type` that differs from the target's type is a `reference type mismatch`.

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

Any type outside the 13 DTCG types fails during validation, including the legacy `string` type and custom reverse-DNS types. See [Value types](./value-types.md).

## Root tokens: `$root`

A group can have a token of its own in its `$root` member, at the path `<group>.$root`. It inherits the group's type.

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

Refer to it by its full path. `{spacing}` names a group, not a token, and fails with `UnresolvedReference`.

## Group extension: `$extends`

`$extends` makes a group start as a copy of another group (members, `$type`, `$description`, `$deprecated`, `$extensions`); the extending group's own members then apply on top.

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

    assert_eq!(tokens.get("iconButton.paddingY").unwrap().value["value"], 8);
    assert_eq!(tokens.get("iconButton.paddingX").unwrap().value["value"], 8);
    assert_eq!(
        tokens.get("iconButton.paddingX").unwrap().token_type.as_deref(),
        Some("dimension")
    );
    assert_eq!(tokens.get("linkButton.paddingX").unwrap().value["value"], 16);
    assert_eq!(
        tokens.group("linkButton").unwrap().description.as_deref(),
        Some("Button spacing")
    );
    Ok(())
}
```

- Use `"{group.path}"` or a local pointer `"#/group/path"`. Anything else is `UnresolvedReference`.
- The target must be a group and must exist. It is looked up in the merged structure, so it can live in another source.
- Chains work; a cycle fails with `group extension cycle: b -> a -> b`.
- Copied tokens report the document of the `$extends` as their `source`.
- If any active group uses `$extends`, [affected re-resolution](./incremental.md) treats every token as affected.

## Metadata

| Field | Meaning |
|---|---|
| `ResolvedToken::description` | The token's own `$description`; never inherited |
| `ResolvedToken::deprecated` | A `Deprecation`: own `$deprecated`, else the nearest group's; `false` ends an inherited deprecation |
| `ResolvedToken::extensions` | The token's own `$extensions`, or `Value::Null`; never inherited |

`Deprecation` is `NotDeprecated`, `Deprecated` or `DeprecatedWithReason(reason)` with `is_deprecated()`, `reason()` and `to_json()`. `ResolvedTokens::group(path)` returns a `ResolvedGroup` (`path`, `token_type`, `description`, `deprecated`, `extensions`) for every group that contains at least one resolved token, with `""` for the merged root; `groups()` lists them in path order.

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
    Ok(())
}
```

## Token order

`ResolvedTokens::iter()` yields tokens in resolution order: sources in resolver order, members (`$root` included) in authored order, each path at the position of its **last** definition. A token overridden by a later source moves to where the override is. Tokens copied by `$extends` appear where the extending group begins.

The order is part of the fingerprint, so reordering members in a file changes it even if no value changes.

## Export

`ResolvedTokens::to_document()` writes a resolution back as a nested DTCG token document: every token with an explicit `$type` and resolved `$value` (no aliases), plus its own `$description` and `$extensions`; `$deprecated` only where it differs from what the written groups imply; `$root` tokens as `$root` members; no `$schema`. Resolving the export again gives the same paths, types, values and metadata. Use it to hand a theme's final tokens to a tool that does not read resolvers.

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
    Ok(())
}
```

Next: [Aliases and references](./aliases.md), then [Value types](./value-types.md).
