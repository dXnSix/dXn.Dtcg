---
title: Value types
description: The 13 DTCG value types and their exact validation rules, strict defaults, the opt-in value extensions and the typed values module.
group: Concepts
order: 70
---

Every resolved token is checked against its effective type after aliases resolve. This page lists the accepted shapes, the opt-in extensions, direct validation, the typed Rust models you read values with, and the color conversions on `values::Color`.

## How validation works

- Validation is **strict DTCG 2025.10** unless you pass extensions.
- It runs on the **resolved** value. A border whose `color` is `"{palette.ink}"` is validated with the palette color in its place.
- The first failure stops resolution with `DtcgError::TokenType`: ``token `spacing.md` in `tokens.json`: resolved value does not match DTCG type `dimension` ``.
- **Objects must match exactly**: required members present, optional members allowed, nothing else. `{ "value": 4, "unit": "px", "note": "x" }` is not a dimension.
- **Numbers must be JSON numbers.** `"16"` is a string.
- Validation never changes values: `ResolvedToken::value` is the resolved JSON as is.

| Type | Shape |
|---|---|
| `color` | `{ "colorSpace", "components": [c1, c2, c3], "alpha"?: 0..1, "hex"?: "#rrggbb" }` |
| `dimension` | `{ "value": number, "unit": "px" \| "rem" }` |
| `duration` | `{ "value": number, "unit": "ms" \| "s" }` |
| `number` | any JSON number |
| `fontFamily` | a string, or a non-empty array of strings |
| `fontWeight` | a number from 1 to 1000, or a weight keyword |
| `cubicBezier` | `[x1, y1, x2, y2]` with `x1` and `x2` in 0..1 |
| `strokeStyle` | a line-style keyword, or `{ "dashArray": [dimension, ...], "lineCap" }` |
| `border` | `{ "color", "width", "style" }` |
| `transition` | `{ "duration", "delay", "timingFunction" }` |
| `shadow` | a shadow object, or a non-empty array of them |
| `gradient` | a non-empty array of `{ "color", "position": number }` |
| `typography` | `{ "fontFamily", "fontSize", "fontWeight", "letterSpacing", "lineHeight" }` |
| `dxn.dtcg.blur` | `{ "kind": "layer" \| "background", "radius": dimension }` (opt-in extension) |

## color

```json
{ "colorSpace": "oklch", "components": [0.62, 0.19, 259.8], "alpha": 0.9 }
```

`components` has exactly 3 entries, each a number in the space's range or the string `"none"`. `alpha` is a number from 0 to 1. `hex` is `#` plus exactly 6 hex digits in either case, and is not compared with `components`.

| Color space | Component 1 | Component 2 | Component 3 |
|---|---|---|---|
| `srgb`, `srgb-linear`, `display-p3`, `a98-rgb`, `prophoto-rgb`, `rec2020`, `xyz-d65`, `xyz-d50` | 0 to 1 | 0 to 1 | 0 to 1 |
| `hsl`, `hwb` | hue 0 to <360 | 0 to 100 | 0 to 100 |
| `lab` | lightness 0 to 100 | any | any |
| `lch` | lightness 0 to 100 | chroma >= 0 | hue 0 to <360 |
| `oklab` | lightness 0 to 1 | any | any |
| `oklch` | lightness 0 to 1 | chroma >= 0 | hue 0 to <360 |

A hue of 360 or more is rejected, not wrapped. CSS strings (`"#ff0000"`, `"rgb(255 0 0)"`), 3- or 8-digit `hex`, four components and alpha inside `components` are not accepted, and no color member may be an alias; use a [JSON Pointer reference](./aliases.md#json-pointer-references) to reuse a component.

## dimension and duration

```json
{ "value": 16, "unit": "px" }
```

`value` is any number, negatives and fractions included. A dimension's `unit` is `px` or `rem`; a duration's is `ms` or `s`. Other CSS units (`em`, `%`, `vh`, `pt`) are rejected.

## number, fontFamily, fontWeight

`number` is any JSON number. `fontFamily` is `"Inter"` or `["Inter", "system-ui", "sans-serif"]`; the array must be non-empty and its elements cannot be aliases (the whole value can alias another `fontFamily` token). `fontWeight` is a number from 1 to 1000 or one of: `thin`, `hairline`, `extra-light`, `ultra-light`, `light`, `normal`, `regular`, `book`, `medium`, `semi-bold`, `demi-bold`, `bold`, `extra-bold`, `ultra-bold`, `black`, `heavy`, `extra-black`, `ultra-black`.

## cubicBezier

`[0.4, 0, 0.2, 1]`: exactly four numbers; `x1` and `x2` in 0..1 inclusive, `y1` and `y2` unrestricted, so overshoot curves like `[0.34, 1.56, 0.64, 1]` are valid.

## strokeStyle and border

A `strokeStyle` is a keyword (`solid`, `dashed`, `dotted`, `double`, `groove`, `ridge`, `outset`, `inset`) or an object with both members: a non-empty `dashArray` of dimensions and a `lineCap` of `round`, `butt` or `square`.

```json
{
  "color": { "colorSpace": "srgb", "components": [0.8, 0.82, 0.86] },
  "width": { "value": 1, "unit": "px" },
  "style": "solid"
}
```

A `border` requires all three members: a color, a dimension and a strokeStyle.

## transition

```json
{
  "duration": { "value": 200, "unit": "ms" },
  "delay": { "value": 0, "unit": "ms" },
  "timingFunction": [0.4, 0, 0.2, 1]
}
```

All three members are required.

## shadow and gradient

A `shadow` is one object or a non-empty array of them for layered shadows. Each needs `color`, `offsetX`, `offsetY`, `blur` and `spread` (the last four dimensions); `inset` is an optional boolean.

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

A `gradient` is a non-empty array of stops, each with a `color` and a numeric `position`. Positions outside [0, 1] are valid and need not be sorted. The resolved JSON keeps the authored numbers; the typed `values::Gradient` clamps each position to [0, 1].

## typography

All five members are required:

| Member | Type |
|---|---|
| `fontFamily` | `fontFamily` |
| `fontSize` | `dimension` |
| `fontWeight` | `fontWeight` |
| `letterSpacing` | `dimension` |
| `lineHeight` | a number, the unitless multiplier of the font size |

Strings such as `"auto"` or `"40px"` are rejected, and an alias at `lineHeight` must target a `number` token. Many design systems store line heights in pixels; that needs the extension below.

## Strict values and the value extensions

`ValueExtensions` enables two value shapes beyond DTCG 2025.10. Both are **off by default**, so validation follows the specification strictly unless you opt in.

| Flag | Setter | Effect |
|---|---|---|
| `line_height_dimension` | `with_line_height_dimension(bool)` | Typography `lineHeight` may be a `dimension` or alias a `dimension` token |
| `blur` | `with_blur(bool)` | The `dxn.dtcg.blur` type is accepted |

Design tools commonly export line heights as pixel dimensions that text styles alias; `line_height_dimension` accepts them. The blur type models layer and background blur effects, such as Figma's blur effect styles.

`ValueExtensions::all()` turns every value extension on. Extensions reach a runtime through `RuntimeOptions`, which also carries the [limits](./limits.md#runtime-limits): `DtcgRuntime::with_options` and `from_json_with_options`. `DtcgRuntime::options()` returns them.

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
    "$type": "dxn.dtcg.blur",
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

    // With every value extension on, both shapes are accepted.
    let extended = DtcgRuntime::from_json_with_options(
        "tokens.json",
        TOKENS,
        RuntimeOptions::default().with_extensions(ValueExtensions::all()),
    )?;
    let tokens = extended.resolve(&[])?.ready().unwrap();
    assert_eq!(tokens.get("body").unwrap().value["lineHeight"]["value"], 24);
    assert_eq!(tokens.get("frost").unwrap().token_type.as_deref(), Some(BLUR_TYPE));
    Ok(())
}
```

Each flag is independent: `ValueExtensions::default().with_line_height_dimension(true)` accepts dimension line heights but not the blur type. Other `dxn.dtcg.*` names are never recognized.

## Unsupported types

A token must end up with one of the 13 types (or `dxn.dtcg.blur` with the extension). Everything else fails with ``unsupported DTCG type `x` for final 2025.10``: the pre-2025 `string` type (use `fontFamily` for font names), custom reverse-DNS types, and misspellings (`colour`, `fontweight`; names are case-sensitive). To carry custom data, put it in `$extensions` and give the token a supported type.

## Validating values directly

`validate_value` (strict) and `validate_value_with` (given `ValueExtensions`) check one fully resolved value, for editors and import tools. A value that still contains aliases does not pass.

```rust
use dxn_dtcg::{
    BLUR_TYPE, ValueExtensions, ValueValidationError, validate_value, validate_value_with,
};
use serde_json::json;

fn main() {
    assert!(validate_value("dimension", &json!({ "value": 4, "unit": "px" })).is_ok());

    let blur = json!({ "kind": "layer", "radius": { "value": 8, "unit": "px" } });
    assert_eq!(
        validate_value(BLUR_TYPE, &blur),
        Err(ValueValidationError::UnsupportedType(BLUR_TYPE.into()))
    );
    assert!(validate_value_with(BLUR_TYPE, &blur, &ValueExtensions::all()).is_ok());

    assert_eq!(
        validate_value("dimension", &json!({ "value": 4, "unit": "em" })),
        Err(ValueValidationError::InvalidValue("dimension".into()))
    );
}
```

## Reading values in Rust

`ResolvedToken::value` is a `serde_json::Value` whose shape is guaranteed by validation. `dxn_dtcg::values` models every type as plain Rust data, and `ResolvedToken::typed(&extensions)` converts a token by its effective type into a `TypedValue` enum with one variant per type. Pass `runtime.options().extensions` so the same shapes are accepted. `TypedValue::from_resolved(type, &value, &extensions)` does the same for any value and validates first.

| Type | Model | Notes |
|---|---|---|
| `color` | `Color { color_space, components, alpha, hex }` | 14 `ColorSpace` variants; each `Component` is `Number(f64)` or `None`; a missing `alpha` is `1.0`; conversions [below](#color-conversions) |
| `dimension` | `Dimension { value, unit }` | `DimensionUnit::{Px, Rem}`; `to_px(rem_base)` |
| `duration` | `Duration { value, unit }` | `DurationUnit::{Ms, S}`; `to_millis()` |
| `number` | `TypedValue::Number(f64)` | |
| `fontFamily` | `FontFamily(Vec<String>)` | a single name becomes a one-element list |
| `fontWeight` | `FontWeight(f64)` | keywords map to numbers (`thin` 100 ... `extra-black` 950); `FontWeight::keyword(name)` |
| `cubicBezier` | `CubicBezier([f64; 4])` | |
| `strokeStyle` | `StrokeStyle::Style(LineStyle)` or `StrokeStyle::Dashes { dash_array, line_cap }` | |
| `border` | `Border { color, width, style }` | |
| `transition` | `Transition { duration, delay, timing_function }` | |
| `shadow` | `Shadow(Vec<ShadowLayer>)` | a single object is one layer; `inset` defaults to `false` |
| `gradient` | `Gradient(Vec<GradientStop>)` | positions clamped to [0, 1] |
| `typography` | `Typography { font_family, font_size, font_weight, letter_spacing, line_height }` | `LineHeight::Multiplier(f64)`, or `LineHeight::Dimension` with the extension |
| `dxn.dtcg.blur` | `Blur { kind, radius }` | `BlurKind::{Layer, Background}` |

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

`typed` reports a value that does not convert as `DtcgError::Projection` with the token path. Every model also implements `FromDtcgValue`, so it can be a field of a `#[derive(DtcgTokens)]` struct, and so can `TypedValue`.

### Converting the JSON yourself

When you need a representation of your own, such as 8-bit RGBA or `std::time::Duration`, read the JSON directly, or wrap the conversion in a function and attach it with `#[dtcg(with = "...")]` ([Typed projection](./typed-projection.md#custom-conversions)).

```rust
use std::time::Duration;

use dxn_dtcg::{DtcgError, DtcgRuntime, ResolvedToken};
use serde_json::Value;

fn projection_error(token: &ResolvedToken, message: &str) -> DtcgError {
    DtcgError::Projection { path: token.path.to_string(), message: message.into() }
}

/// sRGB colors as 8-bit RGBA. For other color spaces, convert first with `values::Color::convert`.
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
          "fade": { "$type": "duration", "$value": { "value": 150, "unit": "ms" } }
        }"##,
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();

    assert_eq!(rgba8(tokens.get("accent").unwrap())?, [255, 128, 0, 128]);
    assert_eq!(duration(tokens.get("fade").unwrap())?, Duration::from_millis(150));
    Ok(())
}
```

## Color conversions

`values::Color` converts between the 14 DTCG color spaces with the formulas of CSS Color 4 and offers the operations design tokens commonly need. It needs no feature; with the `expressions` feature, the built-in module [`dxn.dtcg/color`](./expressions.md#built-in-modules) gives expressions the same operations on color JSON.

| Method | Result |
|---|---|
| `Color::new(space, components, alpha)` | A valid color from numeric components: clamped to the space's ranges, a non-finite component counting as 0 and a non-finite alpha as 1 |
| `coordinates()` | The components as numbers, `none` counting as 0 |
| `convert(space)` | The color in `space`, its components clamped to the space's ranges (an out-of-gamut color is clipped into an RGB space); the alpha is kept, the `hex` fallback dropped |
| `mix(other, amount)`, `mix_in(other, amount, space)` | `other` mixed in by `amount` (0 is this color, 1 is `other`), in OKLCH (the CSS `color-mix()` default) or in `space`, as CSS Color 4 interpolates: premultiplied alpha, the shorter hue arc, and the powerless hue of white, black or a gray taking the other color's; the result is in the mixing space |
| `lighten(amount)`, `darken(amount)` | The OKLCH lightness shifted by `amount` (stopping at black and white, where no chroma remains), in the color's own space |
| `with_alpha(alpha)` | The color with `alpha`, clamped to `[0, 1]`; a non-finite `alpha` keeps the current one |
| `to_hex()`, `Color::from_hex(hex)` | The 6-digit sRGB hex form (`#rrggbb`, clipped into sRGB, alpha not represented), and the sRGB color of one (`None` unless it is `#` and six hex digits, any case) |
| `luminance()`, `contrast(other)` | The WCAG 2.x relative luminance (`Y` in XYZ), and the contrast ratio of the colors clipped into sRGB, from 1 to 21 |
| `in_gamut(space)`, `clamp_to_gamut(space)` | Whether the color lies inside the gamut of `space` (the unit cube of an RGB or XYZ space in linear light, sRGB's for HSL and HWB; the Lab spaces contain every color), and the color clipped into it |
| `to_json()` | The DTCG JSON form; `alpha` only when it is not 1 |

Conversions go through CIE XYZ relative to D65: the RGB spaces through their transfer functions and primaries, ProPhoto RGB, `xyz-d50` and CIE Lab/LCH through D50 with Bradford chromatic adaptation, OKLab and OKLCH through their LMS matrices, and HSL and HWB through sRGB. `none` components count as 0, and every returned color is a valid DTCG color: components are clamped to their ranges and hues normalized to `[0, 360)`, the hue of a color without chroma (up to rounding noise) written as 0; HSL and HWB results are clipped into sRGB first. A non-finite `amount` counts as 0. Results match the CSS Color 4 reference implementation within 1e-5, and in-gamut round trips through every space return the input within 1e-9; to keep them exact, encoded A98 RGB components below about 4e-7 and Rec. 2020 components below about 1.5e-6 read as 0.

```rust
use dxn_dtcg::values::{Color, ColorSpace};

fn main() {
    let accent = Color::new(ColorSpace::Srgb, [0.2, 0.4, 0.9], 1.0);

    let oklch = accent.convert(ColorSpace::Oklch).coordinates();
    assert!((oklch[0] - 0.5518).abs() < 1e-4);

    let white = Color::new(ColorSpace::Srgb, [1.0, 1.0, 1.0], 1.0);
    assert!((accent.contrast(&white) - 5.026).abs() < 1e-3);
    assert_eq!(accent.to_hex(), "#3366e6");
    assert_eq!(Color::from_hex("#3366e6").unwrap().color_space, ColorSpace::Srgb);

    // Lighter in OKLCH, still an sRGB color.
    let hover = accent.lighten(0.08);
    assert_eq!(hover.color_space, ColorSpace::Srgb);

    assert!(Color::new(ColorSpace::DisplayP3, [0.0, 1.0, 0.0], 1.0).in_gamut(ColorSpace::Rec2020));
    println!("{}", hover.to_json());
}
```

`ColorSpace::from_name(name)` parses a DTCG `colorSpace` name, the inverse of `name()`. Every value model has `to_json()` too, and the twelve models of the DTCG types (`Color`, `Dimension`, `Duration`, `CubicBezier`, `FontFamily`, `FontWeight`, `StrokeStyle`, `Border`, `Transition`, `Shadow`, `Gradient` and `Typography`) implement serde's `Serialize` and `Deserialize` with their DTCG JSON forms. Deserializing validates the value as its DTCG type (with every value extension), which is how a [native function](./expressions.md#rust-functions-native-modules) of an expression module takes and returns them.

Next: [Resolvers and modifiers](./resolvers.md), or [Typed projection](./typed-projection.md).
