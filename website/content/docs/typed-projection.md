---
title: Typed projection
description: Map resolved tokens onto Rust structs with #[derive(DtcgTokens)], including attributes, optional tokens, custom conversions, nested and generic structs, and component tokens.
group: Guides
order: 100
---

Projection turns resolved tokens into Rust structs. You declare the fields you need and where they live in the token tree, and `#[derive(DtcgTokens)]` generates the code that reads and converts them. Reach for it when an application wants `button.padding` as a `Dimension` field, not as JSON it has to dig through.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime, DtcgTokens};

#[derive(Debug, DtcgTokens)]
struct Brand {
    #[dtcg(path = "brand.name")]
    name: String,
    #[dtcg(path = "brand.weight")]
    weight: f64,
}

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "brand.json",
        r#"{ "brand": {
          "name": { "$type": "fontFamily", "$value": "Acme Sans" },
          "weight": { "$type": "fontWeight", "$value": 700 }
        } }"#,
    )?;
    let brand: Brand = runtime.project(&[])?.ready().unwrap();
    assert_eq!((brand.name.as_str(), brand.weight), ("Acme Sans", 700.0));
    Ok(())
}
```

`DtcgRuntime::project::<T>(selection)` returns `Result<Resolution<T>, DtcgError>` like `resolve`, passing `Missing` through. To project several structs from one selection, resolve once and call `T::project(&tokens, runtime.registry())` for each.

## The model

Two traits do the work:

```text
// Simplified from the crate.
pub trait DtcgTokens: Sized {
    fn project_at(tokens: &ResolvedTokens, registry: &TypeRegistry, root: &str)
        -> Result<Self, DtcgError>;
    fn project(tokens: &ResolvedTokens, registry: &TypeRegistry) -> Result<Self, DtcgError>;
}

pub trait FromDtcgValue: Sized + 'static {
    fn from_dtcg(token: &ResolvedToken) -> Result<Self, DtcgError>;
}
```

A `DtcgTokens` type is a view of part of the token set; the derive generates `project_at`. A `FromDtcgValue` type converts one token, and field types use it unless told otherwise. `TypeRegistry` is a unit struct that forwards to it; get one from `runtime.registry()`.

Projection is **narrow**: a struct asks for exactly its fields. Other tokens are ignored but still resolve, so a projected token can alias an unprojected one. A missing requested token is an error unless the field is an `Option`.

## Attribute reference

Attributes are written `#[dtcg(...)]`, combined in one attribute or spread over several, and parsed strictly.

On the struct:

| Attribute | Default | Meaning |
|---|---|---|
| `root = "a.b"` | `""` | Path prefix for every field of the struct |
| `id = "button"` | the struct name | The id returned by `DtcgComponentTokens::component_id()`; only for `#[derive(DtcgComponentTokens)]` |

On a field:

| Attribute | Meaning |
|---|---|
| *(none)* | Read the token named exactly like the field. No case conversion; `r#type` reads `type` |
| `rename = "fontSize"` | Read the token with this name instead |
| `path = "type.body.size"` | Read the token at this dotted path, relative to the struct's base path |
| `with = "path::to::function"` | Convert with `fn(&ResolvedToken) -> Result<T, DtcgError>` instead of `FromDtcgValue` |
| `nested` | The field's type implements `DtcgTokens`; project it at the field's path |
| `rest` | Initialize with `Default::default()`; reads no tokens; at most one per struct |

`path` and `rename` cannot be combined, nor `nested` and `with`; `rest` takes no other field attribute; `nested` and `rest` take no value. The derives accept only structs with named fields. The generated code names items by full path (`::core::result::Result`, `::dxn_dtcg::DtcgError`), so a local `type Result<T> = ...` does not affect it.

### How paths are built

```text
base path  = join(root passed to project_at, #[dtcg(root)])
field path = join(base path, path | rename | field name)
```

`join` puts a `.` between non-empty parts (`dxn_dtcg::join_token_path`). `#[dtcg(root = "card")]` with `#[dtcg(path = "corner.radius")]`, projected at the top level, reads `card.corner.radius`; projected with `project_at(..., "theme")` it reads `theme.card.corner.radius`.

## Built-in conversions

| Rust type | Accepts |
|---|---|
| `String` | a JSON string (single `fontFamily`, `fontWeight` and `strokeStyle` keywords) |
| `f64` | a JSON number |
| `f32` | a JSON number within the finite `f32` range |
| `bool` | a JSON boolean |
| `values::Color`, `Dimension`, `Duration`, `FontFamily`, `FontWeight`, `CubicBezier`, `StrokeStyle`, `Border`, `Transition`, `Shadow`, `Gradient`, `Typography`, `LineHeight`, `Blur` | a value of that type's shape |
| `values::TypedValue` | any value of a supported type, converted by the token's type |

Conversions look at the JSON shape, not the token's DTCG type: a `String` field accepts any string-valued token. A wrong shape fails with a `Projection` error naming the path. The typed models apply the consumer rules of the spec: a color without `alpha` has alpha `1`, font-weight keywords become numbers, a single shadow object is one layer, gradient positions clamp to [0, 1].

```rust
use dxn_dtcg::{
    DtcgError, DtcgRuntime, DtcgTokens,
    values::{Color, ColorSpace, Dimension, DimensionUnit, Duration, FontWeight, Shadow},
};

#[derive(Debug, DtcgTokens)]
#[dtcg(root = "card")]
struct Card {
    background: Color,
    radius: Dimension,
    shadow: Shadow,
    weight: FontWeight,
    #[dtcg(rename = "fadeIn")]
    fade_in: Duration,
    r#type: String, // reads `card.type`
}

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "card.json",
        r#"{ "card": {
          "background": { "$type": "color", "$value": { "colorSpace": "srgb", "components": [1, 1, 1] } },
          "radius": { "$type": "dimension", "$value": { "value": 0.5, "unit": "rem" } },
          "shadow": { "$type": "shadow", "$value": {
            "color": { "colorSpace": "srgb", "components": [0, 0, 0], "alpha": 0.2 },
            "offsetX": { "value": 0, "unit": "px" }, "offsetY": { "value": 2, "unit": "px" },
            "blur": { "value": 8, "unit": "px" }, "spread": { "value": 0, "unit": "px" }
          } },
          "weight": { "$type": "fontWeight", "$value": "bold" },
          "fadeIn": { "$type": "duration", "$value": { "value": 0.2, "unit": "s" } },
          "type": { "$type": "fontFamily", "$value": "Acme Sans" }
        } }"#,
    )?;
    let card: Card = runtime.project(&[])?.ready().unwrap();

    assert_eq!(card.background.color_space, ColorSpace::Srgb);
    assert_eq!(card.background.alpha, 1.0); // no `alpha`: opaque
    assert_eq!(card.radius.unit, DimensionUnit::Rem);
    assert_eq!(card.radius.to_px(16.0), 8.0);
    assert_eq!(card.shadow.0.len(), 1); // one object, one layer
    assert_eq!(card.weight, FontWeight(700.0));
    assert_eq!(card.fade_in.to_millis(), 200.0);
    assert_eq!(card.r#type, "Acme Sans");
    Ok(())
}
```

## Optional tokens

A leaf field typed `Option<T>` (also `core::option::Option<T>` or `std::option::Option<T>`) is `None` when its token does not exist and `Some(value)` when it does. A token that exists with the wrong shape is still an error. `nested` fields cannot be optional.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime, DtcgTokens, values::Dimension};

#[derive(Debug, DtcgTokens)]
#[dtcg(root = "button")]
struct Button {
    padding: Dimension,
    #[dtcg(rename = "minWidth")]
    min_width: Option<Dimension>,
    #[dtcg(rename = "borderWidth")]
    border_width: Option<Dimension>,
}

#[derive(Debug, DtcgTokens)]
#[dtcg(root = "button")]
struct WrongShape {
    padding: Option<f64>,
}

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "button.json",
        r#"{ "button": { "$type": "dimension",
          "padding": { "$value": { "value": 8, "unit": "px" } },
          "minWidth": { "$value": { "value": 64, "unit": "px" } }
        } }"#,
    )?;
    let button: Button = runtime.project(&[])?.ready().unwrap();
    assert_eq!(button.min_width.map(|width| width.value), Some(64.0));
    assert!(button.border_width.is_none()); // missing token

    // Present, but not a number.
    assert!(matches!(runtime.project::<WrongShape>(&[]), Err(DtcgError::Projection { .. })));
    Ok(())
}
```

## Custom conversions

### Implement FromDtcgValue for your own types

When you own the type, implement the trait and use it as a field type. This one reads a duration token as whole milliseconds:

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime, DtcgTokens, FromDtcgValue, ResolvedToken};

#[derive(Debug, PartialEq)]
struct Ms(u64);

impl FromDtcgValue for Ms {
    fn from_dtcg(token: &ResolvedToken) -> Result<Self, DtcgError> {
        if token.token_type.as_deref() != Some("duration") {
            return Err(DtcgError::Projection {
                path: token.path.to_string(),
                message: "expected a duration".into(),
            });
        }
        let value = token.value["value"].as_f64().unwrap_or_default();
        let factor = if token.value["unit"] == "s" { 1000.0 } else { 1.0 };
        Ok(Ms((value * factor).round().max(0.0) as u64))
    }
}

#[derive(Debug, DtcgTokens)]
#[dtcg(root = "motion")]
struct Motion {
    fast: Ms,
    slow: Ms,
}

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "motion.json",
        r#"{ "motion": { "$type": "duration",
          "fast": { "$value": { "value": 120, "unit": "ms" } },
          "slow": { "$value": { "value": 0.4, "unit": "s" } }
        } }"#,
    )?;
    let motion: Motion = runtime.project(&[])?.ready().unwrap();
    assert_eq!((motion.fast, motion.slow), (Ms(120), Ms(400)));
    Ok(())
}
```

A conversion can check `token.token_type` to accept only the types it understands. Because values are validated before projection, the shape for a given type is guaranteed.

### Use with for foreign types

Rust's orphan rule prevents implementing `FromDtcgValue` for a type from another crate, such as a renderer's color. Write a function and name it in `with`. The string is parsed as a Rust path and resolved where the struct is defined, so `"srgb"`, `"crate::theme::convert::srgb"` and `"Color::from_token"` all work. Inside the function, `values::Color::from_dtcg(token)` can do the parsing; the example below reads the JSON directly to stay self-contained.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime, DtcgTokens, ResolvedToken};

// Stand-in for a type owned by another crate.
mod renderer {
    #[derive(Debug, PartialEq)]
    pub struct Rgba(pub f32, pub f32, pub f32, pub f32);
}

fn rgba(token: &ResolvedToken) -> Result<renderer::Rgba, DtcgError> {
    let value = &token.value;
    let component = |index: usize| value["components"][index].as_f64().unwrap_or(0.0) as f32;
    match value["colorSpace"].as_str() {
        Some("srgb") => Ok(renderer::Rgba(
            component(0),
            component(1),
            component(2),
            value["alpha"].as_f64().unwrap_or(1.0) as f32,
        )),
        _ => Err(DtcgError::Projection {
            path: token.path.to_string(),
            message: "only sRGB colors are supported".into(),
        }),
    }
}

#[derive(Debug, DtcgTokens)]
struct Surface {
    #[dtcg(path = "surface.fill", with = "rgba")]
    fill: renderer::Rgba,
}

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "surface.json",
        r#"{ "surface": { "fill": { "$type": "color",
          "$value": { "colorSpace": "srgb", "components": [1, 1, 1], "alpha": 0.5 } } } }"#,
    )?;
    let surface: Surface = runtime.project(&[])?.ready().unwrap();
    assert_eq!(surface.fill, renderer::Rgba(1.0, 1.0, 1.0, 0.5));
    Ok(())
}
```

`with` is also how you interpret the same Rust type differently for one field, for example an `f32` read from a dimension's `value`. On an `Option<T>` field the function returns `T`.

## Nested and shared structs

`#[dtcg(nested)]` projects a field whose type is itself `DtcgTokens`, rooted at the field's path. Because `project_at` takes the root as an argument, one struct can be projected at several roots, such as one `ButtonVariant` type for every variant.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime, DtcgTokens, TypeRegistry};

#[derive(Debug, PartialEq, DtcgTokens)]
struct ButtonVariant {
    #[dtcg(rename = "fontWeight")]
    font_weight: f64,
    #[dtcg(rename = "borderStyle")]
    border_style: String,
}

#[derive(Debug, DtcgTokens)]
#[dtcg(root = "button")]
struct Buttons {
    #[dtcg(nested)]
    primary: ButtonVariant,
    #[dtcg(nested, path = "variants.ghost")]
    ghost: ButtonVariant,
}

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "buttons.json",
        r#"{ "button": {
          "primary": {
            "fontWeight": { "$type": "fontWeight", "$value": 600 },
            "borderStyle": { "$type": "strokeStyle", "$value": "solid" }
          },
          "variants": { "ghost": {
            "fontWeight": { "$type": "fontWeight", "$value": 500 },
            "borderStyle": { "$type": "strokeStyle", "$value": "dashed" }
          } }
        } }"#,
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();

    let buttons = Buttons::project(&tokens, runtime.registry())?;
    assert_eq!(buttons.primary.font_weight, 600.0);
    assert_eq!(buttons.ghost.border_style, "dashed");

    // The shared shape can also be projected at any root directly.
    let ghost = ButtonVariant::project_at(&tokens, &TypeRegistry, "button.variants.ghost")?;
    assert_eq!(ghost, buttons.ghost);
    Ok(())
}
```

With no `path` or `rename`, a nested field's name is the path segment.

### Generic structs

For a generic struct the derive adds bounds per field, only for fields whose type mentions a type parameter: `FromDtcgValue` for leaf fields (on `T` for an `Option<T>` field), `DtcgTokens` for `nested` fields and `Default` for the `rest` field. Fields converted `with` a function get no bound.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime, DtcgTokens, values::Dimension};

#[derive(Debug, DtcgTokens)]
struct Scale<V> {
    sm: V,         // V: FromDtcgValue
    lg: Option<V>, // V: FromDtcgValue
}

#[derive(Debug, DtcgTokens)]
struct Themed<T, C> {
    #[dtcg(nested)]
    light: T, // T: DtcgTokens
    #[dtcg(nested)]
    dark: T,
    #[dtcg(rest)]
    cache: C, // C: Default
}

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "scale.json",
        r#"{
          "light": { "$type": "dimension", "sm": { "$value": { "value": 4, "unit": "px" } } },
          "dark": { "$type": "dimension", "sm": { "$value": { "value": 6, "unit": "px" } } }
        }"#,
    )?;
    let themed: Themed<Scale<Dimension>, Vec<String>> = runtime.project(&[])?.ready().unwrap();
    assert_eq!(themed.dark.sm.value, 6.0);
    assert!(themed.light.lg.is_none() && themed.cache.is_empty());
    Ok(())
}
```

## Component tokens

`#[derive(DtcgComponentTokens)]` generates everything `DtcgTokens` does plus an implementation of `DtcgComponentTokens`, which gives the struct a stable `component_id()`. Use it when a registry of components needs to tell their token structs apart, for example to report which component's tokens failed to project.

```rust
use dxn_dtcg::{DtcgComponentTokens, DtcgTokens};

#[derive(DtcgComponentTokens)]
#[dtcg(id = "button", root = "component.button")]
struct ButtonTokens {
    #[dtcg(rename = "fontWeight")]
    font_weight: f64,
}

#[derive(DtcgComponentTokens)]
struct Tooltip {
    delay: f64,
}

fn main() {
    assert_eq!(ButtonTokens::component_id(), "button");
    assert_eq!(Tooltip::component_id(), "Tooltip"); // defaults to the struct name

    // Component token structs are ordinary projections too.
    fn project_any<T: DtcgComponentTokens>(tokens: &dxn_dtcg::ResolvedTokens) -> (&'static str, Option<T>) {
        (T::component_id(), T::project(tokens, &dxn_dtcg::TypeRegistry).ok())
    }
    let _ = project_any::<Tooltip>;
}
```

## Errors

The first failing field ends the projection.

| Cause | Error |
|---|---|
| The token at a non-`Option` field's path does not exist | ``projection `<full path>`: required token is missing`` |
| A scalar conversion gets the wrong shape | ``projection `<path>`: expected string value`` / `expected numeric value` / `expected boolean value` |
| An `f32` field gets a number outside the finite range | `expected numeric value within f32 range` |
| A typed model gets the wrong shape | `expected a <type> value`, for example `expected a color value` |
| A custom conversion fails | whatever error it returns |

Attribute mistakes are compile errors reported at the attribute: an enum or tuple struct (`DtcgTokens only supports structs`, `DtcgTokens requires named fields`), an unknown key (``unknown `dtcg` field attribute `pth`; expected one of path, rename, with, nested, rest``), a duplicated key, a struct key on a field or the reverse, `id` on `DtcgTokens`, `path` with `rename`, `nested` with `with`, `rest` with another key, a value on a flag, a `with` string that is not a path, and a second `rest` field.

## Limitations

The derive covers fixed shapes. It does not support collecting "every token in this group" into a `Vec` or map (`rest` only calls `Default::default()`), name conversion (use `rename` for camelCase tokens), or optional nested structs. For those, implement `DtcgTokens` by hand using `ResolvedTokens::get` and `iter`, and use `join_token_path` to honor the caller's root. A hand-written type can be a `nested` field of a derived struct.

```rust
use std::collections::BTreeMap;

use dxn_dtcg::{
    DtcgError, DtcgRuntime, DtcgTokens, ResolvedTokens, TypeRegistry, join_token_path,
};

#[derive(Debug)]
struct Palette {
    /// Every token directly below `palette`, by name.
    colors: BTreeMap<String, serde_json::Value>,
    /// An optional token, read by hand here for completeness.
    accent: Option<serde_json::Value>,
}

impl DtcgTokens for Palette {
    fn project_at(
        tokens: &ResolvedTokens,
        _registry: &TypeRegistry,
        root: &str,
    ) -> Result<Self, DtcgError> {
        let base = join_token_path(root, "palette");
        let prefix = format!("{base}.");
        let colors = tokens
            .iter()
            .filter_map(|token| {
                let name = token.path.strip_prefix(&prefix)?;
                (!name.contains('.')).then(|| (name.to_owned(), token.value.clone()))
            })
            .collect();
        let accent = tokens.get(&join_token_path(&base, "accent")).map(|token| token.value.clone());
        Ok(Self { colors, accent })
    }
}

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "palette.json",
        r#"{ "palette": { "$type": "color",
          "red": { "$value": { "colorSpace": "srgb", "components": [1, 0, 0] } },
          "blue": { "$value": { "colorSpace": "srgb", "components": [0, 0, 1] } }
        } }"#,
    )?;
    let palette: Palette = runtime.project(&[])?.ready().unwrap();
    assert_eq!(palette.colors.keys().collect::<Vec<_>>(), ["blue", "red"]);
    assert!(palette.accent.is_none());
    Ok(())
}
```

Next: [Cookbook](./cookbook.md), or [Errors](./errors.md).
