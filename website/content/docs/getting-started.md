---
title: Get started
description: Add dxn_dtcg, resolve a token document, switch themes with a resolver and project tokens into a struct.
group: Start
order: 10
---

`dxn_dtcg` reads [Design Tokens Community Group](https://www.designtokens.org/) (DTCG) 2025.10 documents and returns the effective value of every token for the variant you pick. This page takes you from one JSON file to themed, typed tokens. Each snippet is a complete program for `src/main.rs`.

## Install

The crates are not on crates.io. Releases are git tags, so depend on one:

```toml
[dependencies]
dxn_dtcg = { git = "https://github.com/dXnSix/dXn.Dtcg", tag = "v1.1.0" }
```

`dxn_dtcg` re-exports the derive macros, so you never depend on `dxn_dtcg_derive` yourself. Resolved values are `serde_json::Value`s; add `serde_json = "1"` only when you want to build or name them with `json!`. The newest tag is on the repository's [Releases](https://github.com/dXnSix/dXn.Dtcg/releases) page, and you move to a new release by changing the tag. The crate needs Rust 1.95 or newer (edition 2024).

## Resolve one document

`DtcgRuntime::from_json` stores a document under a logical name and makes it the root. `resolve` takes a selection of modifier contexts; a plain token document has none, so pass `&[]`.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime, Resolution};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r#"{
          "color": {
            "$type": "color",
            "brand": { "$value": { "colorSpace": "srgb", "components": [0.2, 0.4, 0.9] } },
            "action": { "$value": "{color.brand}" }
          },
          "spacing": {
            "$type": "dimension",
            "md": { "$value": { "value": 16, "unit": "px" } }
          }
        }"#,
    )?;

    // A single in-memory document references nothing else, so it is always ready.
    let Resolution::Ready(tokens) = runtime.resolve(&[])? else {
        unreachable!()
    };

    let action = tokens.get("color.action").expect("token exists");
    assert_eq!(action.token_type.as_deref(), Some("color")); // inherited from the group
    assert_eq!(action.authored_value, "{color.brand}"); // what the file says
    assert_eq!(action.value["components"][2], 0.9); // what the alias resolves to

    for token in tokens.iter() {
        println!("{} = {}", token.path, token.value);
    }
    Ok(())
}
```

Every `ResolvedToken` carries:

| Field | Meaning |
|---|---|
| `path` | Dotted token path, for example `color.action` |
| `token_type` | The effective DTCG type; always `Some` after resolution |
| `authored_value` | The `$value` as written, aliases included |
| `value` | The value after every alias and reference is resolved and validated |
| `description`, `deprecated`, `extensions` | The token's own metadata (`deprecated` is inherited from groups) |
| `source` | Resource id of the document that supplied the winning definition |

`tokens.iter()` yields tokens in a deterministic order, and `tokens.fingerprint()` is a SHA-256 hex digest you can use as a cache key.

## Switch themes with a resolver

Real design systems split tokens across files and add variants. A resolver document lists token sources in merge order and declares modifiers whose contexts you switch at run time. Here `dark.tokens.json` overrides two tokens of the base file.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

const RESOLVER: &str = r##"{
  "version": "2025.10",
  "sets": {
    "base": { "sources": [{ "$ref": "base.tokens.json" }] }
  },
  "modifiers": {
    "theme": {
      "default": "light",
      "contexts": {
        "light": [],
        "dark": [{ "$ref": "dark.tokens.json" }]
      }
    }
  },
  "resolutionOrder": [{ "$ref": "#/sets/base" }, { "$ref": "#/modifiers/theme" }]
}"##;

const BASE: &str = r#"{
  "palette": {
    "$type": "color",
    "white": { "$value": { "colorSpace": "srgb", "components": [1, 1, 1] } },
    "ink": { "$value": { "colorSpace": "srgb", "components": [0.1, 0.1, 0.12] } }
  },
  "surface": {
    "$type": "color",
    "background": { "$value": "{palette.white}" },
    "text": { "$value": "{palette.ink}" }
  }
}"#;

const DARK: &str = r#"{
  "surface": {
    "background": { "$value": "{palette.ink}" },
    "text": { "$value": "{palette.white}" }
  }
}"#;

fn main() -> Result<(), DtcgError> {
    let mut runtime = DtcgRuntime::from_json("theme.resolver.json", RESOLVER)?;
    runtime.provide_json_batch([("base.tokens.json", BASE), ("dark.tokens.json", DARK)])?;

    let light = runtime.resolve(&[])?.ready().expect("all documents provided");
    let dark = runtime
        .resolve(&[("theme", "dark")])?
        .ready()
        .expect("all documents provided");

    assert_eq!(light.get("surface.background").unwrap().value["components"][0], 1);
    assert_eq!(dark.get("surface.background").unwrap().value["components"][0], 0.1);
    // The dark override inherits `color` from the base `surface` group.
    assert_eq!(dark.get("surface.background").unwrap().token_type.as_deref(), Some("color"));
    assert_ne!(light.fingerprint(), dark.fingerprint());

    // Names match case-insensitively.
    assert!(runtime.resolve(&[("Theme", "Dark")])?.ready().is_some());
    Ok(())
}
```

All active sources merge into one document before aliases resolve: groups merge, a later definition of a token replaces the earlier one whole, and `surface.background` follows whichever definition won. See [Resolvers and modifiers](./resolvers.md).

## Load documents on demand

The runtime never reads files. When resolution reaches a document the catalog lacks, `resolve` returns `Resolution::Missing` with one request per document. You load them however you like, provide them, and resolve again. Only the documents of the selected contexts are requested.

This snippet reads from the file system, so it needs a `design/` directory to run:

```rust
use std::{error::Error, fs, path::Path};

use dxn_dtcg::{DtcgRuntime, LocalReferencePolicy, Resolution, ResolvedTokens};

fn load_theme(dir: &Path, selection: &[(&str, &str)]) -> Result<ResolvedTokens, Box<dyn Error>> {
    let mut runtime = DtcgRuntime::default();
    runtime.set_root("theme.resolver.json")?;
    loop {
        match runtime.resolve(selection)? {
            Resolution::Ready(tokens) => return Ok(tokens),
            Resolution::Missing(requests) => {
                for request in requests {
                    // Let only relative, local references reach the file system.
                    LocalReferencePolicy::check(&request.referenced_from, &request.original_reference)?;
                    let json = fs::read_to_string(dir.join(request.resource_id.as_str()))?;
                    runtime.provide_requested_json(&request, json)?;
                }
            }
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let tokens = load_theme(Path::new("design/tokens"), &[("theme", "dark")])?;
    println!("{} tokens, fingerprint {}", tokens.len(), tokens.fingerprint());
    Ok(())
}
```

`set_root` only names the root, so the first `resolve` asks for it too. The loop ends because every round provides at least one document that was not in the catalog. [Loading documents](./loading.md) covers resource ids, async fetching and path safety.

## Project tokens into a struct

`#[derive(DtcgTokens)]` maps token paths onto struct fields. The typed models in `dxn_dtcg::values` (`Color`, `Dimension`, `Typography` and the rest) work as field types directly.

```rust
use dxn_dtcg::values::{Color, Dimension};
use dxn_dtcg::{DtcgError, DtcgRuntime, DtcgTokens};

#[derive(Debug, DtcgTokens)]
#[dtcg(root = "button")]
struct ButtonTokens {
    background: Color,
    #[dtcg(rename = "paddingX")]
    padding_x: Dimension,
    #[dtcg(path = "radius.default")]
    radius: Dimension,
    /// `None` when the document defines no `button.borderWidth`.
    #[dtcg(rename = "borderWidth")]
    border_width: Option<Dimension>,
}

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "button.tokens.json",
        r#"{
          "button": {
            "background": { "$type": "color", "$value": { "colorSpace": "srgb", "components": [0.2, 0.4, 0.9] } },
            "paddingX": { "$type": "dimension", "$value": { "value": 0.75, "unit": "rem" } },
            "radius": { "$type": "dimension", "default": { "$value": { "value": 6, "unit": "px" } } },
            "unused": { "$type": "number", "$value": 1 }
          }
        }"#,
    )?;

    let button = runtime.project::<ButtonTokens>(&[])?.ready().expect("ready");
    assert_eq!(button.padding_x.to_px(16.0), 12.0);
    assert_eq!(button.radius.value, 6.0);
    assert!(button.border_width.is_none());
    assert_eq!(button.background.alpha, 1.0); // no `alpha` member: opaque
    Ok(())
}
```

A projection reads only its own fields. Tokens it does not mention, like `button.unused`, still resolve and can still be alias targets. A missing token is an error unless the field is an `Option`. More in [Typed projection](./typed-projection.md).

## Handle errors

There are two kinds of "not ready", and they are different:

- `Ok(Resolution::Missing(..))` is not an error: provide the documents and call again.
- `Err(DtcgError)` means the documents, the selection or a projection are wrong. Errors name the document and token path, and resolution stops at the first one.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

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

Fix `spacing.md` and the next run reports the dangling alias in `gap`. The full list is in [Errors](./errors.md) and [Troubleshooting](./troubleshooting.md).

Next: [DTCG in five minutes](./dtcg-primer.md) if the format is new to you, or [Which API do I use](./choosing-an-api.md).
