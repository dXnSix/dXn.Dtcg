# 5. Value types

Every resolved token is checked against its effective type. This chapter lists the accepted types
and the exact rules for each one, the RayX extensions, and ways to turn validated values into Rust
types.

- [How validation works](#how-validation-works)
- [Summary](#summary)
- [`color`](#color)
- [`dimension`](#dimension)
- [`duration`](#duration)
- [`number`](#number)
- [`fontFamily`](#fontfamily)
- [`fontWeight`](#fontweight)
- [`cubicBezier`](#cubicbezier)
- [`strokeStyle`](#strokestyle)
- [`border`](#border)
- [`transition`](#transition)
- [`shadow`](#shadow)
- [`gradient`](#gradient)
- [`typography`](#typography)
- [`dev.rayx.blur` (RayX extension)](#devrayxblur-rayx-extension)
- [Unsupported types](#unsupported-types)
- [Validating values directly](#validating-values-directly)
- [Reading values in Rust](#reading-values-in-rust)

## How validation works

- Validation runs on the **resolved** value, after every alias and reference has been substituted.
  A border whose `color` is `"{palette.ink}"` is validated with the palette color in its place.
- Each token is validated as it is resolved. The first failure stops resolution with
  `DtcgError::TokenType`, which names the resource and the token path:
  ``token `spacing.md` in `tokens.json`: resolved value does not match DTCG type `dimension` ``.
- **Objects must match exactly.** Required members must be present, optional members may be, and no
  other member is allowed. `{ "value": 4, "unit": "px", "note": "x" }` is not a dimension.
- **Numbers must be JSON numbers.** `"16"` is a string, not a number.
- Validation does not change values. `ResolvedToken::value` is exactly the resolved JSON.

## Summary

| Type | Shape |
|---|---|
| `color` | `{ "colorSpace": …, "components": [c1, c2, c3], "alpha"?: 0..1, "hex"?: "#rrggbb" }` |
| `dimension` | `{ "value": number, "unit": "px" \| "rem" }` |
| `duration` | `{ "value": number, "unit": "ms" \| "s" }` |
| `number` | any JSON number |
| `fontFamily` | a string, or a non-empty array of strings |
| `fontWeight` | a number from 1 to 1000, or a weight keyword |
| `cubicBezier` | `[x1, y1, x2, y2]` with `x1` and `x2` in 0..1 |
| `strokeStyle` | a line-style keyword, or `{ "dashArray": [dimension, …], "lineCap": … }` |
| `border` | `{ "color": color, "width": dimension, "style": strokeStyle }` |
| `transition` | `{ "duration": duration, "delay": duration, "timingFunction": cubicBezier }` |
| `shadow` | a shadow object, or a non-empty array of them |
| `gradient` | a non-empty array of `{ "color": color, "position": number }` |
| `typography` | `{ "fontFamily", "fontSize", "fontWeight", "letterSpacing", "lineHeight" }` |
| `dev.rayx.blur` | `{ "kind": "layer" \| "background", "radius": dimension }` (RayX extension) |

## `color`

```json
{ "colorSpace": "oklch", "components": [0.62, 0.19, 259.8], "alpha": 0.9 }
```

| Member | Required | Rule |
|---|---|---|
| `colorSpace` | yes | One of the color spaces below |
| `components` | yes | Exactly 3 entries, each a number in the space's range or the string `"none"` |
| `alpha` | no | A number from 0 to 1 inclusive |
| `hex` | no | `#` followed by exactly 6 hex digits, either case. Not compared with `components`. |

| Color space | Component 1 | Component 2 | Component 3 |
|---|---|---|---|
| `srgb`, `srgb-linear`, `display-p3`, `a98-rgb`, `prophoto-rgb`, `rec2020`, `xyz-d65`, `xyz-d50` | 0 ≤ c ≤ 1 | 0 ≤ c ≤ 1 | 0 ≤ c ≤ 1 |
| `hsl` | hue 0 ≤ h < 360 | saturation 0 ≤ s ≤ 100 | lightness 0 ≤ l ≤ 100 |
| `hwb` | hue 0 ≤ h < 360 | whiteness 0 ≤ w ≤ 100 | blackness 0 ≤ b ≤ 100 |
| `lab` | lightness 0 ≤ L ≤ 100 | a: any | b: any |
| `lch` | lightness 0 ≤ L ≤ 100 | chroma C ≥ 0 | hue 0 ≤ h < 360 |
| `oklab` | lightness 0 ≤ L ≤ 1 | a: any | b: any |
| `oklch` | lightness 0 ≤ L ≤ 1 | chroma C ≥ 0 | hue 0 ≤ h < 360 |

Not accepted: CSS strings (`"#ff0000"`, `"rgb(255 0 0)"`), 8-digit or 3-digit `hex`, four
components, and alpha inside `components`.

## `dimension`

```json
{ "value": 16, "unit": "px" }
```

`value` is any number, negatives and fractions included. `unit` is `px` or `rem`. Other CSS units
(`em`, `%`, `vh`, `pt`) are rejected.

## `duration`

```json
{ "value": 200, "unit": "ms" }
```

`value` is any number. `unit` is `ms` or `s`.

## `number`

Any JSON number: `1.5`, `-2`, `400`. Strings and booleans are rejected.

## `fontFamily`

```json
"Inter"
```

```json
["Inter", "system-ui", "sans-serif"]
```

A single string, or a non-empty array in which every element is a string. `[]` is rejected.

## `fontWeight`

A number from 1 to 1000 inclusive (`650` is fine), or one of these keywords:

`thin`, `hairline`, `extra-light`, `ultra-light`, `light`, `normal`, `regular`, `book`, `medium`,
`semi-bold`, `demi-bold`, `bold`, `extra-bold`, `ultra-bold`, `black`, `heavy`, `extra-black`,
`ultra-black`.

## `cubicBezier`

```json
[0.4, 0, 0.2, 1]
```

Exactly four numbers `[x1, y1, x2, y2]`. `x1` and `x2` must be in 0..1 inclusive; `y1` and `y2` are
unrestricted, which allows overshoot curves such as `[0.34, 1.56, 0.64, 1]`.

## `strokeStyle`

A keyword: `solid`, `dashed`, `dotted`, `double`, `groove`, `ridge`, `outset`, `inset`. Or an
object:

```json
{ "dashArray": [{ "value": 4, "unit": "px" }, { "value": 2, "unit": "px" }], "lineCap": "round" }
```

`dashArray` is a non-empty array of dimensions. `lineCap` is `round`, `butt` or `square`. Both are
required.

## `border`

```json
{
  "color": { "colorSpace": "srgb", "components": [0.8, 0.82, 0.86] },
  "width": { "value": 1, "unit": "px" },
  "style": "solid"
}
```

All three members are required: a `color`, a `dimension` and a `strokeStyle`.

## `transition`

```json
{
  "duration": { "value": 200, "unit": "ms" },
  "delay": { "value": 0, "unit": "ms" },
  "timingFunction": [0.4, 0, 0.2, 1]
}
```

All three members are required: two `duration`s and a `cubicBezier`.

## `shadow`

```json
{
  "color": { "colorSpace": "srgb", "components": [0, 0, 0], "alpha": 0.15 },
  "offsetX": { "value": 0, "unit": "px" },
  "offsetY": { "value": 4, "unit": "px" },
  "blur": { "value": 12, "unit": "px" },
  "spread": { "value": 0, "unit": "px" },
  "inset": false
}
```

One shadow object, or a non-empty array of them for layered shadows. Each object needs `color`,
`offsetX`, `offsetY`, `blur` and `spread`, the last four being dimensions. `inset` is an optional
boolean.

## `gradient`

```json
[
  { "color": { "colorSpace": "srgb", "components": [0.15, 0.39, 0.92] }, "position": 0 },
  { "color": { "colorSpace": "srgb", "components": [0.49, 0.23, 0.93] }, "position": 1 }
]
```

A non-empty array of stops. Each stop has a `color` and a numeric `position`. Positions are not
range-checked or required to be sorted, so a renderer should clamp and sort them as it needs.

## `typography`

```json
{
  "fontFamily": ["Inter", "sans-serif"],
  "fontSize": { "value": 16, "unit": "px" },
  "fontWeight": 400,
  "letterSpacing": { "value": 0, "unit": "px" },
  "lineHeight": 1.5
}
```

All five members are required:

| Member | Type |
|---|---|
| `fontFamily` | `fontFamily` |
| `fontSize` | `dimension` |
| `fontWeight` | `fontWeight` |
| `letterSpacing` | `dimension` |
| `lineHeight` | a number (the DTCG unitless multiplier) **or a `dimension`** |

The `dimension` form of `lineHeight` is a RayX extension. Design-system line heights are often
pixel tokens that text styles alias, as in `"lineHeight": "{lineHeight.body}"`. Strings such as
`"auto"` or `"40px"`, and units other than `px` and `rem`, are rejected.

## `dev.rayx.blur` (RayX extension)

```json
{ "kind": "background", "radius": { "value": 20, "unit": "px" } }
```

A layer or background blur effect, as produced by the RayX Figma export. `kind` is `layer` or
`background`, and `radius` is a dimension. The type name is available as the constant
`dtcg_runtime::BLUR_TYPE`. Other `dev.rayx.*` names are not recognized.

## Unsupported types

Every token must end up with one of the types above. Anything else fails, for example:

- the pre-2025 `string` type (use `fontFamily` for font names);
- custom reverse-DNS types such as `com.example.elevation`;
- misspellings such as `colour` or `fontweight` (type names are case-sensitive).

The error is ``token `x` in `tokens.json`: unsupported DTCG type `string` for final 2025.10``.
To carry custom data, put it in `$extensions` and give the token a supported type, or read it
through a [JSON Pointer reference](04-aliases-and-references.md#references-to-arbitrary-locations).

## Validating values directly

`validate_value` is public, so editors and import tools can check a value before writing it:

```rust
use dtcg_runtime::{BLUR_TYPE, ValueValidationError, validate_value};
use serde_json::json;

assert!(validate_value("dimension", &json!({ "value": 4, "unit": "px" })).is_ok());
assert!(validate_value(BLUR_TYPE, &json!({ "kind": "layer", "radius": { "value": 8, "unit": "px" } })).is_ok());

assert_eq!(
    validate_value("dimension", &json!({ "value": 4, "unit": "em" })),
    Err(ValueValidationError::InvalidValue("dimension".into()))
);
assert_eq!(
    validate_value("string", &json!("Inter")).unwrap_err().to_string(),
    "unsupported DTCG type `string` for final 2025.10"
);
```

`validate_value` checks a fully resolved value. A value that still contains aliases, such as
`{ "color": "{palette.ink}", … }`, will not pass.

## Reading values in Rust

`ResolvedToken::value` is a `serde_json::Value`. Because every resolved value has already passed
the checks above, conversion code can rely on its shape. The functions below are typical; to have
them applied automatically, use them through [typed projection](08-typed-projection.md).

```rust
use std::time::Duration;

use dtcg_runtime::{DtcgError, DtcgRuntime, ResolvedToken};
use serde_json::Value;

fn projection_error(token: &ResolvedToken, message: &str) -> DtcgError {
    DtcgError::Projection { path: token.path.to_string(), message: message.into() }
}

/// sRGB colors as 8-bit RGBA. Other color spaces need a color-management library.
fn rgba8(token: &ResolvedToken) -> Result<[u8; 4], DtcgError> {
    let value = &token.value;
    if value["colorSpace"] != "srgb" {
        return Err(projection_error(token, "expected an sRGB color"));
    }
    let channel = |component: &Value| {
        // `"none"` (a missing component) is treated as zero here.
        (component.as_f64().unwrap_or(0.0) * 255.0).round() as u8
    };
    let components = &value["components"];
    let alpha = value["alpha"].as_f64().unwrap_or(1.0);
    Ok([
        channel(&components[0]),
        channel(&components[1]),
        channel(&components[2]),
        (alpha * 255.0).round() as u8,
    ])
}

/// Dimensions in logical pixels, with 1rem = 16px.
fn pixels(token: &ResolvedToken) -> f64 {
    let value = token.value["value"].as_f64().unwrap_or_default();
    if token.value["unit"] == "rem" { value * 16.0 } else { value }
}

fn duration(token: &ResolvedToken) -> Result<Duration, DtcgError> {
    let value = token.value["value"].as_f64().unwrap_or_default();
    let millis = if token.value["unit"] == "s" { value * 1000.0 } else { value };
    if millis < 0.0 {
        return Err(projection_error(token, "negative duration"));
    }
    Ok(Duration::from_micros((millis * 1000.0).round() as u64))
}

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "accent": { "$type": "color", "$value": { "colorSpace": "srgb", "components": [1, 0.5, 0], "alpha": 0.5 } },
          "gutter": { "$type": "dimension", "$value": { "value": 1.5, "unit": "rem" } },
          "fade": { "$type": "duration", "$value": { "value": 150, "unit": "ms" } }
        }"##,
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();

    assert_eq!(rgba8(tokens.get("accent").unwrap())?, [255, 128, 0, 128]);
    assert_eq!(pixels(tokens.get("gutter").unwrap()), 24.0);
    assert_eq!(duration(tokens.get("fade").unwrap())?, Duration::from_millis(150));
    Ok(())
}
```
