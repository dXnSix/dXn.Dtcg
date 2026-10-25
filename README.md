# dXn.Dtcg

[![CI](https://github.com/dXnSix/dXn.Dtcg/actions/workflows/ci.yml/badge.svg)](https://github.com/dXnSix/dXn.Dtcg/actions/workflows/ci.yml)
![Rust 1.97+](https://img.shields.io/badge/rust-1.97%2B-blue)
![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)

**Documentation:** https://dxnsix.github.io/dXn.Dtcg/

A Rust runtime for [Design Tokens Community Group](https://www.designtokens.org/) (DTCG)
**2025.10** documents. It reads design-token files and resolver documents, applies themes and
other modifiers, resolves aliases, checks every value against its DTCG type, and projects the
result into ordinary Rust structs.

Design systems ship their tokens as JSON: a palette, semantic aliases on top of it, and light,
dark, brand or density variants. At run time an application needs one thing from all of that,
the effective value of each token for the variant the user picked. `dxn_dtcg` computes it.
It does no I/O of its own: it tells you which documents it needs, and you load them from disk,
over HTTP, from an asset bundle or from memory. The same code therefore runs in a desktop app,
on a server, in a build script or in the browser through WebAssembly.

## Features

- **The DTCG 2025.10 token format**: nested groups, `$type` inheritance, `$root` tokens, group
  extension with `$extends`, and `$description`, `$deprecated` and `$extensions` metadata exposed
  on every resolved token and group. Passes all 250 fixtures of the official DTCG 2025.10 test
  suite.
- **Aliases and references**: `{curly.brace}` aliases at every position the format allows, with
  type checks, and JSON Pointer `$ref` references at any depth, with cycle detection. A reference
  can adjust what it points to, for example to take a color with a different alpha.
- **Resolver documents**: sets, modifiers with named contexts and defaults, `resolutionOrder`,
  inline and nested resolvers, file fragments, and source references with shallow overrides. All
  active sources merge into one document, as the resolver specification describes. Inputs match
  case-insensitively.
- **Strict value checks**: all 13 DTCG types (color, dimension, typography, shadow, gradient and
  the rest) are validated after aliases resolve, so bad data surfaces as an error naming the file
  and the token path. Value extensions are opt-in.
- **Typed values**: the `values` module models every type (`Color`, `Dimension`, `Typography`,
  `Shadow` and the rest), and `ResolvedToken::typed` converts a resolved token into it.
- **Lazy loading without I/O**: `resolve` returns either the tokens or the list of documents it
  still needs (`Resolution::Missing`). Only the documents for the active contexts are requested.
- **Typed projection**: `#[derive(DtcgTokens)]` maps token paths onto struct fields, with
  `Option` fields for tokens that may be absent. You can add your own conversions with
  `FromDtcgValue` or `#[dtcg(with = "...")]`.
- **Export**: `ResolvedTokens::to_document` writes a resolution back as a plain DTCG token
  document with explicit types and resolved values.
- **Incremental updates**: re-resolve only the tokens an edit changed, plus the tokens that depend
  on them. Each resolved set carries a SHA-256 fingerprint you can use as a cache key.
- **Editor tooling**: inspect authored sources and where they came from, preview unsaved sources
  without touching the catalog, and list every document reference so files can be moved safely.
- **Computed tokens (opt-in)**: with the `expressions` feature, a token can compute its value
  with a JavaScript or TypeScript expression stored in `$extensions["dxn.dtcg.expressions"]`,
  while its `$value` stays the valid DTCG snapshot other tools read. Expressions read other
  tokens as declared inputs, import ES modules and call Rust functions, and run in a
  deterministic sandbox; see [Expressions](#expressions).
- **Bounded on untrusted input**: configurable limits on documents, reference depth, reference
  chains, copied value size and token count, plus fixed bounds on value and path depth. Resource
  ids are normalized, portable and cannot climb above the root, and an opt-in policy rejects
  absolute paths and URLs.

## Platform support

With the default features the crates are pure, safe Rust: no build scripts, no C code, no
`unsafe` code, no async runtime and no file-system or network access, so they build for any target
that has the Rust standard library. The opt-in `expressions` feature adds a JavaScript engine
written in C (QuickJS), compiled with the platform's C compiler; on WebAssembly it needs an LLVM
clang (see [Expressions](docs/15-expressions.md#webassembly)).

| Platform | Status |
|---|---|
| Linux x86_64 | Tested in CI: formatting, Clippy, tests, packaging |
| Windows x86_64 | Tested in CI: formatting, Clippy, tests, packaging |
| macOS arm64 | Tested in CI: formatting, Clippy, tests, packaging |
| WebAssembly (`wasm32-unknown-unknown`) | Built in CI, and with the `expressions` feature also tested under Node; needs no host I/O, threads or system calls |
| iOS, Android and other `std` targets | Expected to work (no platform-specific code); not covered by CI |

- **Rust**: 1.97 or newer, edition 2024. `no_std` is not supported.
- **Threads**: `DtcgRuntime` and `ResolvedTokens` are `Send + Sync`. Resolution borrows the runtime
  immutably, so several threads can resolve against one runtime at the same time.

## Installation

Releases are git tags, not crates.io packages. Depend on a release tag; the latest one is on the
repository's [Releases](https://github.com/dXnSix/dXn.Dtcg/releases) page:

```toml
[dependencies]
dxn_dtcg = { git = "https://github.com/dXnSix/dXn.Dtcg", tag = "v1.1.0" }
# Optional: only needed to construct or name `serde_json::Value`s yourself.
serde_json = "1"
```

`Cargo.lock` records the exact commit of the tag; to update, change the tag by hand. Depend only on
a release tag, never on a branch or a path. `dxn_dtcg` re-exports the derive macros, so you do
not depend on `dxn_dtcg_derive` directly. Two features are optional: `diagnostics` adds redacted
error diagnostics and resolution counters for test harnesses, and `expressions` adds computed
tokens.

Releases up to `v1.0.0` were published as `dtcg_runtime` and `dtcg_runtime_derive`. From `v1.1.0`
they are `dxn_dtcg` and `dxn_dtcg_derive` in `dXnSix/dXn.Dtcg`: rename the dependency and replace
`dtcg_runtime::` paths with `dxn_dtcg::`; the API is otherwise unchanged.

## Examples

### Resolve a token document

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

### Switch themes with a resolver

A resolver document lists the token sets to apply in order, plus *modifiers* whose *contexts* can
be switched at run time. The active sources merge into one document: groups merge, later sources
replace earlier tokens with the same path, and aliases resolve against that final result.

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

    // Inputs match case-insensitively.
    assert!(runtime.resolve(&[("Theme", "Dark")])?.ready().is_some());
    Ok(())
}
```

### Load documents on demand

The runtime never reads files itself. When it reaches a document it does not have yet, it
returns `Resolution::Missing` with one request per document. You load those and resolve again.
Only the documents of the selected contexts are requested.

```rust,no_run
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

### Project tokens into structs

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime, DtcgTokens, FromDtcgValue, ResolvedToken};

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
above, are ignored, but they still resolve and can still be alias targets. A field of type
`Option<T>` is `None` when its token is missing.

### Read typed values

Without a struct of your own, `ResolvedToken::typed` converts any resolved token into the models of
the `values` module:

```rust
use dxn_dtcg::values::{DimensionUnit, TypedValue};
use dxn_dtcg::{DtcgError, DtcgRuntime};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r#"{ "gutter": { "$type": "dimension", "$value": { "value": 1.5, "unit": "rem" } } }"#,
    )?;
    let tokens = runtime.resolve(&[])?.ready().expect("ready");

    let TypedValue::Dimension(gutter) = tokens.get("gutter").unwrap().typed(&runtime.options().extensions)? else {
        unreachable!()
    };
    assert_eq!(gutter.unit, DimensionUnit::Rem);
    assert_eq!(gutter.to_px(16.0), 24.0);
    Ok(())
}
```

## How it works

```text
JSON strings ──provide──► catalog (ResourceId → document text)
                              │
                              ▼
              discovery: resolutionOrder + selected contexts ──► Missing(requests)
                              │                                        │
                              │               you load them, provide them, resolve again
                              ▼
              merge: one document; groups merge, later tokens replace whole
                              │
                              ▼
              aliases → effective types → value validation
                              │
                              ▼
              ResolvedTokens ──► typed values, #[derive(DtcgTokens)] structs, to_document
```

## Crates

| Crate | Purpose |
|---|---|
| [`dxn_dtcg`](crates/dxn_dtcg) | Catalog, resolver, value validation, typed projection, source inspection, diagnostics |
| [`dxn_dtcg_derive`](crates/dxn_dtcg_derive) | `#[derive(DtcgTokens)]`, `#[derive(DtcgComponentTokens)]` and `#[expression_module]`, re-exported by `dxn_dtcg` |

## Documentation

The full technical documentation is in [`docs/`](docs/README.md):

1. [Overview](docs/01-overview.md): concepts, architecture, design principles, platforms
2. [Getting started](docs/02-getting-started.md): a step-by-step tutorial
3. [Token documents](docs/03-token-documents.md): structure, merging, types, `$root`, `$extends`,
   metadata, export
4. [Aliases and references](docs/04-aliases-and-references.md): curly aliases, where they may
   appear, JSON Pointers, type determination
5. [Value types](docs/05-value-types.md): the 13 DTCG types, strict validation, value extensions,
   typed values
6. [Resolvers and modifiers](docs/06-resolvers-and-modifiers.md): sets, contexts, selections
7. [Document loading](docs/07-document-loading.md): the catalog, resource ids, lazy loading
8. [Typed projection](docs/08-typed-projection.md): the derive macros and conversions
9. [Incremental resolution](docs/09-incremental-resolution.md): fingerprints and affected tokens
10. [Source inspection](docs/10-source-inspection.md): APIs for editors and tooling
11. [Errors, limits and diagnostics](docs/11-errors-limits-and-diagnostics.md)
12. [DTCG 2025.10 conformance](docs/12-conformance.md): support, extensions, deviations
13. [API reference](docs/13-api-reference.md)
14. [Architecture and contributing](docs/14-architecture-and-contributing.md)
15. [Expressions](docs/15-expressions.md): computed tokens with JavaScript and TypeScript, modules,
    Rust functions, the sandbox (feature `expressions`)

## Value extensions

Validation is strict DTCG 2025.10 by default. Two value shapes beyond the specification are opt-in:
the `dxn.dtcg.blur` type for layer and background blur effects, and typography `lineHeight` given as
a dimension, the form design tools commonly export. A runtime accepts them only when it is created
with them:

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime, RuntimeOptions, ValueExtensions};

fn main() -> Result<(), DtcgError> {
    let json = r#"{ "frost": { "$type": "dxn.dtcg.blur",
                     "$value": { "kind": "background", "radius": { "value": 20, "unit": "px" } } } }"#;

    // Strict by default: `dxn.dtcg.blur` is an unsupported type.
    assert!(DtcgRuntime::from_json("tokens.json", json)?.resolve(&[]).is_err());

    let runtime = DtcgRuntime::from_json_with_options(
        "tokens.json",
        json,
        RuntimeOptions::default().with_extensions(ValueExtensions::all()),
    )?;
    assert!(runtime.resolve(&[])?.is_ready());
    Ok(())
}
```

See [value types](docs/05-value-types.md#strict-values-and-the-value-extensions) and
[conformance](docs/12-conformance.md#strict-by-default-value-extensions-on-request).

## Expressions

DTCG values are literals. With the `expressions` feature and
`RuntimeOptions::with_expressions(ExpressionOptions::default())`, a token can compute its value
from other tokens with a JavaScript or TypeScript expression in its `$extensions`:

```json
{
  "space": {
    "$type": "dimension",
    "base": { "$value": { "value": 4, "unit": "px" } },
    "large": {
      "$value": { "value": 16, "unit": "px" },
      "$extensions": {
        "dxn.dtcg.expressions": {
          "inputs": { "base": "{space.base}" },
          "js": "({ ...base, value: base.value * 4 })"
        }
      }
    }
  }
}
```

The `$value` stays a valid DTCG snapshot, so every other tool still reads the document, and
without the option the extension is opaque metadata. With it, resolution evaluates the expression
in a sandboxed QuickJS engine (no clock, no randomness, frozen built-ins, deterministic step,
memory and stack limits), validates the result against the token's type and keeps the resolved
`$value` as `ResolvedToken::snapshot`. Expressions import ES modules that the host registers or a
token document declares (loaded through the same request/provide protocol), call Rust functions
registered as native modules or with `#[expression_module]`, and use the built-in modules
`dxn.dtcg` (dimensions, durations, rounding) and `dxn.dtcg/color` (conversions between the DTCG
color spaces, mixing, contrast). The color API is also available in Rust on `values::Color`,
without the feature. See [Expressions](docs/15-expressions.md).

## Development

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo package --workspace
```

See [Architecture and contributing](docs/14-architecture-and-contributing.md) for the source layout
and test groups, and [Releasing](docs/releasing.md) for how releases are cut.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT)
at your option.
