# 8. Typed projection

Projection turns resolved tokens into Rust structs. You declare the fields you need and where they
live in the token tree, and `#[derive(DtcgTokens)]` generates the code that reads and converts
them.

- [The model](#the-model)
- [Projecting](#projecting)
- [Attribute reference](#attribute-reference)
- [How field paths are built](#how-field-paths-are-built)
- [Built-in conversions](#built-in-conversions)
- [Custom conversions](#custom-conversions)
- [Nested structs and shared shapes](#nested-structs-and-shared-shapes)
- [Component tokens](#component-tokens)
- [Errors](#errors)
- [Limitations and manual implementations](#limitations-and-manual-implementations)

## The model

```rust,ignore
pub trait DtcgTokens: Sized {
    /// Projects the value rooted at `root`, a dotted token path ("" for the top level).
    fn project_at(tokens: &ResolvedTokens, registry: &TypeRegistry, root: &str)
        -> Result<Self, DtcgError>;

    /// `project_at(tokens, registry, "")`.
    fn project(tokens: &ResolvedTokens, registry: &TypeRegistry) -> Result<Self, DtcgError>;
}

pub trait FromDtcgValue: Sized + 'static {
    fn from_dtcg(token: &ResolvedToken) -> Result<Self, DtcgError>;
}
```

- A `DtcgTokens` type is a **view** of part of the token set. The derive generates `project_at`.
- A `FromDtcgValue` type converts **one token**. Field types use it unless told otherwise.
- `TypeRegistry` is the conversion context passed through projection. It is a unit struct that
  forwards to `FromDtcgValue` (`registry.convert::<T>(token)`); get one from `runtime.registry()`,
  or write `&TypeRegistry` directly.

Projection is **narrow**. A struct asks for exactly its fields; tokens it does not mention are
ignored, but they still took part in resolution and may be alias targets of the tokens it reads. A
field whose token is missing is an error.

## Projecting

Resolve and project in one call:

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime, DtcgTokens};

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

`DtcgRuntime::project::<T>(selection)` returns `Result<Resolution<T>, DtcgError>`, just like
`resolve`, and passes `Missing` through. To project several structs from one selection, resolve
once and call `T::project(&tokens, runtime.registry())` for each struct, which avoids repeating the
resolution.

## Attribute reference

All attributes are written `#[dtcg(...)]`. Several may be combined in one attribute or spread over
several.

### On the struct

| Attribute | Default | Meaning |
|---|---|---|
| `root = "a.b"` | `""` | A path prefix for every field of the struct |
| `id = "button"` | the struct name | The component id returned by `DtcgComponentTokens::component_id()`. Only used by `#[derive(DtcgComponentTokens)]`. |

### On a field

| Attribute | Meaning |
|---|---|
| *(none)* | Read the token named exactly like the field (`font_size` reads `font_size`, with no case conversion) |
| `rename = "fontSize"` | Read the token with this name instead |
| `path = "type.body.size"` | Read the token at this dotted path (relative to the struct's base path). Takes precedence over `rename`. |
| `with = "path::to::function"` | Convert with `fn(&ResolvedToken) -> Result<FieldType, DtcgError>` instead of `FromDtcgValue` |
| `nested` | The field's type implements `DtcgTokens`; project it at the field's path |
| `rest` | Initialize the field with `Default::default()`. Reads no tokens. At most one per struct. |

The derives accept only structs with named fields.

## How field paths are built

```text
base path  = join(root passed to project_at, #[dtcg(root)])
field path = join(base path, path | rename | field name)
```

`join` puts a `.` between two non-empty parts and drops empty ones (`dtcg_runtime::join_token_path`
is the public helper). For example, `#[dtcg(root = "card")]` with a field
`#[dtcg(path = "corner.radius")]`, projected at the top level, reads `card.corner.radius`;
projected with `project_at(…, "theme")`, it reads `theme.card.corner.radius`.

## Built-in conversions

| Rust type | Accepts | Typical DTCG types |
|---|---|---|
| `String` | a JSON string | `fontFamily` (single family), `fontWeight` keywords, `strokeStyle` keywords |
| `f64` | a JSON number | `number`, numeric `fontWeight` |
| `f32` | a JSON number, converted from `f64` | as `f64` |
| `bool` | a JSON boolean | none at the top level; useful in custom conversions |

The built-in conversions look at the JSON shape, not at the DTCG type. A `String` field accepts any
string-valued token. Composite types (colors, dimensions, shadows and so on) need a custom
conversion.

## Custom conversions

### Implement `FromDtcgValue` for your own types

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime, DtcgTokens, FromDtcgValue, ResolvedToken};

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

A conversion can check `token.token_type` to accept only the DTCG types it understands, as above.
Since values are validated before projection, the shape for a given type is guaranteed.

### Use `with` for foreign types

Rust's orphan rule prevents implementing `FromDtcgValue` (a foreign trait) for a type from another
crate, such as a renderer's color type. Write a function instead and name it in `with`. The string
is parsed as a Rust path and resolved where the struct is defined, so `"srgb"`,
`"crate::theme::convert::srgb"` and `"Color::from_token"` all work.

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime, DtcgTokens, ResolvedToken};

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

`with` is also the way to interpret the same Rust type differently for one field, for example an
`f32` read from a dimension's `value`.

## Nested structs and shared shapes

`#[dtcg(nested)]` projects a field whose type is itself `DtcgTokens`, rooted at the field's path.
Because `project_at` takes the root as an argument, the **same struct can be projected at several
roots**: one `ButtonVariant` type for every variant.

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime, DtcgTokens, TypeRegistry};

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

When a nested field has no `path` or `rename`, the field name is the path segment, as with
`primary` above. `with` is ignored on `nested` fields.

For generic structs, the derive adds a `DtcgTokens` bound to every type parameter, so generic
parameters are meant to be nested token structs:

```rust
use dtcg_runtime::DtcgTokens;

#[derive(DtcgTokens)]
struct Themed<T> {
    #[dtcg(nested)]
    light: T,
    #[dtcg(nested)]
    dark: T,
}
```

## Component tokens

`#[derive(DtcgComponentTokens)]` generates everything `#[derive(DtcgTokens)]` does, plus an
implementation of `DtcgComponentTokens`, which gives the struct a stable component id:

```rust
use dtcg_runtime::{DtcgComponentTokens, DtcgTokens};

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

assert_eq!(ButtonTokens::component_id(), "button");
assert_eq!(Tooltip::component_id(), "Tooltip"); // defaults to the struct name

// Component token structs are ordinary projections too.
fn project_any<T: DtcgComponentTokens>(tokens: &dtcg_runtime::ResolvedTokens) -> (&'static str, Option<T>) {
    (T::component_id(), T::project(tokens, &dtcg_runtime::TypeRegistry).ok())
}
let _ = project_any::<Tooltip>;
```

Use it when a registry of components needs to tell their token structs apart, for example to
report which component's tokens failed to project.

## Errors

The first failing field ends the projection:

| Cause | Error |
|---|---|
| The token at a field's path does not exist | ``projection `<full path>`: required token is missing`` |
| A built-in conversion gets the wrong JSON shape | ``projection `<path>`: expected string value`` / `expected numeric value` / `expected boolean value` |
| A custom conversion fails | whatever error the conversion returns |

Mistakes in the attributes are compile errors: a derive on an enum (`DtcgTokens only supports
structs`), on a tuple struct (`DtcgTokens requires named fields`), a second `rest` field, or an
unknown field attribute (`unsupported dtcg field attribute`).

## Limitations and manual implementations

The derive covers fixed shapes. It does not support:

- **Optional tokens.** Every field's token is required. There is no `Option<T>` support.
- **Collections.** It cannot gather "every token in this group" into a `Vec` or a map. `rest` is a
  placeholder that only calls `Default::default()`; it does not collect anything.
- **Name conversion.** Field names are used verbatim; use `rename` for camelCase tokens.

For these cases, implement `DtcgTokens` by hand. `ResolvedTokens::get` and `ResolvedTokens::iter`
give full access to the token set:

```rust
use std::collections::BTreeMap;

use dtcg_runtime::{
    DtcgError, DtcgRuntime, DtcgTokens, ResolvedTokens, TypeRegistry, join_token_path,
};

#[derive(Debug)]
struct Palette {
    /// Every token directly below `palette`, by name.
    colors: BTreeMap<String, serde_json::Value>,
    /// An optional token.
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

A hand-written `DtcgTokens` type can be used as a `nested` field of a derived struct.
