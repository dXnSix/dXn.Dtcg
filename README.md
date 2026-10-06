# RayX.Dtcg

[![CI](https://github.com/rayx-ui/RayX.Dtcg/actions/workflows/ci.yml/badge.svg)](https://github.com/rayx-ui/RayX.Dtcg/actions/workflows/ci.yml)
![Rust 1.95+](https://img.shields.io/badge/rust-1.95%2B-blue)
![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)

A Rust runtime for [Design Tokens Community Group](https://www.designtokens.org/) (DTCG)
**2025.10** documents. It reads design-token files and resolver documents, applies themes and
other modifiers, resolves aliases, checks every value against its DTCG type, and projects the
result into ordinary Rust structs.

Design systems ship their tokens as JSON: a palette, semantic aliases on top of it, and light,
dark, brand or density variants. At run time an application needs one thing from all of that,
the effective value of each token for the variant the user picked. `dtcg_runtime` computes it.
It does no I/O of its own: it tells you which documents it needs, and you load them from disk,
over HTTP, from an asset bundle or from memory. The same code therefore runs in a desktop app,
on a server, in a build script or in the browser through WebAssembly.

## Features

- **The DTCG 2025.10 token format**: nested groups, `$type` inheritance, `$root` tokens, group
  extension with `$extends`, and `$description`, `$deprecated` and `$extensions` metadata.
- **Aliases and references**: `{curly.brace}` aliases and JSON Pointer `$ref` references, inside
  composite values too, with cycle detection. A reference can adjust what it points to, for
  example to take a color with a different alpha.
- **Resolver documents**: sets, modifiers with named contexts and defaults, `resolutionOrder`,
  inline and nested resolvers, file fragments, and source references with shallow overrides.
- **Value checks**: all 13 DTCG types (color, dimension, typography, shadow, gradient and the rest)
  are validated after aliases resolve, so bad data surfaces as an error naming the file and the
  token path.
- **Lazy loading without I/O**: `resolve` returns either the tokens or the list of documents it
  still needs (`Resolution::Missing`). Only the documents for the active contexts are requested.
- **Typed projection**: `#[derive(DtcgTokens)]` maps token paths onto struct fields. You can add
  your own conversions with `FromDtcgValue` or `#[dtcg(with = "...")]`.
- **Incremental updates**: re-resolve only the tokens an edit changed, plus the tokens that depend
  on them. Each resolved set carries a SHA-256 fingerprint you can use as a cache key.
- **Editor tooling**: inspect authored sources and where they came from, preview unsaved sources
  without touching the catalog, and list every document reference so files can be moved safely.
- **Bounded on untrusted input**: configurable limits on document count, reference depth and alias
  depth. Resource ids are normalized and cannot climb above the root, and an opt-in policy
  rejects absolute paths and URLs.

## Platform support

The crates are pure, safe Rust. They have no build scripts, no `unsafe` code, no async runtime and
no file-system or network access, so they build for any target that has the Rust standard library.

| Platform | Status |
|---|---|
| Linux x86_64 | Tested in CI: formatting, Clippy, tests, packaging |
| Windows x86_64 | Tested in CI: formatting, Clippy, tests, packaging |
| WebAssembly (`wasm32-unknown-unknown`) | Built in CI; needs no host I/O, threads or system calls |
| macOS, iOS, Android and other `std` targets | Expected to work (no platform-specific code); not covered by CI |

- **Rust**: 1.95 or newer, edition 2024. `no_std` is not supported.
- **Threads**: `DtcgRuntime` and `ResolvedTokens` are `Send + Sync`. Resolution borrows the runtime
  immutably, so several threads can resolve against one runtime at the same time.

## Installation

The crates are not on crates.io yet. Depend on the Git repository:

```toml
[dependencies]
dtcg_runtime = { git = "https://github.com/rayx-ui/RayX.Dtcg" }
# Optional: only needed to construct or name `serde_json::Value`s yourself.
serde_json = "1"
```

`dtcg_runtime` re-exports the derive macros, so you do not depend on `dtcg_runtime_derive`
directly. The one optional feature, `diagnostics`, adds redacted error diagnostics and resolution
counters for test harnesses.

## Examples

### Resolve a token document

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime, Resolution};

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

### Switch themes with a resolver

A resolver document lists the token sets to apply in order, plus *modifiers* whose *contexts* can
be switched at run time. Later sources replace earlier tokens with the same path, and aliases
resolve against that final result.

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime};

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
    "$type": "color",
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
    assert_ne!(light.fingerprint(), dark.fingerprint());
    Ok(())
}
```

### Load documents on demand

The runtime never reads files itself. When it reaches a document it does not have yet, it
returns `Resolution::Missing` with one request per document. You load those and resolve again.
Only the documents of the selected contexts are requested.

```rust,no_run
use std::{error::Error, fs, path::Path};

use dtcg_runtime::{DtcgRuntime, LocalReferencePolicy, Resolution, ResolvedTokens};

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

### Project tokens into structs

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime, DtcgTokens, FromDtcgValue, ResolvedToken};

/// Logical pixels; `rem` converts at 16 px.
#[derive(Debug, PartialEq)]
struct Px(f64);

impl FromDtcgValue for Px {
    fn from_dtcg(token: &ResolvedToken) -> Result<Self, DtcgError> {
        match (token.value["value"].as_f64(), token.value["unit"].as_str()) {
            (Some(value), Some("px")) => Ok(Px(value)),
            (Some(value), Some("rem")) => Ok(Px(value * 16.0)),
            _ => Err(DtcgError::Projection {
                path: token.path.to_string(),
                message: "expected a dimension".into(),
            }),
        }
    }
}

/// A conversion for a type you cannot implement `FromDtcgValue` for.
fn srgb(token: &ResolvedToken) -> Result<[f32; 3], DtcgError> {
    let error = || DtcgError::Projection {
        path: token.path.to_string(),
        message: "expected an sRGB color".into(),
    };
    if token.value["colorSpace"] != "srgb" {
        return Err(error());
    }
    let components = token.value["components"].as_array().ok_or_else(error)?;
    let mut rgb = [0.0; 3];
    for (channel, component) in rgb.iter_mut().zip(components) {
        *channel = component.as_f64().ok_or_else(error)? as f32;
    }
    Ok(rgb)
}

#[derive(Debug, DtcgTokens)]
#[dtcg(root = "button")]
struct ButtonTokens {
    #[dtcg(with = "srgb")]
    background: [f32; 3],
    #[dtcg(rename = "paddingX")]
    padding_x: Px,
    #[dtcg(path = "radius.default")]
    radius: Px,
    #[dtcg(rename = "fontWeight")]
    font_weight: f64,
}

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "button.tokens.json",
        r#"{
          "button": {
            "background": { "$type": "color", "$value": { "colorSpace": "srgb", "components": [0.2, 0.4, 0.9] } },
            "paddingX": { "$type": "dimension", "$value": { "value": 0.75, "unit": "rem" } },
            "radius": { "$type": "dimension", "default": { "$value": { "value": 6, "unit": "px" } } },
            "fontWeight": { "$type": "fontWeight", "$value": 600 },
            "unused": { "$type": "number", "$value": 1 }
          }
        }"#,
    )?;

    let button = runtime.project::<ButtonTokens>(&[])?.ready().expect("ready");
    assert_eq!(button.padding_x, Px(12.0));
    assert_eq!(button.radius, Px(6.0));
    assert_eq!(button.font_weight, 600.0);
    assert_eq!(button.background, [0.2, 0.4, 0.9]);
    Ok(())
}
```

A projection asks only for its own fields. Tokens it does not mention, such as `button.unused`
above, are ignored, but they still resolve and can still be alias targets.

## How it works

```text
JSON strings ──provide──► catalog (ResourceId → document text)
                              │
                              ▼
              discovery: resolutionOrder + selected contexts ──► Missing(requests)
                              │                                        │
                              │               you load them, provide them, resolve again
                              ▼
              overlay: later sources replace whole tokens by path
                              │
                              ▼
              aliases → effective types → value validation
                              │
                              ▼
              ResolvedTokens ──► #[derive(DtcgTokens)] structs
```

## Crates

| Crate | Purpose |
|---|---|
| [`dtcg_runtime`](crates/dtcg_runtime) | Catalog, resolver, value validation, typed projection, source inspection, diagnostics |
| [`dtcg_runtime_derive`](crates/dtcg_runtime_derive) | `#[derive(DtcgTokens)]` and `#[derive(DtcgComponentTokens)]`, re-exported by `dtcg_runtime` |

## Documentation

The full technical documentation is in [`docs/`](docs/README.md):

1. [Overview](docs/01-overview.md): concepts, architecture, design principles, platforms
2. [Getting started](docs/02-getting-started.md): a step-by-step tutorial
3. [Token documents](docs/03-token-documents.md): groups, paths, types, `$root`, `$extends`
4. [Aliases and references](docs/04-aliases-and-references.md): curly aliases and JSON Pointers
5. [Value types](docs/05-value-types.md): the 13 DTCG types and how values are validated
6. [Resolvers and modifiers](docs/06-resolvers-and-modifiers.md): sets, contexts, selections
7. [Document loading](docs/07-document-loading.md): the catalog, resource ids, lazy loading
8. [Typed projection](docs/08-typed-projection.md): the derive macros and conversions
9. [Incremental resolution](docs/09-incremental-resolution.md): fingerprints and affected tokens
10. [Source inspection](docs/10-source-inspection.md): APIs for editors and tooling
11. [Errors, limits and diagnostics](docs/11-errors-limits-and-diagnostics.md)
12. [DTCG 2025.10 conformance](docs/12-conformance.md): support, extensions, deviations
13. [API reference](docs/13-api-reference.md)
14. [Architecture and contributing](docs/14-architecture-and-contributing.md)

## RayX extensions

The crate is UI-neutral, but it supports a few additions used by the
[RayX](https://github.com/rayx-ui) UI toolkit. Other users can ignore them: the `dev.rayx.blur`
value type, typography `lineHeight` given as a dimension, and the Token Editor's `.tokeneditor`
project envelope. See [conformance](docs/12-conformance.md#extensions).

## Development

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo package --workspace
```

See [Architecture and contributing](docs/14-architecture-and-contributing.md) for the source layout,
test groups and release process.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT)
at your option.
