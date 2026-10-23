# 5. Value types

Every resolved token is checked against its effective type. This chapter lists the accepted types
and the exact rules for each one, the opt-in RayX extensions, and the typed Rust models in the
`values` module that validated values convert into.

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
- [Strict values and the RayX extensions](#strict-values-and-the-rayx-extensions)
- [Unsupported types](#unsupported-types)
- [Validating values directly](#validating-values-directly)
- [Reading values in Rust](#reading-values-in-rust)

## How validation works

- Validation is **strict DTCG 2025.10** by default. The two RayX value extensions are accepted only
  when the runtime is created with them (see
  [Strict values and the RayX extensions](#strict-values-and-the-rayx-extensions)).
- Validation runs on the **resolved** value, after every alias and reference has been substituted.
  A border whose `color` is `"{palette.ink}"` is validated with the palette color in its place.
  Where a curly-brace alias may appear inside a value, and which type it must target, is described
  in [Aliases and references](04-aliases-and-references.md#where-curly-brace-aliases-are-allowed).
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
| `dev.rayx.blur` | `{ "kind": "layer" \| "background", "radius": dimension }` (RayX extension, opt-in) |

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

Hues must lie in [0, 360): a hue of 360 or more is rejected, not wrapped. The `xyz-d65` and
`xyz-d50` components are bounded to [0, 1] like the RGB spaces.

Not accepted: CSS strings (`"#ff0000"`, `"rgb(255 0 0)"`), 8-digit or 3-digit `hex`, four
components, and alpha inside `components`. No color member may be a curly-brace alias; use a
[JSON Pointer reference](04-aliases-and-references.md#json-pointer-references) to reuse a component
of another color.

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

A single string, or a non-empty array in which every element is a string. `[]` is rejected. A
whole `fontFamily` value may be an alias to another `fontFamily` token, but an element of the array
may not: `["{font.brand}", "sans-serif"]` fails with
``curly-brace references are not allowed inside fontFamily values``.

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

A non-empty array of stops. Each stop has a `color` and a numeric `position`. Positions outside
[0, 1] are valid and need not be sorted. The resolved JSON keeps the authored numbers; the typed
[`values::Gradient`](#reading-values-in-rust) clamps each position to [0, 1], as Format §9.7 asks
consumers to do.

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
| `lineHeight` | a number, the DTCG unitless multiplier of the font size |

Strings such as `"auto"` or `"40px"` are rejected. An alias at `lineHeight` must target a `number`
token.

With the RayX `line_height_dimension` extension, `lineHeight` may also be a `dimension`, and a
`lineHeight` alias may target a `dimension` token. Design-system line heights are often pixel
tokens that text styles alias, as in `"lineHeight": "{lineHeight.body}"`. Without the extension,
such a value fails validation and such an alias fails with
``reference type mismatch: expected number, found dimension``.

## `dev.rayx.blur` (RayX extension)

```json
{ "kind": "background", "radius": { "value": 20, "unit": "px" } }
```

A layer or background blur effect, as produced by the RayX Figma export. `kind` is `layer` or
`background`, and `radius` is a dimension; `radius` may alias a `dimension` token. The type name is
available as the constant `dxn_dtcg::BLUR_TYPE`. The type exists only with the RayX
`rayx_blur` extension; a default runtime reports it as an
[unsupported type](#unsupported-types). Other `dev.rayx.*` names are never recognized.

## Strict values and the RayX extensions

A runtime validates strictly unless you pass options. `ValueExtensions` holds the two RayX value
extensions, both off by default:

| Flag | Setter | Effect |
|---|---|---|
| `line_height_dimension` | `with_line_height_dimension(bool)` | Typography `lineHeight` may be a `dimension` or alias a `dimension` token |
| `rayx_blur` | `with_rayx_blur(bool)` | The `dev.rayx.blur` type is accepted |

`ValueExtensions::rayx()` turns both on; it is the profile RayX and the RayX Token Editor use.
Extensions reach a runtime through `RuntimeOptions`, which also carries the
[limits](11-errors-limits-and-diagnostics.md#runtime-limits):

- `DtcgRuntime::with_options(options)`, `DtcgRuntime::from_json_with_options(id, json, options)`
  and `DtcgRuntime::from_envelope_with_options(id, json, options)` create a runtime with them;
- `DtcgRuntime::options()` returns them;
- `DtcgRuntime::default()`, `from_json`, `from_envelope` and `with_limits` use strict values.

```rust
use dxn_dtcg::{BLUR_TYPE, DtcgError, DtcgRuntime, RuntimeOptions, ValueExtensions};

const TOKENS: &str = r##"{
  "leading": { "$type": "dimension", "$value": { "value": 24, "unit": "px" } },
  "body": {
    "$type": "typography",
    "$value": {
      "fontFamily": "Inter",
      "fontSize": { "value": 16, "unit": "px" },
      "fontWeight": 400,
      "letterSpacing": { "value": 0, "unit": "px" },
      "lineHeight": "{leading}"
    }
  },
  "frost": {
    "$type": "dev.rayx.blur",
    "$value": { "kind": "background", "radius": { "value": 20, "unit": "px" } }
  }
}"##;

fn main() -> Result<(), DtcgError> {
    // Strict DTCG 2025.10: the line height must alias a number token.
    let strict = DtcgRuntime::from_json("tokens.json", TOKENS)?;
    assert_eq!(
        strict.resolve(&[]).unwrap_err().to_string(),
        "token `body` in `tokens.json`: reference type mismatch: expected number, found dimension"
    );

    // The RayX profile accepts both extensions.
    let rayx = DtcgRuntime::from_json_with_options(
        "tokens.json",
        TOKENS,
        RuntimeOptions::default().with_extensions(ValueExtensions::rayx()),
    )?;
    assert_eq!(rayx.options().extensions, ValueExtensions::rayx());

    let tokens = rayx.resolve(&[])?.ready().unwrap();
    assert_eq!(tokens.get("body").unwrap().value["lineHeight"]["value"], 24);
    assert_eq!(tokens.get("frost").unwrap().token_type.as_deref(), Some(BLUR_TYPE));
    Ok(())
}
```

Each flag is independent, for example
`ValueExtensions::default().with_line_height_dimension(true)` accepts dimension line heights but
not the blur type. Every other rule stays strict with the extensions on.

## Unsupported types

Every token must end up with one of the 13 DTCG types, or `dev.rayx.blur` when the `rayx_blur`
extension is on. Anything else fails, for example:

- the pre-2025 `string` type (use `fontFamily` for font names);
- `dev.rayx.blur` in a runtime without the extension;
- custom reverse-DNS types such as `com.example.elevation`;
- misspellings such as `colour` or `fontweight` (type names are case-sensitive).

The error is ``token `x` in `tokens.json`: unsupported DTCG type `string` for final 2025.10``.
To carry custom data, put it in `$extensions` and give the token a supported type, or read it
through a [JSON Pointer reference](04-aliases-and-references.md#references-to-arbitrary-locations).

## Validating values directly

`validate_value` and `validate_value_with` are public, so editors and import tools can check a
value before writing it. `validate_value` is strict DTCG 2025.10; `validate_value_with` applies the
given `ValueExtensions`:

```rust
use dxn_dtcg::{
    BLUR_TYPE, ValueExtensions, ValueValidationError, validate_value, validate_value_with,
};
use serde_json::json;

assert!(validate_value("dimension", &json!({ "value": 4, "unit": "px" })).is_ok());

let blur = json!({ "kind": "layer", "radius": { "value": 8, "unit": "px" } });
assert_eq!(
    validate_value(BLUR_TYPE, &blur),
    Err(ValueValidationError::UnsupportedType(BLUR_TYPE.into()))
);
assert!(validate_value_with(BLUR_TYPE, &blur, &ValueExtensions::rayx()).is_ok());

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
the checks above, conversion code can rely on its shape.

### Typed values: the `values` module

`dxn_dtcg::values` models every type as plain Rust data, and `ResolvedToken::typed(&extensions)`
converts a resolved token by its effective type into a `TypedValue` enum with one variant per type.
`TypedValue::from_resolved(type, &value, &extensions)` does the same for any value: it validates
first and returns a `ValueValidationError` for a value that does not match. Pass the extensions the
runtime was created with, `runtime.options().extensions`, so the same shapes are accepted.

| Type | Model | Notes |
|---|---|---|
| `color` | `Color { color_space, components, alpha, hex }` | `ColorSpace` has the 14 spaces; each `Component` is `Number(f64)` or `None`; a missing `alpha` is `1.0` |
| `dimension` | `Dimension { value, unit }` | `DimensionUnit::{Px, Rem}`; `to_px(rem_base)` converts |
| `duration` | `Duration { value, unit }` | `DurationUnit::{Ms, S}`; `to_millis()` converts |
| `number` | `TypedValue::Number(f64)` | |
| `fontFamily` | `FontFamily(Vec<String>)` | a single name becomes a one-element list |
| `fontWeight` | `FontWeight(f64)` | keywords map to numbers (`thin` 100 … `extra-black` 950); `FontWeight::keyword(name)` |
| `cubicBezier` | `CubicBezier([f64; 4])` | |
| `strokeStyle` | `StrokeStyle::Style(LineStyle)` or `StrokeStyle::Dashes { dash_array, line_cap }` | |
| `border` | `Border { color, width, style }` | |
| `transition` | `Transition { duration, delay, timing_function }` | |
| `shadow` | `Shadow(Vec<ShadowLayer>)` | a single shadow object is one layer; `inset` defaults to `false` |
| `gradient` | `Gradient(Vec<GradientStop>)` | positions clamped to [0, 1] |
| `typography` | `Typography { font_family, font_size, font_weight, letter_spacing, line_height }` | `LineHeight::Multiplier(f64)`, or `LineHeight::Dimension` with the RayX extension |
| `dev.rayx.blur` | `Blur { kind, radius }` | `BlurKind::{Layer, Background}`; RayX extension |

```rust
use dxn_dtcg::values::{
    Color, ColorSpace, Component, DimensionUnit, FontWeight, LineHeight, Shadow, TypedValue,
};
use dxn_dtcg::{DtcgError, DtcgRuntime};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "accent": { "$type": "color", "$value": { "colorSpace": "srgb", "components": [1, 0.5, "none"], "hex": "#FF8000" } },
          "gutter": { "$type": "dimension", "$value": { "value": 1.5, "unit": "rem" } },
          "strong": { "$type": "fontWeight", "$value": "semi-bold" },
          "raised": { "$type": "shadow", "$value": {
            "color": { "colorSpace": "srgb", "components": [0, 0, 0], "alpha": 0.2 },
            "offsetX": { "value": 0, "unit": "px" }, "offsetY": { "value": 2, "unit": "px" },
            "blur": { "value": 4, "unit": "px" }, "spread": { "value": 0, "unit": "px" }
          } },
          "body": { "$type": "typography", "$value": {
            "fontFamily": "Inter", "fontSize": { "value": 16, "unit": "px" }, "fontWeight": 400,
            "letterSpacing": { "value": 0, "unit": "px" }, "lineHeight": 1.5
          } }
        }"##,
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();
    let extensions = &runtime.options().extensions;
    let typed = |path: &str| tokens.get(path).unwrap().typed(extensions);

    let TypedValue::Color(Color { color_space, components, alpha, hex }) = typed("accent")? else {
        unreachable!()
    };
    assert_eq!(color_space, ColorSpace::Srgb);
    assert_eq!(components, [Component::Number(1.0), Component::Number(0.5), Component::None]);
    assert_eq!(alpha, 1.0); // no `alpha` member
    assert_eq!(hex.as_deref(), Some("#FF8000"));

    let TypedValue::Dimension(gutter) = typed("gutter")? else { unreachable!() };
    assert_eq!(gutter.unit, DimensionUnit::Rem);
    assert_eq!(gutter.to_px(16.0), 24.0);

    assert_eq!(typed("strong")?, TypedValue::FontWeight(FontWeight(600.0)));

    let TypedValue::Shadow(Shadow(layers)) = typed("raised")? else { unreachable!() };
    assert_eq!(layers.len(), 1);
    assert!(!layers[0].inset);

    let TypedValue::Typography(body) = typed("body")? else { unreachable!() };
    assert_eq!(body.font_family.0, ["Inter"]);
    assert_eq!(body.line_height, LineHeight::Multiplier(1.5));
    Ok(())
}
```

`typed` reports a value that does not convert as `DtcgError::Projection` naming the token path.
Every model also implements `FromDtcgValue`, so it can be a field of a
[`#[derive(DtcgTokens)]`](08-typed-projection.md) struct, and `TypedValue` itself can be one.
These implementations check only the value's shape, accepting the RayX extension shapes, because
the runtime has already validated the value with its own options.

### Converting the JSON yourself

When you need a representation of your own, such as 8-bit RGBA or `std::time::Duration`, read the
JSON directly. The functions below are typical; to have them applied automatically, use them
through [typed projection](08-typed-projection.md).

```rust
use std::time::Duration;

use dxn_dtcg::{DtcgError, DtcgRuntime, ResolvedToken};
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
