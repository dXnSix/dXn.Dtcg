---
title: DTCG in five minutes
description: The Design Tokens Community Group format in brief - tokens, groups, types, aliases, resolvers and modifiers - for Rust developers new to it.
group: Start
order: 20
---

Design tokens are the named visual decisions of a design system: colors, spacing, type, radii, shadows, motion. The Design Tokens Community Group (DTCG) standardizes a JSON format for them, so design tools, build pipelines and applications can share one source of truth. `dxn_dtcg` implements version 2025.10, which has two parts: the **format** for token files and **resolvers** that combine files and switch variants.

## Tokens and groups

A token is a JSON object with a `$value`. Every other object is a group. A token is named by the path of group names leading to it.

```json
{
  "color": {
    "$type": "color",
    "brand": {
      "primary": { "$value": { "colorSpace": "srgb", "components": [0.2, 0.4, 0.9] } }
    }
  }
}
```

This defines one token, `color.brand.primary`. It has no `$type` of its own and inherits `color` from its group. Names starting with `$` are reserved properties: `$value`, `$type`, `$description`, `$deprecated`, `$extensions`, `$root`, `$extends`, and `$schema` at the top of a file. Details: [Token documents](./token-documents.md).

## Types

Every token has one of 13 types: `color`, `dimension`, `fontFamily`, `fontWeight`, `duration`, `cubicBezier`, `number`, `strokeStyle`, `border`, `transition`, `shadow`, `gradient` and `typography`. The type fixes the shape of the value. A dimension is `{ "value": 16, "unit": "px" }`, a color is a color space plus three components. The library validates the shape after aliases resolve. See [Value types](./value-types.md).

## Aliases

A token's value can point at another token, so a semantic name like `button.background` can mean "the brand color":

```json
{
  "button": { "background": { "$type": "color", "$value": "{color.brand.primary}" } },
  "badge": { "background": { "$value": { "$ref": "#/button/background/$value" } } }
}
```

`{color.brand.primary}` is a curly-brace alias. `{ "$ref": "#/…" }` is a JSON Pointer reference, which can also reach into part of a value. Aliases resolve against the final merged document, so overriding the target changes every token that points to it. See [Aliases and references](./aliases.md).

## Resolvers, sets and modifiers

A resolver document combines token files and describes switchable variants:

```json
{
  "version": "2025.10",
  "sets": {
    "foundation": { "sources": [{ "$ref": "foundation.json" }] }
  },
  "modifiers": {
    "theme": {
      "default": "light",
      "contexts": {
        "light": [{ "$ref": "light.json" }],
        "dark": [{ "$ref": "dark.json" }]
      }
    }
  },
  "resolutionOrder": [{ "$ref": "#/sets/foundation" }, { "$ref": "#/modifiers/theme" }]
}
```

| Term | Meaning |
|---|---|
| Set | An ordered list of sources that is always applied |
| Modifier | A switch with named contexts, each with its own sources, and an optional default |
| `resolutionOrder` | The order in which sources merge; later sources win |
| Selection | Your choice of context per modifier, such as `[("theme", "dark")]` |
| Source | An inline token document, a file reference, a fragment of a file, a set, or a nested resolver |

Only the selected context's sources are applied and loaded. See [Resolvers and modifiers](./resolvers.md).

## What the library adds

DTCG says what documents look like and what a resolver does. `dxn_dtcg` is the runtime that does it: it finds the documents a selection needs, merges them, resolves aliases, validates values, and hands you tokens as `serde_json::Value`s, typed Rust models or your own structs. It performs no I/O; see [How it works](./how-it-works.md).

Next: [Which API do I use](./choosing-an-api.md), or [How it works](./how-it-works.md).
