---
title: dXn.Dtcg
description: A Rust runtime for Design Tokens Community Group 2025.10 documents - resolvers, themes, aliases, strict validation and typed projection, with no I/O of its own.
eyebrow: Design tokens for Rust
headline: Design tokens in, typed Rust out.
lede: >-
  dxn_dtcg reads DTCG 2025.10 token files and resolver documents, applies the theme the user picked,
  resolves aliases, validates every value against its type and hands you plain Rust structs.
  It does no I/O: it tells you which documents it needs, so the same code runs in a desktop app,
  on a server, in a build script or in WebAssembly.
install: |-
  [dependencies]
  dxn_dtcg = { git = "https://github.com/dXnSix/dXn.Dtcg", tag = "v1.1.0" }
facts:
  - { label: Spec, value: "DTCG 2025.10" }
  - { label: Official fixtures, value: "250 of 250 pass" }
  - { label: Rust, value: "1.97+" }
  - { label: I/O, value: "none, no unsafe" }
  - { label: Targets, value: "native and wasm32" }
  - { label: License, value: "MIT OR Apache-2.0" }
heroTitle: main.rs
heroLang: rust
heroCode: |
  use dxn_dtcg::values::{Color, Dimension};
  use dxn_dtcg::{DtcgRuntime, DtcgTokens, Resolution};

  #[derive(DtcgTokens)]
  #[dtcg(root = "button")]
  struct Button {
      background: Color,
      #[dtcg(rename = "paddingX")]
      padding_x: Dimension,
  }

  fn main() -> Result<(), Box<dyn std::error::Error>> {
      let mut runtime = DtcgRuntime::default();
      runtime.set_root("theme.resolver.json")?;
      let button = loop {
          match runtime.project::<Button>(&[("theme", "dark")])? {
              Resolution::Ready(button) => break button,
              Resolution::Missing(requests) => for request in requests {
                  let json = std::fs::read_to_string(format!("design/{}", request.resource_id))?;
                  runtime.provide_requested_json(&request, json)?;
              },
          }
      };
      println!("{}px", button.padding_x.to_px(16.0));
      Ok(())
  }
features:
  - title: Lazy loading without I/O
    text: resolve returns the tokens or the list of documents it still needs. Only the documents of the selected contexts are requested, from disk, HTTP, an asset bundle or memory.
    link: ./docs/loading.md
  - title: Themes as modifiers
    text: Sets, modifiers with named contexts, nested resolvers and shallow overrides merge into one document, so a dark theme overrides a few palette tokens and every alias follows.
    link: ./docs/resolvers.md
  - title: Typed projection
    text: "#[derive(DtcgTokens)] maps token paths onto struct fields with Option fields, nested structs and your own conversions. Typed models cover all 13 DTCG types."
    link: ./docs/typed-projection.md
  - title: Strict, named errors
    text: Wrong units, dangling aliases, type mismatches and cycles fail with the document and the token path, not as a wrong color on screen.
    link: ./docs/errors.md
  - title: Built for editors
    text: Re-resolve only the tokens an edit changed, inspect where every source came from, preview unsaved changes and list every file reference.
    link: ./docs/incremental.md
  - title: Passes the official suite
    text: All 250 fixtures of the DTCG 2025.10 test suite pass, with bounded work on untrusted input and a SHA-256 fingerprint of every resolution.
    link: ./docs/conformance.md
next:
  - { title: Get started, text: "Install it and resolve your first token.", link: ./docs/getting-started.md }
  - { title: DTCG in five minutes, text: "The format, for developers who have not used it.", link: ./docs/dtcg-primer.md }
  - { title: Which API do I use, text: "Decision tables for every task.", link: ./docs/choosing-an-api.md }
  - { title: Cookbook, text: "Themes, app wiring, caching, CI checks, editor features.", link: ./docs/cookbook.md }
---

## Switch themes by changing a selection

A resolver document lists token sources and declares modifiers. Pick a context and every source for it merges into one document; aliases resolve last, so `surface.background` follows the override.

```rust
let mut runtime = DtcgRuntime::from_json("theme.resolver.json", RESOLVER)?;
runtime.provide_json_batch([("base.tokens.json", BASE), ("dark.tokens.json", DARK)])?;

let light = runtime.resolve(&[])?.ready().expect("all documents provided");
let dark = runtime.resolve(&[("theme", "dark")])?.ready().expect("all documents provided");

assert_ne!(light.fingerprint(), dark.fingerprint());
assert_eq!(dark.get("surface.background").unwrap().value["components"][0], 0.1);
```

See [Resolvers and modifiers](./docs/resolvers.md).

## Load only what the selection needs

The runtime never opens a file. A missing document is a value, not an error: provide it and resolve again. Light mode never reads `dark.json`.

```rust
loop {
    match runtime.resolve(&selection)? {
        Resolution::Ready(tokens) => return Ok(tokens),
        Resolution::Missing(requests) => for request in requests {
            let text = my_loader(request.resource_id.as_str())?; // disk, HTTP, assets, tests
            runtime.provide_requested_json(&request, text)?;
        },
    }
}
```

See [Loading documents](./docs/loading.md) for file system, async HTTP and embedded loaders, resource ids and path safety.

## Get structs, not JSON

Fields read tokens by path. `Option` fields tolerate absent tokens, `nested` reuses one shape at several roots, and `with` converts into types you do not own.

```rust
#[derive(DtcgTokens)]
#[dtcg(root = "card")]
struct Card {
    radius: Dimension,
    #[dtcg(rename = "fadeIn")]
    fade_in: Duration,
    #[dtcg(path = "shadow.raised")]
    shadow: Option<Shadow>,
}
```

See [Typed projection](./docs/typed-projection.md).

## Errors that name the token

Validation runs on the resolved value, so bad data fails at load time with the document and token path.

```text
token `spacing.md` in `tokens.json`: resolved value does not match DTCG type `dimension`
token `heading` in `tokens.json`: reference type mismatch: expected fontWeight, found number
modifier `theme` context `blue`: selected modifier context is unknown; allowed contexts: light, dark
```

See [Errors](./docs/errors.md) and [Troubleshooting](./docs/troubleshooting.md).

## Edit without re-resolving everything

Replace one document and get back only the tokens the edit can have changed, then patch your snapshot. A failed edit rolls the document back.

```rust
let affected = runtime
    .replace_json_and_resolve_affected("tokens.json", edited, &[], &[])?
    .ready()
    .expect("document is loaded");
snapshot.apply_affected(&affected); // equals a full resolve
```

See [Incremental resolution](./docs/incremental.md) and [Source inspection](./docs/inspection.md).
