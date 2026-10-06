# 4. Aliases and references

Tokens can take their values from other tokens. DTCG 2025.10 has two syntaxes for this, and
`dtcg_runtime` supports both, as whole values and inside composite values.

- [The two syntaxes](#the-two-syntaxes)
- [Curly-brace aliases](#curly-brace-aliases)
- [JSON Pointer references](#json-pointer-references)
- [Token-level `$ref`](#token-level-ref)
- [References to arbitrary locations](#references-to-arbitrary-locations)
- [Adjusting a referenced value](#adjusting-a-referenced-value)
- [How a pointer is interpreted](#how-a-pointer-is-interpreted)
- [Aliases see the final token set](#aliases-see-the-final-token-set)
- [`authored_value` and `value`](#authored_value-and-value)
- [Cycles, depth and errors](#cycles-depth-and-errors)

## The two syntaxes

| Syntax | Example | Refers to |
|---|---|---|
| Curly-brace alias | `"{color.brand.primary}"` | The resolved value of a token, by token path |
| JSON Pointer reference | `{ "$ref": "#/color/brand/primary/$value" }` | A location in the document: a token's value or type, a part of a value, or any other JSON |

Both are resolved after all sources are stacked, and both may appear anywhere inside a value: as
the whole value, as an object member, or as an array element.

## Curly-brace aliases

A JSON string that **starts with `{` and ends with `}`** is an alias. The text between the braces
is a token path.

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime};

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
          "font": { "body": { "$type": "fontFamily", "$value": "Inter" } },

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
            "font": { "$type": "fontFamily", "$value": ["{font.body}", "system-ui", "sans-serif"] },
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

    assert_eq!(tokens.get("card.font").unwrap().value[0], "Inter");

    // A whole-value alias also takes the target's type.
    let outline = tokens.get("card.outline").unwrap();
    assert_eq!(outline.token_type.as_deref(), Some("border"));
    assert_eq!(outline.value, *border);
    Ok(())
}
```

Rules:

- **The whole string is the alias.** There is no interpolation: `"x {a}"` is the literal string
  `x {a}`. Conversely, a value such as `"{Inter}"` is always read as an alias, here to a token
  named `Inter`, and fails if there is none.
- **The target must be a token.** `{spacing}` names a group and fails with
  ``projection `spacing`: missing alias target``. Write `{spacing.$root}` to refer to a group's
  root token.
- **Targets are looked up in the final token set** of the current selection, whichever source
  defined them (see [below](#aliases-see-the-final-token-set)).
- **Type inheritance.** When a token's *entire* value is an alias and the token has no type of its
  own or from a group, it takes the target's type. Aliases nested inside a value do not affect the
  token's type.

## JSON Pointer references

A JSON object with a `$ref` member is a reference. Its `$ref` must be a local JSON Pointer
(RFC 6901) that starts with `#/`. In pointer segments, `~1` stands for `/` and `~0` for `~`.

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime};

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

    // Value-level reference to `…/$value`: the value and the type come from the target.
    let faded = tokens.get("faded").unwrap();
    assert_eq!(faded.value, 0.25);
    assert_eq!(faded.token_type.as_deref(), Some("number"));

    // Property-level references into another token's value.
    assert_eq!(tokens.get("tinted").unwrap().value["components"], serde_json::json!([0.25, 0.5, 1]));

    // A `…/$type` pointer yields the target's type name as a string.
    assert_eq!(tokens.get("kind").unwrap().value, "color");
    Ok(())
}
```

There is **no implicit `/$value` suffix**. `{ "$ref": "#/base/color" }` points at the token
*object* (`$type`, `$value` and all) and, for a color token, fails validation. Point at
`#/base/color/$value` instead.

## Token-level `$ref`

A token can be a reference itself: an object with `$ref` instead of `$value`. This is a whole-token
alias, written as a pointer.

```json
{
  "spacing": { "base": { "$type": "number", "$value": 4 } },
  "gap": { "$ref": "#/spacing/base/$value" }
}
```

- The `$ref` must be a string starting with `#/`. Otherwise the error is
  ``token-level `$ref` must be a string`` or ``token-level `$ref` must be a local JSON Pointer``.
- The token's type is its own `$type` or, when the pointer ends in `/$value`, the target's type.
  Unlike `$value` tokens, a token-level `$ref` does **not** inherit a group `$type`.
- `ResolvedToken::authored_value` is the whole authored token object, for example
  `{"$ref": "#/spacing/base/$value"}`.
- Other members such as `$description` and `$extensions` are allowed beside `$ref`.

## References to arbitrary locations

A pointer does not have to target a token. It can read any JSON in the source, for example a value
kept under `$extensions`. Such a token needs an explicit `$type`, because there is no target token
to take one from.

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "$extensions": { "org.example": { "grid/unit~v2": 8 } },
          "grid": { "$type": "number", "$ref": "#/$extensions/org.example/grid~1unit~0v2" }
        }"##,
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();
    assert_eq!(tokens.get("grid").unwrap().value, 8);
    Ok(())
}
```

The JSON found at the pointer is itself resolved, so it may contain further aliases and references.

## Adjusting a referenced value

A reference object may carry members besides `$ref`. The referenced value must then be an object,
and each extra member is resolved and set on a copy of it. The copy replaces the reference. This is
a shallow merge: a member replaces the whole member of the same name.

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime};

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

## How a pointer is interpreted

For each `$ref` pointer, the runtime splits it into segments and finds the first segment that is
`$value` or `$type`:

- If that segment is preceded by at least one name, and those names form the path of a token in
  the current token set
  (`#/base/color/$value` gives `base.color`), it is a **token reference**. The token is resolved
  from the final, overlaid token set, exactly like a curly-brace alias, and any further segments
  index into its resolved value.
- Otherwise it is a **document reference**. The pointer is evaluated against the raw JSON of the
  source that contains the reference: the whole document, or the fragment when the source is
  `file.json#/fragment`. The JSON found there is then resolved recursively.

So `#/base/color/$value` behaves the same wherever `base.color` is defined, but `#/$extensions/x`
only reads from the reference's own source.

## Aliases see the final token set

Aliases and references are resolved **after** every source of the selection has been stacked. An
alias therefore always sees the definition that won, even when the alias was written in an earlier
file than the override:

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime};

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
              "dark": [{ "palette": { "$type": "color",
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

This is why a theme only needs to override the handful of palette tokens that differ: every
semantic token that aliases them follows along.

## `authored_value` and `value`

| Field | Contains |
|---|---|
| `ResolvedToken::authored_value` | The `$value` as written, with aliases and references intact. For a token-level `$ref`, the whole token object. |
| `ResolvedToken::value` | The fully resolved value: no aliases, no `$ref` objects. This is what was validated. |

Editors use `authored_value` to show and preserve aliases. Renderers use `value`.

## Cycles, depth and errors

The runtime detects reference cycles and bounds alias chains:

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime};

fn main() {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r##"{ "a": { "$ref": "#/b/$value" }, "b": { "$ref": "#/a/$value" } }"##,
    )
    .unwrap();

    match runtime.resolve(&[]) {
        Err(DtcgError::Token { path, message, .. }) => {
            assert_eq!(path, "a");
            assert_eq!(message, "alias cycle: a -> b -> a");
        }
        other => panic!("expected a cycle, got {other:?}"),
    }
}
```

| Situation | Error |
|---|---|
| Alias or token reference to a path with no token (including a group path) | `Projection`: ``projection `<path>`: missing alias target`` |
| Tokens that alias each other in a loop | `Token`: `alias cycle: a -> b -> a` |
| Document pointers that reference each other in a loop | `InvalidPointer`: ``circular JSON Pointer reference `#/…` `` |
| A pointer to a location that does not exist | `InvalidPointer` with the pointer |
| A pointer that does not start with `#/`, or a bad `~` escape | `InvalidPointer` |
| A reference with extra members whose target is not an object | `InvalidPointer` with the pointer |
| More nested aliases than `RuntimeLimits::max_alias_depth` (default 64) | `LimitExceeded { kind: "alias depth", .. }` |

`max_alias_depth` counts the tokens being resolved at the same time on one chain, so a very long
chain that is resolved in authored order may stay under the limit while the same chain in reverse
order exceeds it. See [Errors, limits and diagnostics](11-errors-limits-and-diagnostics.md#runtime-limits).
