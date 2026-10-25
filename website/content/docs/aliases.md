---
title: Aliases and references
description: Curly-brace aliases, where they may appear, JSON Pointer references, type determination, cycles and how aliases see the final token set.
group: Concepts
order: 60
---

A token can take its value from another token. DTCG 2025.10 has two syntaxes and `dxn_dtcg` supports both: curly-brace aliases as whole values and at the composite members the format allows, and JSON Pointer references at any depth. Both resolve after all active sources are merged.

| Syntax | Example | Refers to |
|---|---|---|
| Curly-brace alias | `"{color.brand.primary}"` | The resolved value of a token, by token path |
| JSON Pointer reference | `{ "$ref": "#/color/brand/primary/$value" }` | A location in the merged document: a token's value or type, part of a value, or any other JSON |

## Curly-brace aliases

A JSON string that starts with `{` and ends with `}` is an alias; the text between the braces is a token path.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "color": {
            "$type": "color",
            "border": { "$value": { "colorSpace": "srgb", "components": [0.8, 0.82, 0.86] } },
            "shadow": { "$value": { "colorSpace": "srgb", "components": [0, 0, 0], "alpha": 0.2 } }
          },
          "size": {
            "$type": "dimension",
            "hairline": { "$value": { "value": 1, "unit": "px" } },
            "none": { "$value": { "value": 0, "unit": "px" } },
            "blur": { "$value": { "value": 6, "unit": "px" } }
          },
          "card": {
            "border": {
              "$type": "border",
              "$value": { "color": "{color.border}", "width": "{size.hairline}", "style": "solid" }
            },
            "shadow": {
              "$type": "shadow",
              "$value": [{
                "color": "{color.shadow}", "offsetX": "{size.none}", "offsetY": "{size.hairline}",
                "blur": "{size.blur}", "spread": "{size.none}"
              }]
            },
            "outline": { "$value": "{card.border}" }
          }
        }"##,
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();

    let border = &tokens.get("card.border").unwrap().value;
    assert_eq!(border["width"]["value"], 1);
    assert_eq!(border["color"]["components"][0], 0.8);

    let shadow = &tokens.get("card.shadow").unwrap().value;
    assert_eq!(shadow[0]["color"]["alpha"], 0.2);

    // A whole-value alias also takes the target's type.
    let outline = tokens.get("card.outline").unwrap();
    assert_eq!(outline.token_type.as_deref(), Some("border"));
    assert_eq!(outline.value, *border);
    Ok(())
}
```

- **The whole string is the alias.** `"x {a}"` is the literal string `x {a}`; there is no interpolation. A value such as `"{Inter}"` is always an alias to a token named `Inter`.
- **The target must be a token.** `{spacing}` names a group and fails with `UnresolvedReference`, as does `{}`. Write `{spacing.$root}` for a group's root token.
- **Targets come from the final token set** of the current selection, whichever source defined them.
- **The resolved value is inserted as is.** An array-valued target is never spliced into a surrounding array.

## Where aliases are allowed

An alias may be a token's whole `$value`, or sit at these positions inside a composite value, where it must reference a token of the listed type:

| Token type | Position | Target type |
|---|---|---|
| `border` | `color` / `width` / `style` | `color` / `dimension` / `strokeStyle` |
| `transition` | `duration`, `delay` / `timingFunction` | `duration` / `cubicBezier` |
| `shadow` | `color` / `offsetX`, `offsetY`, `blur`, `spread` | `color` / `dimension` |
| `shadow` | an element of a shadow array | `shadow` |
| `gradient` | a stop's `color` / `position` | `color` / `number` |
| `gradient` | an element of the gradient array | `gradient` |
| `typography` | `fontFamily` / `fontSize`, `letterSpacing` / `fontWeight` / `lineHeight` | `fontFamily` / `dimension` / `fontWeight` / `number` |
| `strokeStyle` | an element of `dashArray` | `dimension` |

Everywhere else an alias fails with ``curly-brace references are not allowed inside <type> values``: color members, dimension and duration members, `cubicBezier` elements, `fontFamily` array elements, `lineCap`, shadow `inset` and a whole `dashArray`. An alias at an allowed position whose target has another type fails with ``reference type mismatch: expected <type>, found <type>``.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

fn resolve(json: &str) -> Result<(), DtcgError> {
    DtcgRuntime::from_json("tokens.json", json)?.resolve(&[]).map(|_| ())
}

fn main() {
    // An alias inside a fontFamily array is not a composite position.
    let error = resolve(
        r#"{
          "font": { "$type": "fontFamily", "brand": { "$value": "Inter" } },
          "stack": { "$type": "fontFamily", "$value": ["{font.brand}", "sans-serif"] }
        }"#,
    )
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "token `stack` in `tokens.json`: curly-brace references are not allowed inside fontFamily values"
    );

    // A border width must alias a dimension token.
    let error = resolve(
        r#"{
          "ink": { "$type": "color", "$value": { "colorSpace": "srgb", "components": [0, 0, 0] } },
          "one": { "$type": "number", "$value": 1 },
          "rule": { "$type": "border", "$value": { "color": "{ink}", "width": "{one}", "style": "solid" } }
        }"#,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        DtcgError::TokenType { ref path, ref message, .. }
            if path == "rule" && message == "reference type mismatch: expected dimension, found number"
    ));
}
```

To reuse part of a value at a position that does not admit aliases, use a JSON Pointer reference.

## Types through references

A token's effective type is its own `$type`, else the type of the token a **whole-value reference** targets, else its group's `$type`. A whole-value reference is a curly-brace alias, a `{ "$ref": "#/.../$value" }` object with no other members, or a token-level `$ref` to `.../$value`. Types follow chains: an untyped alias of an untyped alias takes the type at the end.

When a token has its own `$type` **and** a whole-value reference, the two must name the same type, compared by name and not by shape. A `fontWeight` token that aliases a `number` token fails even though `400` is a valid weight.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

fn main() -> Result<(), DtcgError> {
    let mismatch = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "weights": { "regular": { "$type": "number", "$value": 400 } },
          "heading": { "$type": "fontWeight", "$value": "{weights.regular}" }
        }"##,
    )?;
    let error = mismatch.resolve(&[]).unwrap_err();
    assert_eq!(
        error.to_string(),
        "token `heading` in `tokens.json`: reference type mismatch: expected fontWeight, found number"
    );
    Ok(())
}
```

## JSON Pointer references

An object with a `$ref` member is a reference. The `$ref` is a local RFC 6901 pointer in URI-fragment form, starting with `#/`. It may appear anywhere inside a value, and its target is not position-checked, because the resolved value as a whole is validated against the token's type.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "base": {
            "opacity": { "$type": "number", "$value": 0.25 },
            "color": { "$type": "color", "$value": { "colorSpace": "srgb", "components": [0.25, 0.5, 0.75] } }
          },
          "faded": { "$value": { "$ref": "#/base/opacity/$value" } },
          "tinted": {
            "$type": "color",
            "$value": {
              "colorSpace": "srgb",
              "components": [
                { "$ref": "#/base/color/$value/components/0" },
                { "$ref": "#/base/color/$value/components/1" },
                1
              ]
            }
          },
          "kind": { "$type": "fontFamily", "$value": { "$ref": "#/base/color/$type" } }
        }"##,
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();

    // Reference to `.../$value`: the value and the type come from the target.
    let faded = tokens.get("faded").unwrap();
    assert_eq!(faded.value, 0.25);
    assert_eq!(faded.token_type.as_deref(), Some("number"));

    // Pointers into another token's value, where a curly-brace alias is not allowed.
    assert_eq!(tokens.get("tinted").unwrap().value["components"], serde_json::json!([0.25, 0.5, 1]));

    // A `.../$type` pointer yields the target's type name as a string.
    assert_eq!(tokens.get("kind").unwrap().value, "color");
    Ok(())
}
```

:::warning No implicit /$value
`{ "$ref": "#/base/color" }` points at the token object, `$type` and `$value` included, and fails validation for a color. Point at `#/base/color/$value`.
:::

### Pointer syntax

One parser handles value-level `$ref`s, token-level `$ref`s and the fragments of file references such as `library.json#/brand`:

- `%XX` escapes are decoded first, so `#/brand%20colors` addresses the member `brand colors`. An unencoded space works too; a malformed `%` sequence is `InvalidPointer`.
- In each segment `~1` stands for `/` and `~0` for `~`; any other `~` is an error.
- An array index is `0` or a decimal number with no leading zero or sign. `01`, `+1` and `-1` are rejected, the index must exist, and the RFC's `-` never names an element.

### Token-level `$ref`

A token can be a reference itself: an object with `$ref` instead of `$value`.

```json
{
  "spacing": { "base": { "$type": "number", "$value": 4 } },
  "gap": { "$ref": "#/spacing/base/$value" }
}
```

Point at the target's `.../$value`. Its type follows the usual order (own `$type`, else the target token's type when the pointer ends in `/$value`, else the group's), `authored_value` is the whole authored token object, and metadata such as `$type`, `$description` and `$deprecated` may sit beside `$ref`.

### References to arbitrary locations

A pointer can read any JSON in the merged document, for example a value kept under `$extensions`. Such a token has no target token to take a type from, so it needs its own `$type` or a typed group. The JSON found at the pointer is itself resolved, so it may contain further aliases and references.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "$extensions": { "org.example": { "grid/unit~v2": 8, "columns": 12 } },
          "grid": { "$type": "number", "$ref": "#/$extensions/org.example/grid~1unit~0v2" },
          "layout": {
            "$type": "number",
            "columns": { "$ref": "#/$extensions/org.example/columns" }
          }
        }"##,
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();
    assert_eq!(tokens.get("grid").unwrap().value, 8);
    assert_eq!(tokens.get("layout.columns").unwrap().value, 12);
    Ok(())
}
```

### How a pointer is interpreted

The runtime finds the first `$value` or `$type` segment. If the names before it form the path of a token (`#/base/color/$value` gives `base.color`), it is a **token reference**: the token is resolved from the final token set exactly like an alias, and the remaining segments index into its resolved value. Otherwise it is a **document reference**, evaluated against the merged document, where later sources win.

## Adjusting a referenced value

A reference object may carry members besides `$ref`. The referenced value must then be an object; each extra member is resolved and set on a copy of it. The merge is shallow: a member replaces the whole member of the same name. This is how you take a color with a different alpha.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "brand": { "$type": "color", "$value": { "colorSpace": "srgb", "components": [0.2, 0.4, 0.8] } },
          "brandMuted": { "$type": "color", "$value": { "$ref": "#/brand/$value", "alpha": 0.5 } }
        }"##,
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();

    let muted = &tokens.get("brandMuted").unwrap().value;
    assert_eq!(muted["components"], serde_json::json!([0.2, 0.4, 0.8]));
    assert_eq!(muted["alpha"], 0.5);
    Ok(())
}
```

The extra members sit at the same positions as in a literal value, so the alias position rules apply to them: `"alpha": "{x}"` would be rejected inside a color.

## Aliases see the final token set

Because aliases resolve after every source is merged, an alias sees the definition that won, even when it was written in an earlier file than the override. This is why a theme only needs to override a handful of palette tokens:

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "theme.resolver.json",
        r##"{
          "version": "2025.10",
          "sets": {
            "base": { "sources": [{
              "palette": { "$type": "color",
                "primary": { "$value": { "colorSpace": "srgb", "components": [1, 0, 0] } } },
              "button": { "background": { "$value": "{palette.primary}" } }
            }] }
          },
          "modifiers": {
            "theme": { "default": "light", "contexts": {
              "light": [],
              "dark": [{ "palette": {
                "primary": { "$value": { "colorSpace": "srgb", "components": [0, 0, 1] } } } }]
            } }
          },
          "resolutionOrder": [{ "$ref": "#/sets/base" }, { "$ref": "#/modifiers/theme" }]
        }"##,
    )?;

    let light = runtime.resolve(&[])?.ready().unwrap();
    let dark = runtime.resolve(&[("theme", "dark")])?.ready().unwrap();

    // `button.background` is only defined in the base set, but follows the override.
    assert_eq!(light.get("button.background").unwrap().value["components"][0], 1);
    assert_eq!(dark.get("button.background").unwrap().value["components"][2], 1);
    Ok(())
}
```

`ResolvedToken::authored_value` keeps the value as written (aliases intact, or the whole token object for a token-level `$ref`); `value` is fully resolved with no aliases left. Editors show `authored_value`, renderers read `value`.

## Cycles and limits

The runtime detects reference cycles and bounds every chain.

| Situation | Error |
|---|---|
| Alias to a path with no token (group paths and `{}` included) | `UnresolvedReference`: ``unresolved reference `<path>`: missing alias target`` |
| Alias at a position that does not admit one | `Token`: ``curly-brace references are not allowed inside <type> values`` |
| Alias target of the wrong type, or own `$type` differing from the target's | `TokenType`: ``reference type mismatch: expected <type>, found <type>`` |
| Tokens aliasing each other in a loop | `Token`: `alias cycle: a -> b -> a` |
| Document pointers referencing each other in a loop | `InvalidPointer`: ``circular JSON Pointer reference `#/...` `` |
| Pointer to a location that does not exist | `InvalidPointer` |
| More than `max_alias_depth` hops (default 64) in one chain | `LimitExceeded { kind: "alias depth", .. }` |
| References copying more than `max_value_nodes` JSON nodes (default 4,000,000) | `LimitExceeded { kind: "value nodes", .. }` |
| A resolved value nested deeper than 128 levels | `LimitExceeded { kind: "value depth", .. }` |

`max_alias_depth` counts hops along one chain, whatever the syntax, and does not depend on authoring order. See [Limits and diagnostics](./limits.md).

Next: [Value types](./value-types.md), or [Resolvers and modifiers](./resolvers.md).
