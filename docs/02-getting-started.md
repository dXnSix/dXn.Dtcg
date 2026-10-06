# 2. Getting started

This tutorial goes from one token file to themed, typed tokens loaded on demand. Each step is a
complete program you can paste into `src/main.rs`.

- [Add the dependency](#add-the-dependency)
- [Step 1: Resolve one document](#step-1-resolve-one-document)
- [Step 2: Combine files with a resolver](#step-2-combine-files-with-a-resolver)
- [Step 3: Load documents on demand](#step-3-load-documents-on-demand)
- [Step 4: Discover and select contexts](#step-4-discover-and-select-contexts)
- [Step 5: Project tokens into structs](#step-5-project-tokens-into-structs)
- [Step 6: Handle errors](#step-6-handle-errors)
- [Where to go next](#where-to-go-next)

## Add the dependency

```toml
[dependencies]
dtcg_runtime = { git = "https://github.com/rayx-ui/RayX.Dtcg" }
serde_json = "1"
```

Resolved values are `serde_json::Value`s. You need `serde_json` as a direct dependency only to name
that type or to build values with `json!`; indexing (`value["unit"]`) and accessors (`as_f64()`)
work without it.

## Step 1: Resolve one document

`DtcgRuntime::from_json` creates a runtime, stores the document in its catalog under a logical
name, and makes that document the *root*. `resolve` takes a selection, which is empty here because
a plain token document has no modifiers.

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime, Resolution};

const TOKENS: &str = r##"{
  "palette": {
    "$type": "color",
    "blue500": { "$value": { "colorSpace": "srgb", "components": [0.15, 0.39, 0.92], "hex": "#2663eb" } }
  },
  "space": {
    "$type": "dimension",
    "$description": "The spacing scale",
    "2": { "$value": { "value": 8, "unit": "px" } },
    "4": { "$value": { "value": 16, "unit": "px" } }
  },
  "button": {
    "background": { "$value": "{palette.blue500}" },
    "padding": { "$value": "{space.4}" }
  }
}"##;

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json("tokens.json", TOKENS)?;

    let tokens = match runtime.resolve(&[])? {
        Resolution::Ready(tokens) => tokens,
        Resolution::Missing(requests) => panic!("unexpected requests: {requests:?}"),
    };

    // Tokens come back in a deterministic resolution order.
    for token in tokens.iter() {
        println!(
            "{:<18} {:<10} {}",
            token.path,
            token.token_type.as_deref().unwrap_or("-"),
            token.value
        );
    }

    let padding = tokens.get("button.padding").unwrap();
    assert_eq!(padding.token_type.as_deref(), Some("dimension")); // taken from the alias target
    assert_eq!(padding.authored_value, "{space.4}");
    assert_eq!(padding.value["value"], 16);
    assert_eq!(padding.source.as_str(), "tokens.json");
    Ok(())
}
```

Each `ResolvedToken` carries:

| Field | Meaning |
|---|---|
| `path` | The dotted token path, for example `button.padding` |
| `token_type` | The effective DTCG type. Always `Some` for a resolved token. |
| `authored_value` | The `$value` as written, aliases included |
| `value` | The value after every alias and reference is resolved |
| `extensions` | The token's `$extensions` object, or `Null` |
| `source` | The resource id of the document that supplied the winning definition |

The `button` tokens have no `$type` of their own. A token whose whole value is an alias takes the
type of its target.

## Step 2: Combine files with a resolver

Real design systems split tokens across files and add variants. A *resolver* document lists the
sources and the order to stack them in. Here the root is a resolver that references two token
files, and the `theme` modifier switches a dark override in and out.

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime};

const RESOLVER: &str = r##"{
  "version": "2025.10",
  "sets": {
    "foundation": { "sources": [{ "$ref": "tokens/foundation.json" }] }
  },
  "modifiers": {
    "theme": {
      "default": "light",
      "contexts": {
        "light": [],
        "dark": [{ "$ref": "tokens/dark.json" }]
      }
    }
  },
  "resolutionOrder": [{ "$ref": "#/sets/foundation" }, { "$ref": "#/modifiers/theme" }]
}"##;

const FOUNDATION: &str = r##"{
  "palette": {
    "$type": "color",
    "white": { "$value": { "colorSpace": "srgb", "components": [1, 1, 1] } },
    "gray900": { "$value": { "colorSpace": "srgb", "components": [0.07, 0.08, 0.1] } }
  },
  "color": {
    "$type": "color",
    "background": { "$value": "{palette.white}" },
    "text": { "$value": "{palette.gray900}" }
  }
}"##;

const DARK: &str = r##"{
  "color": {
    "$type": "color",
    "background": { "$value": "{palette.gray900}" },
    "text": { "$value": "{palette.white}" }
  }
}"##;

fn main() -> Result<(), DtcgError> {
    // Resource ids are logical, relative names. References inside a document resolve
    // against that document's directory, like relative URLs.
    let mut runtime = DtcgRuntime::from_json("acme.resolver.json", RESOLVER)?;
    runtime.provide_json("tokens/foundation.json", FOUNDATION)?;
    runtime.provide_json("tokens/dark.json", DARK)?;

    let light = runtime.resolve(&[])?.ready().unwrap(); // "theme" falls back to "light"
    let dark = runtime.resolve(&[("theme", "dark")])?.ready().unwrap();

    let background = |tokens: &dtcg_runtime::ResolvedTokens| {
        tokens.get("color.background").unwrap().value["components"][0].clone()
    };
    assert_eq!(background(&light), 1);
    assert_eq!(background(&dark), 0.07);

    // The dark definition replaced the whole token, and records where it came from.
    assert_eq!(dark.get("color.background").unwrap().source.as_str(), "tokens/dark.json");
    Ok(())
}
```

Note that `dark.json` repeats `"$type": "color"` on its `color` group. An overriding token replaces
the earlier token completely, type included, so it must carry its own type: on the token, on a
group in the same file, or by being a whole-value alias. See
[overlay semantics](06-resolvers-and-modifiers.md#overlay-semantics).

## Step 3: Load documents on demand

Above, every document was provided up front. Usually you do not know in advance which files a
selection needs, and you do not want to load the dark theme for a light-mode user. Instead, set the
root and let `resolve` tell you what is missing:

- `Resolution::Ready(tokens)` means everything needed was in the catalog;
- `Resolution::Missing(requests)` lists the documents it reached but does not have. Provide them
  and resolve again.

This program uses a `HashMap` as the "file system". Swap in `std::fs`, HTTP or an asset bundle in a
real application; [Document loading](07-document-loading.md) shows several loaders.

```rust
use std::collections::HashMap;

use dtcg_runtime::{DtcgError, DtcgRuntime, Resolution, ResolvedTokens};

fn storage() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        (
            "design/acme.resolver.json",
            r##"{
              "version": "2025.10",
              "sets": { "base": { "sources": [{ "$ref": "tokens/base.json" }] } },
              "modifiers": { "theme": { "default": "light", "contexts": {
                "light": [{ "$ref": "tokens/light.json" }],
                "dark": [{ "$ref": "tokens/dark.json" }]
              } } },
              "resolutionOrder": [{ "$ref": "#/sets/base" }, { "$ref": "#/modifiers/theme" }]
            }"##,
        ),
        ("design/tokens/base.json", r#"{ "radius": { "$type": "number", "$value": 6 } }"#),
        ("design/tokens/light.json", r#"{ "elevation": { "$type": "number", "$value": 1 } }"#),
        ("design/tokens/dark.json", r#"{ "elevation": { "$type": "number", "$value": 3 } }"#),
    ])
}

fn resolve_loading(
    runtime: &mut DtcgRuntime,
    files: &HashMap<&str, &str>,
    selection: &[(&str, &str)],
    loads: &mut Vec<String>,
) -> Result<ResolvedTokens, DtcgError> {
    loop {
        match runtime.resolve(selection)? {
            Resolution::Ready(tokens) => return Ok(tokens),
            Resolution::Missing(requests) => {
                for request in requests {
                    let id = request.resource_id.as_str();
                    let text = files.get(id).unwrap_or_else(|| panic!("no file {id}"));
                    loads.push(id.to_owned());
                    runtime.provide_requested_json(&request, *text)?;
                }
            }
        }
    }
}

fn main() -> Result<(), DtcgError> {
    let files = storage();
    let mut runtime = DtcgRuntime::default();
    runtime.set_root("design/acme.resolver.json")?;

    let mut loads = Vec::new();
    let light = resolve_loading(&mut runtime, &files, &[], &mut loads)?;
    assert_eq!(light.get("elevation").unwrap().value, 1);
    // The root first, then everything the light context needs. dark.json was never read.
    assert_eq!(
        loads,
        ["design/acme.resolver.json", "design/tokens/base.json", "design/tokens/light.json"]
    );

    loads.clear();
    let dark = resolve_loading(&mut runtime, &files, &[("theme", "dark")], &mut loads)?;
    assert_eq!(dark.get("elevation").unwrap().value, 3);
    assert_eq!(loads, ["design/tokens/dark.json"]); // only the new document
    Ok(())
}
```

A few things to notice:

- The root itself is requested the first time, because `set_root` only names it.
- Each `Missing` response lists *every* missing document the runtime could reach in that pass,
  de-duplicated and sorted. Nested resolvers can reveal more documents once provided, which is why
  the loop runs until `Ready`.
- `request.resource_id` is already normalized: `tokens/base.json` referenced from
  `design/acme.resolver.json` becomes `design/tokens/base.json`.
- Documents stay in the catalog, so later selections only fetch what is new.

## Step 4: Discover and select contexts

A UI that lets people pick a theme needs to know which modifiers and contexts exist.
`modifiers()` lists them from the root resolver, and `normalize_selection` fills in defaults and
validates a selection, with error messages that list the allowed values.

This resolver uses *inline* sources, token documents written directly inside it, so the example
needs no other files.

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime};

const RESOLVER: &str = r##"{
  "version": "2025.10",
  "modifiers": {
    "theme": {
      "default": "light",
      "contexts": {
        "light": [{ "surface": { "$type": "number", "$value": 0 } }],
        "dark": [{ "surface": { "$type": "number", "$value": 1 } }]
      }
    },
    "density": {
      "contexts": {
        "comfortable": [{ "gap": { "$type": "dimension", "$value": { "value": 16, "unit": "px" } } }],
        "compact": [{ "gap": { "$type": "dimension", "$value": { "value": 8, "unit": "px" } } }]
      }
    }
  },
  "resolutionOrder": [{ "$ref": "#/modifiers/theme" }, { "$ref": "#/modifiers/density" }]
}"##;

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json("ui.resolver.json", RESOLVER)?;

    for axis in runtime.modifiers()?.ready().unwrap() {
        println!("{}: {:?} (default {:?})", axis.name, axis.contexts, axis.default);
    }
    // theme: ["light", "dark"] (default Some("light"))
    // density: ["comfortable", "compact"] (default None)

    // `density` has no default, so it must be selected.
    let error = runtime.normalize_selection(&[]).unwrap_err();
    assert_eq!(
        error.to_string(),
        "modifier `density` context ``: modifier has no default and was not selected; \
         allowed contexts: comfortable, compact"
    );

    // The normalized selection is complete and ordered by axis, which makes it a good cache key.
    let selection = runtime.normalize_selection(&[("density", "compact")])?.ready().unwrap();
    assert_eq!(
        selection,
        [
            ("theme".to_owned(), "light".to_owned()),
            ("density".to_owned(), "compact".to_owned())
        ]
    );

    let tokens = runtime.resolve(&[("density", "compact")])?.ready().unwrap();
    assert_eq!(tokens.get("gap").unwrap().value["value"], 8);
    Ok(())
}
```

## Step 5: Project tokens into structs

`#[derive(DtcgTokens)]` generates code that reads specific token paths into struct fields. The
built-in conversions cover `String`, `f64`, `f32` and `bool`. For anything else, implement
`FromDtcgValue` for your type or point a field at a conversion function with
`#[dtcg(with = "...")]`.

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime, DtcgTokens, FromDtcgValue, ResolvedToken};

#[derive(Debug, PartialEq)]
struct Px(f32);

impl FromDtcgValue for Px {
    fn from_dtcg(token: &ResolvedToken) -> Result<Self, DtcgError> {
        match (token.value["value"].as_f64(), token.value["unit"].as_str()) {
            (Some(value), Some("px")) => Ok(Px(value as f32)),
            (Some(value), Some("rem")) => Ok(Px(value as f32 * 16.0)),
            _ => Err(DtcgError::Projection {
                path: token.path.to_string(),
                message: "expected a px or rem dimension".into(),
            }),
        }
    }
}

#[derive(Debug, PartialEq, DtcgTokens)]
struct Typography {
    #[dtcg(rename = "fontFamily")]
    family: String,
    #[dtcg(rename = "fontWeight")]
    weight: f64,
}

#[derive(Debug, PartialEq, DtcgTokens)]
#[dtcg(root = "card")]
struct CardTokens {
    padding: Px,
    #[dtcg(path = "corner.radius")]
    radius: Px,
    #[dtcg(nested, path = "title")]
    title: Typography,
}

const TOKENS: &str = r##"{
  "card": {
    "padding": { "$type": "dimension", "$value": { "value": 1, "unit": "rem" } },
    "corner": { "radius": { "$type": "dimension", "$value": { "value": 12, "unit": "px" } } },
    "title": {
      "fontFamily": { "$type": "fontFamily", "$value": "Inter" },
      "fontWeight": { "$type": "fontWeight", "$value": 650 }
    },
    "shadow": { "$type": "number", "$value": 2 }
  }
}"##;

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json("card.json", TOKENS)?;
    let card = runtime.project::<CardTokens>(&[])?.ready().unwrap();

    assert_eq!(
        card,
        CardTokens {
            padding: Px(16.0),
            radius: Px(12.0),
            title: Typography { family: "Inter".into(), weight: 650.0 },
        }
    );
    Ok(())
}
```

- With no attribute, a field reads the token named exactly like the field (`padding`). Names are
  not case-converted, so use `rename` or `path` for camelCase tokens.
- `#[dtcg(root = "card")]` prefixes every path in the struct.
- `#[dtcg(nested, path = "title")]` projects another `DtcgTokens` struct below `card.title`.
- `card.shadow` is not requested, and that is fine.

`DtcgRuntime::project` resolves and projects in one call. If you project several structs from the
same selection, resolve once and call `T::project(&tokens, runtime.registry())` for each.

## Step 6: Handle errors

There are two kinds of "not ready":

- `Ok(Resolution::Missing(..))`: not an error. Provide the documents and try again.
- `Err(DtcgError)`: the documents, the selection or a projection are wrong. Errors name the
  resource and the token path.

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime};

fn main() {
    let runtime = DtcgRuntime::from_json(
        "broken.json",
        r#"{
          "spacing": { "$type": "dimension", "md": { "$value": { "value": 16, "unit": "em" } } },
          "gap": { "$value": "{spacing.lg}" }
        }"#,
    )
    .unwrap();

    match runtime.resolve(&[]) {
        Err(DtcgError::TokenType { resource, path, message }) => {
            // `em` is not a DTCG 2025.10 dimension unit.
            assert_eq!(resource.as_str(), "broken.json");
            assert_eq!(path, "spacing.md");
            assert_eq!(message, "resolved value does not match DTCG type `dimension`");
        }
        other => panic!("unexpected result: {other:?}"),
    }
}
```

Resolution stops at the first error, here `spacing.md`. With that fixed, the next run would report
the dangling alias in `gap` as ``projection `spacing.lg`: missing alias target``. The full list of
errors is in [Errors, limits and diagnostics](11-errors-limits-and-diagnostics.md).

## Where to go next

- What may appear in token files: [Token documents](03-token-documents.md) and
  [Aliases and references](04-aliases-and-references.md).
- Everything resolvers can express: [Resolvers and modifiers](06-resolvers-and-modifiers.md).
- Production loaders, caching and path safety: [Document loading](07-document-loading.md).
- All projection attributes: [Typed projection](08-typed-projection.md).
