---
title: Cookbook
description: Worked recipes - light and dark themes, wiring DtcgTokens into an app, caching, hot reload, CSS variables, deprecation reports, CI checks and editor features.
group: Guides
order: 110
---

Recipes for common tasks. Each is a complete program built from the API described in the concept pages; the loading recipes for disk, HTTP and embedded assets are in [Loading documents](./loading.md#loader-patterns).

## Light and dark themes from one resolver

Model the theme as a modifier. The base set holds the palette and semantic tokens; the dark context overrides only the palette entries that differ. Map your UI's theme enum onto a selection.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Theme {
    Light,
    Dark,
}

impl Theme {
    fn selection(self) -> [(&'static str, &'static str); 1] {
        [("theme", if self == Theme::Dark { "dark" } else { "light" })]
    }
}

const RESOLVER: &str = r##"{
  "version": "2025.10",
  "sets": { "base": { "sources": [{ "$ref": "base.json" }] } },
  "modifiers": { "theme": { "default": "light", "contexts": {
    "light": [],
    "dark": [{ "$ref": "dark.json" }]
  } } },
  "resolutionOrder": [{ "$ref": "#/sets/base" }, { "$ref": "#/modifiers/theme" }]
}"##;

const BASE: &str = r#"{
  "palette": {
    "$type": "color",
    "paper": { "$value": { "colorSpace": "srgb", "components": [1, 1, 1] } },
    "ink": { "$value": { "colorSpace": "srgb", "components": [0.1, 0.1, 0.12] } }
  },
  "surface": {
    "$type": "color",
    "background": { "$value": "{palette.paper}" },
    "text": { "$value": "{palette.ink}" }
  }
}"#;

// Dark swaps the palette values; `surface.*` follows because aliases resolve last.
const DARK: &str = r#"{
  "palette": {
    "paper": { "$value": { "colorSpace": "srgb", "components": [0.1, 0.1, 0.12] } },
    "ink": { "$value": { "colorSpace": "srgb", "components": [1, 1, 1] } }
  }
}"#;

fn main() -> Result<(), DtcgError> {
    let mut runtime = DtcgRuntime::from_json("theme.resolver.json", RESOLVER)?;
    runtime.provide_json_batch([("base.json", BASE), ("dark.json", DARK)])?;

    let light = runtime.resolve(&Theme::Light.selection())?.ready().unwrap();
    let dark = runtime.resolve(&Theme::Dark.selection())?.ready().unwrap();

    assert_eq!(light.get("surface.background").unwrap().value["components"][0], 1);
    assert_eq!(dark.get("surface.background").unwrap().value["components"][0], 0.1);
    Ok(())
}
```

If the user can pick from a list, build the picker from `runtime.modifiers()` (names, contexts, default) instead of hard-coding the enum.

## Wire DtcgTokens into an app

Project once per theme into a struct your widgets read, and share it as `Arc`. A small store loads the runtime once and builds each theme's struct on first use.

```rust
use std::{collections::HashMap, sync::Arc};

use dxn_dtcg::values::{Color, Dimension};
use dxn_dtcg::{DtcgError, DtcgRuntime, DtcgTokens};

#[derive(Debug, DtcgTokens)]
struct ButtonTokens {
    background: Color,
    #[dtcg(rename = "paddingX")]
    padding_x: Dimension,
}

#[derive(Debug, DtcgTokens)]
struct AppTheme {
    #[dtcg(nested)]
    button: ButtonTokens,
    #[dtcg(path = "surface.background")]
    background: Color,
    /// Not every theme defines a focus ring width.
    #[dtcg(path = "focus.width")]
    focus_width: Option<Dimension>,
}

struct ThemeStore {
    runtime: DtcgRuntime,
    themes: HashMap<String, Arc<AppTheme>>,
}

impl ThemeStore {
    fn new(runtime: DtcgRuntime) -> Self {
        Self { runtime, themes: HashMap::new() }
    }

    /// The projected theme for a theme name; built on first use.
    fn theme(&mut self, name: &str) -> Result<Arc<AppTheme>, DtcgError> {
        if let Some(theme) = self.themes.get(name) {
            return Ok(Arc::clone(theme));
        }
        let theme = self
            .runtime
            .project::<AppTheme>(&[("theme", name)])?
            .ready()
            .expect("all documents are provided");
        let theme = Arc::new(theme);
        self.themes.insert(name.to_owned(), Arc::clone(&theme));
        Ok(theme)
    }
}

fn main() -> Result<(), DtcgError> {
    let mut runtime = DtcgRuntime::from_json(
        "app.resolver.json",
        r##"{
          "version": "2025.10",
          "modifiers": { "theme": { "default": "light", "contexts": {
            "light": [{ "surface": { "background": { "$type": "color",
              "$value": { "colorSpace": "srgb", "components": [1, 1, 1] } } } }],
            "dark": [{ "surface": { "background": { "$type": "color",
              "$value": { "colorSpace": "srgb", "components": [0, 0, 0] } } } }]
          } } },
          "resolutionOrder": [
            { "type": "set", "name": "components", "sources": [{
              "button": {
                "background": { "$type": "color", "$value": "{surface.background}" },
                "paddingX": { "$type": "dimension", "$value": { "value": 12, "unit": "px" } }
              }
            }] },
            { "$ref": "#/modifiers/theme" }
          ]
        }"##,
    )?;
    runtime.set_root("app.resolver.json")?;

    let mut store = ThemeStore::new(runtime);
    let dark = store.theme("dark")?;
    assert_eq!(dark.button.background.components[0], dxn_dtcg::values::Component::Number(0.0));
    assert!(dark.focus_width.is_none());
    assert!(Arc::ptr_eq(&dark, &store.theme("dark")?)); // cached
    Ok(())
}
```

Notice that `button.background` aliases `surface.background`, which only exists in the theme contexts: the alias follows whichever context is selected. When a renderer type is not `FromDtcgValue`, convert with `#[dtcg(with = "...")]` in your own crate, as shown in [Typed projection](./typed-projection.md#use-with-for-foreign-types).

## Cache resolutions per theme

`resolve` recomputes every time. The catalog revision plus the normalized selection identify a result exactly, so they make a precise cache key. `normalize_selection` collapses `("Theme", "DARK")` and `("theme", "dark")` and omitted defaults into one key.

```rust
use std::{collections::HashMap, sync::Arc};

use dxn_dtcg::{DtcgError, DtcgRuntime, Resolution, ResolvedTokens};

#[derive(Default)]
struct TokenCache {
    revision: u64,
    entries: HashMap<Vec<(String, String)>, Arc<ResolvedTokens>>,
}

impl TokenCache {
    fn resolve(
        &mut self,
        runtime: &DtcgRuntime,
        selection: &[(&str, &str)],
    ) -> Result<Resolution<Arc<ResolvedTokens>>, DtcgError> {
        if runtime.catalog_revision() != self.revision {
            self.entries.clear();
            self.revision = runtime.catalog_revision();
        }
        let key = match runtime.normalize_selection(selection)? {
            Resolution::Ready(key) => key,
            Resolution::Missing(requests) => return Ok(Resolution::Missing(requests)),
        };
        if let Some(tokens) = self.entries.get(&key) {
            return Ok(Resolution::Ready(Arc::clone(tokens)));
        }
        Ok(match runtime.resolve(selection)? {
            Resolution::Ready(tokens) => {
                let tokens = Arc::new(tokens);
                self.entries.insert(key, Arc::clone(&tokens));
                Resolution::Ready(tokens)
            }
            Resolution::Missing(requests) => Resolution::Missing(requests),
        })
    }
}

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json("t.json", r#"{ "a": { "$type": "number", "$value": 1 } }"#)?;
    let mut cache = TokenCache::default();
    let first = cache.resolve(&runtime, &[])?.ready().unwrap();
    let second = cache.resolve(&runtime, &[])?.ready().unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    Ok(())
}
```

To decide whether to re-render after a reload, compare `fingerprint()` instead: it is equal when no value, type, winning source or token order changed.

## Read the theme from a settings file

Settings and URL handlers usually carry inputs as JSON. `selection_from_json` accepts an object of strings and reports anything else as `InvalidInput`; `selection_refs` borrows the pairs.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime, selection_from_json, selection_refs};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "r.json",
        r##"{
          "version": "2025.10",
          "modifiers": { "theme": { "default": "light", "contexts": {
            "light": [{ "bg": { "$type": "number", "$value": 0 } }],
            "dark": [{ "bg": { "$type": "number", "$value": 1 } }]
          } } },
          "resolutionOrder": [{ "$ref": "#/modifiers/theme" }]
        }"##,
    )?;

    let settings: serde_json::Value = serde_json::from_str(r#"{ "theme": "Dark" }"#).unwrap();
    let inputs = selection_from_json(&settings)?;
    let tokens = runtime.resolve(&selection_refs(&inputs))?.ready().unwrap();
    assert_eq!(tokens.get("bg").unwrap().value, 1);

    // A stale setting names a context that no longer exists: the error lists the allowed ones.
    let stale = selection_from_json(&serde_json::json!({ "theme": "sepia" }))?;
    let error = runtime.resolve(&selection_refs(&stale)).unwrap_err();
    assert!(error.to_string().ends_with("allowed contexts: light, dark"));
    Ok(())
}
```

## Generate CSS custom properties

`ResolvedToken::typed` gives every value a shape you can format. This turns the colors, dimensions and numbers of a theme into a `:root` block; other types are skipped.

```rust
use dxn_dtcg::values::{ColorSpace, Component, DimensionUnit, TypedValue};
use dxn_dtcg::{DtcgError, DtcgRuntime, ResolvedTokens, ValueExtensions};

fn css(tokens: &ResolvedTokens, extensions: &ValueExtensions) -> String {
    let mut out = String::from(":root {\n");
    for token in tokens.iter() {
        let value = match token.typed(extensions) {
            Ok(TypedValue::Color(color)) if color.color_space == ColorSpace::Srgb => {
                let channel = |component: Component| match component {
                    Component::Number(n) => (n * 255.0).round() as u8,
                    Component::None => 0,
                };
                let [r, g, b] = color.components.map(channel);
                format!("rgb({r} {g} {b} / {})", color.alpha)
            }
            Ok(TypedValue::Dimension(dimension)) => match dimension.unit {
                DimensionUnit::Px => format!("{}px", dimension.value),
                DimensionUnit::Rem => format!("{}rem", dimension.value),
            },
            Ok(TypedValue::Number(number)) => number.to_string(),
            _ => continue,
        };
        out.push_str(&format!("  --{}: {value};\n", token.path.replace('.', "-")));
    }
    out.push_str("}\n");
    out
}

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "color": { "$type": "color", "brand": { "$value": { "colorSpace": "srgb", "components": [0.2, 0.4, 0.9] } } },
          "space": { "$type": "dimension", "md": { "$value": { "value": 1, "unit": "rem" } } }
        }"##,
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();
    let stylesheet = css(&tokens, &runtime.options().extensions);
    assert_eq!(
        stylesheet,
        ":root {\n  --color-brand: rgb(51 102 230 / 1);\n  --space-md: 1rem;\n}\n"
    );
    Ok(())
}
```

## Reload a token file

When a file changes on disk, replace it in the catalog and compare fingerprints to see whether anything the UI shows actually changed. `replace_json` fails for an unknown id and for malformed JSON, and leaves the catalog untouched in both cases, so a half-saved file never breaks the running theme.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime, ResolvedTokens};

/// Returns the new tokens when the reload changed them.
fn reload(
    runtime: &mut DtcgRuntime,
    selection: &[(&str, &str)],
    id: &str,
    text: String,
    current: &ResolvedTokens,
) -> Result<Option<ResolvedTokens>, DtcgError> {
    runtime.replace_json(id, text)?;
    let next = runtime.resolve(selection)?.ready().expect("same documents as before");
    Ok((next.fingerprint() != current.fingerprint()).then_some(next))
}

fn main() -> Result<(), DtcgError> {
    let mut runtime = DtcgRuntime::from_json("t.json", r#"{ "gap": { "$type": "number", "$value": 4 } }"#)?;
    let current = runtime.resolve(&[])?.ready().unwrap();

    // Saved without a change to values: nothing to repaint.
    let same = r#"{ "gap": { "$type": "number", "$value": 4, "$description": "Spacing" } }"#;
    assert!(reload(&mut runtime, &[], "t.json", same.into(), &current)?.is_none());

    // Truncated save: rejected, previous document stays.
    let broken = reload(&mut runtime, &[], "t.json", r#"{ "gap": "#.into(), &current);
    assert!(matches!(broken, Err(DtcgError::InvalidJson { .. })));

    let changed = r#"{ "gap": { "$type": "number", "$value": 8 } }"#;
    let next = reload(&mut runtime, &[], "t.json", changed.into(), &current)?.unwrap();
    assert_eq!(next.get("gap").unwrap().value, 8);
    Ok(())
}
```

If edits come keystroke by keystroke in a large file set, see [Incremental resolution](./incremental.md).

## Report deprecated tokens

`$deprecated` is inherited from groups, so a token's `deprecated` field already answers "is this deprecated, and why". List them after resolving, for a lint step or a design-system changelog.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "opacity": {
            "$type": "number",
            "$deprecated": "Use emphasis.* instead",
            "disabled": { "$value": 0.4 },
            "overlay": { "$value": 0.6, "$deprecated": false }
          },
          "emphasis": { "$type": "number", "low": { "$value": 0.4 } }
        }"##,
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();

    let deprecated: Vec<(&str, Option<&str>)> = tokens
        .iter()
        .filter(|token| token.deprecated.is_deprecated())
        .map(|token| (&*token.path, token.deprecated.reason()))
        .collect();
    assert_eq!(deprecated, [("opacity.disabled", Some("Use emphasis.* instead"))]);
    Ok(())
}
```

## Validate every theme combination in CI

`resolve` only checks the selected contexts, so a typo in the `dark` context goes unnoticed until someone picks it. In a token repository's CI, resolve the full cartesian product of the root modifiers. `validate_complete` first makes sure every document is loadable.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime, ModifierAxis};

fn all_selections(axes: &[ModifierAxis]) -> Vec<Vec<(&str, &str)>> {
    let mut selections = vec![Vec::new()];
    for axis in axes {
        selections = selections
            .into_iter()
            .flat_map(|selection| {
                axis.contexts.iter().map(move |context| {
                    let mut next = selection.clone();
                    next.push((axis.name.as_str(), context.as_str()));
                    next
                })
            })
            .collect();
    }
    selections
}

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "r.json",
        r##"{
          "version": "2025.10",
          "sets": { "base": { "sources": [{ "gap": { "$type": "number", "$value": 8 } }] } },
          "modifiers": {
            "theme": { "default": "light", "contexts": {
              "light": [],
              "dark": [{ "gap": { "$type": "number", "$value": "{missing}" } }]
            } },
            "density": { "default": "comfortable", "contexts": { "comfortable": [], "compact": [] } }
          },
          "resolutionOrder": [
            { "$ref": "#/sets/base" }, { "$ref": "#/modifiers/theme" }, { "$ref": "#/modifiers/density" }
          ]
        }"##,
    )?;

    // Structure and documents are fine in every context...
    assert!(runtime.validate_complete()?.ready().is_some());

    // ...but the dark context has a dangling alias.
    let axes = runtime.modifiers()?.ready().unwrap();
    let failing: Vec<_> = all_selections(&axes)
        .into_iter()
        .filter(|selection| runtime.resolve(selection).is_err())
        .collect();
    assert_eq!(
        failing,
        [
            vec![("theme", "dark"), ("density", "comfortable")],
            vec![("theme", "dark"), ("density", "compact")]
        ]
    );
    Ok(())
}
```

`modifiers()` lists the root resolver's axes only. For a graph with nested resolvers, add their modifiers to the combinations, or rely on their defaults. Print each failing selection with its error and exit non-zero from `main` to fail the job.

## Share one runtime across threads

Resolution takes `&self` and the runtime is `Send + Sync`, so readers can run in parallel behind an `RwLock`; take the write lock only to provide or replace documents.

```rust
use std::sync::{Arc, RwLock};
use std::thread;

use dxn_dtcg::{DtcgError, DtcgRuntime};

fn main() -> Result<(), DtcgError> {
    let runtime = Arc::new(RwLock::new(DtcgRuntime::from_json(
        "r.json",
        r##"{
          "version": "2025.10",
          "modifiers": { "theme": { "default": "light", "contexts": {
            "light": [{ "bg": { "$type": "number", "$value": 0 } }],
            "dark": [{ "bg": { "$type": "number", "$value": 1 } }]
          } } },
          "resolutionOrder": [{ "$ref": "#/modifiers/theme" }]
        }"##,
    )?));

    let readers: Vec<_> = ["light", "dark"]
        .into_iter()
        .map(|theme| {
            let runtime = Arc::clone(&runtime);
            thread::spawn(move || {
                let runtime = runtime.read().unwrap();
                let tokens = runtime.resolve(&[("theme", theme)]).unwrap().ready().unwrap();
                tokens.get("bg").unwrap().value.clone()
            })
        })
        .collect();
    let values: Vec<_> = readers.into_iter().map(|reader| reader.join().unwrap()).collect();
    assert_eq!(values, [0, 1]);
    Ok(())
}
```

## Build an editor feature

Editors and linters for token files combine three groups of APIs, each with its own page.

| Feature | APIs | Page |
|---|---|---|
| Re-resolve after each edit and patch the shown snapshot | `replace_json_and_resolve_affected`, `apply_affected` | [Incremental resolution](./incremental.md) |
| Show where a token's source comes from; preview an unsaved set or context | `inspect_source`, `preview_source`, `preview_occurrence`, `validate_source` | [Source inspection](./inspection.md) |
| Rename or move a token file and fix every reference | `source_references()` | [Source inspection](./inspection.md#listing-reference-sites) |
| Show the authored value next to the resolved one | `ResolvedToken::authored_value` and `value` | [Aliases and references](./aliases.md) |
| Export the final theme for another tool | `ResolvedTokens::to_document()` | [Token documents](./token-documents.md#export) |

Next: [Incremental resolution](./incremental.md), or [Troubleshooting](./troubleshooting.md).
