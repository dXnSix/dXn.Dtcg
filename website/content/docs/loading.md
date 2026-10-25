---
title: Loading documents
description: The catalog, resource ids and fragments, the request/provide protocol, loader patterns for disk, HTTP and embedded assets, caching, path safety and Token Editor envelopes.
group: Concepts
order: 90
---

`dxn_dtcg` performs no I/O. Documents enter the runtime as strings under logical names, and the runtime tells you which ones it still needs. This page covers the catalog, resource ids, the protocol, loader patterns and path safety.

## The catalog

Each `DtcgRuntime` owns a catalog mapping `ResourceId`s to document text (`Arc<str>`), a revision counter and the id of the root document.

- **On insert** the runtime checks that the id is valid and the text is well-formed JSON, nothing more. A valid JSON document that is wrong DTCG is accepted and reported when a resolution reaches it.
- **On resolve** the documents involved are parsed from their text; nothing parsed is kept between calls.
- **The root** is where resolution starts. `from_json` and `from_envelope` set it; otherwise call `set_root`. The root need not be in the catalog yet; if it is not, the first `resolve` requests it.

## Resource ids

A `ResourceId` is a logical, portable, normalized document name such as `themes/dark.json`. Every valid id is an ordinary relative file name on Windows and Unix alike.

| Rule | Example |
|---|---|
| Relative, `/` separators | `tokens/base.json` |
| No leading `/`, no `\` | `/abs.json`, `tokens\base.json` rejected |
| No `:` in any segment (drive letters, URI schemes, NTFS streams) | `C:/x.json`, `https://example.com/a.json`, `a.json:hidden` rejected |
| No `#`: an id names a whole document | `library.json#/palette` rejected |
| No control characters and none of `<` `>` `"` `\|` `?` `*` | `what?.json` rejected |
| No empty or `.` segments; no segment ending in `.` or a space | `a//b.json`, `./a.json`, `dir./a.json` rejected |
| No Windows device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1`-`COM9`, `LPT1`-`LPT9`), any case, with or without extension | `con.json` rejected; `console.json` fine |
| `..` removes the previous segment but cannot climb above the root | `themes/../base.json` becomes `base.json`; `../base.json` rejected |
| Case-sensitive; Unicode allowed | `Tokens.json` and `tokens.json` differ |

Invalid ids fail with `InvalidResourceId`. References resolve against the **directory** of the document that contains them, like relative URLs:

```rust
use dxn_dtcg::{DtcgError, ResourceId};

fn main() -> Result<(), DtcgError> {
    let root = ResourceId::parse("themes/base/root.json")?;

    let reference = root.resolve("../shared/palette.json#/colors/primary")?;
    assert_eq!(reference.resource_id().as_str(), "themes/shared/palette.json");
    assert_eq!(reference.fragment(), Some("/colors/primary"));
    assert_eq!(reference.original(), "../shared/palette.json#/colors/primary");

    // A fragment-only reference stays in the same document.
    assert_eq!(root.resolve("#/tokens/button")?.resource_id(), &root);

    // An id never carries a fragment, and must be portable.
    for invalid in ["library.json#/palette", "con.json", "what?.json", "dir./a.json"] {
        assert!(ResourceId::parse(invalid).is_err(), "{invalid}");
    }

    // Escaping the root, absolute paths and drive letters are rejected.
    for invalid in ["", "/absolute.json", "C:/x.json", "../../../escape.json"] {
        assert!(root.resolve(invalid).is_err(), "{invalid}");
    }
    Ok(())
}
```

URLs are not valid references: the path contains `:`, so the reference fails with `InvalidResourceId` and no document is requested. Map remote documents onto relative ids and fetch them yourself.

### Fragments

A fragment selects part of a document: `library.json#/palette` makes the `palette` member the source root. Fragments are JSON Pointers in URI-fragment form, parsed by the same rules as value-level `$ref`s ([pointer syntax](./aliases.md#pointer-syntax)). A malformed fragment fails with `InvalidPointer` when the reference is parsed; a well-formed one that addresses nothing fails when the document is loaded.

## The request/provide protocol

```rust
// As defined by dxn_dtcg (simplified).
pub enum Resolution<T> {
    Ready(T),
    Missing(Vec<DocumentRequest>),
}

pub struct DocumentRequest {
    pub resource_id: ResourceId,      // the normalized id to provide
    pub referenced_from: ResourceId,  // the document that contains the reference
    pub original_reference: Arc<str>, // the reference as written, fragment included
}
```

`Resolution` has `ready()` (`Some` for `Ready`), `map(f)`, `is_ready()`, `is_missing()` and `missing()` (borrows the requests). These methods can return `Missing`: `resolve`, `project`, `modifiers`, `normalize_selection`, `validate_complete`, `replace_json_and_resolve_affected`, `source_references`, `inspect_source`, `validate_source`, `preview_source` and `preview_occurrence`.

A `Missing` response is:

- **Complete for the pass**: every missing document reachable without the missing documents' own contents.
- **De-duplicated and sorted** by resource id, so the same catalog gives the same requests.
- **Scoped to the active graph**: `resolve` requests only the selected contexts' documents; `validate_complete`, `validate_source` and `source_references` request every context's.
- **Possibly incremental**: a missing nested resolver reveals more references once provided. Loop until `Ready`.
- **Before input errors**: a missing nested resolver may declare modifiers, so selection inputs are checked only once every reached document is present.

When the root itself is missing, the request has `referenced_from` and `original_reference` equal to the root's id. `provide_requested_json(&request, json)` is `provide_json(request.resource_id.as_str(), json)` without re-parsing the id.

:::tip Why the loop terminates
Each round provides at least one document that was not in the catalog, and `RuntimeLimits::max_documents` bounds what one discovery can reach.
:::

## Loader patterns

### Everything up front

When all documents are known in advance, such as a design system embedded in a binary or a WebAssembly build, provide them in one batch. The batch is all-or-nothing and bumps the revision once.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

fn embedded_runtime() -> Result<DtcgRuntime, DtcgError> {
    let mut runtime = DtcgRuntime::from_json(
        "acme.resolver.json",
        include_str!("../design/acme.resolver.json"),
    )?;
    runtime.provide_json_batch([
        ("tokens/foundation.json", include_str!("../design/tokens/foundation.json")),
        ("tokens/dark.json", include_str!("../design/tokens/dark.json")),
    ])?;
    Ok(runtime)
}

fn main() -> Result<(), DtcgError> {
    let runtime = embedded_runtime()?;
    assert!(runtime.resolve(&[])?.is_ready());
    Ok(())
}
```

### File system

A synchronous loader that keeps reads inside one directory. It touches the file system, so it needs a `design` directory with the documents:

```rust
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

use dxn_dtcg::{DocumentRequest, DtcgRuntime, LocalReferencePolicy, Resolution, ResolvedTokens};

struct FileLoader {
    base: PathBuf,
}

impl FileLoader {
    fn new(base: impl AsRef<Path>) -> std::io::Result<Self> {
        Ok(Self { base: fs::canonicalize(base)? })
    }

    fn read(&self, request: &DocumentRequest) -> Result<String, Box<dyn Error>> {
        // Reject absolute paths, drive letters, URI schemes and backslashes.
        LocalReferencePolicy::check(&request.referenced_from, &request.original_reference)?;
        let path = fs::canonicalize(self.base.join(request.resource_id.as_str()))?;
        // Also refuse symbolic links that lead out of the token directory.
        if !path.starts_with(&self.base) {
            return Err(format!("{} is outside the token directory", request.resource_id).into());
        }
        Ok(fs::read_to_string(path)?)
    }

    fn resolve(
        &self,
        runtime: &mut DtcgRuntime,
        selection: &[(&str, &str)],
    ) -> Result<ResolvedTokens, Box<dyn Error>> {
        loop {
            match runtime.resolve(selection)? {
                Resolution::Ready(tokens) => return Ok(tokens),
                Resolution::Missing(requests) => {
                    for request in &requests {
                        runtime.provide_requested_json(request, self.read(request)?)?;
                    }
                }
            }
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let loader = FileLoader::new("design")?;
    let mut runtime = DtcgRuntime::default();
    runtime.set_root("acme.resolver.json")?;
    let tokens = loader.resolve(&mut runtime, &[("theme", "dark")])?;
    println!("{} tokens", tokens.len());
    Ok(())
}
```

### HTTP and other asynchronous sources

The runtime has no async API because resolution is CPU-only and fast. Run the loop in your async code and await the fetches between calls. Fetching all requests of a round concurrently saves round trips. In a browser (`wasm32-unknown-unknown`) the same loop works with `fetch` through `wasm-bindgen`. This snippet touches the network once you replace the stub:

```rust
use std::error::Error;

use dxn_dtcg::{DtcgRuntime, Resolution, ResolvedTokens};

async fn fetch_text(url: &str) -> Result<String, Box<dyn Error>> {
    // Use your HTTP client here, for example `reqwest::get(url).await?.text().await?`.
    unimplemented!("GET {url}")
}

async fn resolve_remote(
    base_url: &str,
    selection: &[(&str, &str)],
) -> Result<ResolvedTokens, Box<dyn Error>> {
    let mut runtime = DtcgRuntime::default();
    runtime.set_root("acme.resolver.json")?;
    loop {
        match runtime.resolve(selection)? {
            Resolution::Ready(tokens) => return Ok(tokens),
            Resolution::Missing(requests) => {
                for request in requests {
                    let text = fetch_text(&format!("{base_url}/{}", request.resource_id)).await?;
                    runtime.provide_requested_json(&request, text)?;
                }
            }
        }
    }
}

fn main() {
    let _ = resolve_remote("https://design.example.com/tokens", &[]);
}
```

### Preloading every context

To fetch everything any selection could need, for an offline cache or a bundle, drive the loop with `validate_complete` instead of `resolve`:

```rust
use std::collections::HashMap;

use dxn_dtcg::{DtcgError, DtcgRuntime, Resolution};

fn preload_all(runtime: &mut DtcgRuntime, files: &HashMap<&str, &str>) -> Result<(), DtcgError> {
    loop {
        match runtime.validate_complete()? {
            Resolution::Ready(()) => return Ok(()),
            Resolution::Missing(requests) => {
                for request in requests {
                    runtime.provide_requested_json(&request, files[request.resource_id.as_str()])?;
                }
            }
        }
    }
}

fn main() -> Result<(), DtcgError> {
    let files = HashMap::from([
        ("light.json", r#"{ "bg": { "$type": "number", "$value": 0 } }"#),
        ("dark.json", r#"{ "bg": { "$type": "number", "$value": 1 } }"#),
    ]);
    let mut runtime = DtcgRuntime::from_json(
        "r.json",
        r##"{
          "version": "2025.10",
          "modifiers": { "theme": { "default": "light", "contexts": {
            "light": [{ "$ref": "light.json" }], "dark": [{ "$ref": "dark.json" }]
          } } },
          "resolutionOrder": [{ "$ref": "#/modifiers/theme" }]
        }"##,
    )?;
    preload_all(&mut runtime, &files)?;
    assert!(runtime.contains_document("dark.json")?); // fetched although "light" is the default
    Ok(())
}
```

## Catalog API

| Method | Effect | Revision |
|---|---|---|
| `DtcgRuntime::default()` / `with_limits(limits)` / `with_options(options)` | An empty runtime with no root | 0 |
| `from_json(id, json)` / `from_json_with_options(..)` | Empty runtime, then `provide_json` and `set_root` | 1 |
| `from_envelope(id, json)` / `from_envelope_with_options(..)` | See [envelopes](#token-editor-envelopes) | 1 |
| `set_root(id)` | Names the root; need not be provided yet | unchanged |
| `provide_json(id, json)` | Inserts the document, replacing any previous text | +1 |
| `provide_requested_json(&request, json)` | `provide_json` keyed by `request.resource_id` | +1 |
| `provide_json_batch(iter)` | Inserts several documents, all or nothing; a repeated id is `DuplicateDocument` | +1 |
| `replace_json(id, json)` | Like `provide_json`, but the document must exist (`UnknownDocument`) | +1 |
| `replace_envelope(id, json)` | Replaces the resolver stored under `id` with the one in a new envelope | +1 |
| `replace_json_and_resolve_affected(..)` | See [Incremental resolution](./incremental.md) | +1, or +2 on rollback |
| `remove_document(id)` | Removes a document, which must exist | +1 |
| `contains_document(id)` | `Ok(true)` if present; `Err` only for an invalid id | unchanged |
| `catalog_revision()` | The current revision (`u64`) | n/a |

A mutation that fails (invalid id, malformed JSON, duplicate in a batch, unknown document, invalid envelope) changes nothing, revision included. The revision increases on every successful mutation, even if the new text equals the old. `Debug` for `DtcgRuntime` prints the root, options, revision and document ids, never document text.

## Caching and sharing

`resolve` recomputes from the catalog every time. In a UI, cache by catalog revision plus normalized selection; the [cookbook](./cookbook.md#cache-resolutions-per-theme) has a complete cache. To skip downstream work, compare `ResolvedTokens::fingerprint()` values.

`DtcgRuntime` is `Send + Sync` and every reading method takes `&self`, so an `Arc<RwLock<DtcgRuntime>>` allows concurrent resolution with exclusive updates. Cloning is cheap since documents are reference-counted, so clone, modify and swap works too.

## Path safety

Resource ids come from documents, which may not be trustworthy. The id rules prevent the usual escapes: no leading `/`, no `\`, no `:`, no `..` above the root, and no Windows-special names. Joining an id onto a base directory with `Path::join` therefore stays inside it on Windows and Unix, symbolic links aside. A reference that breaks the rules fails with `InvalidResourceId` and never becomes a `DocumentRequest`.

A loader that reads files should still check each request:

1. `LocalReferencePolicy::check(&request.referenced_from, &request.original_reference)` rejects absolute paths, backslashes and anything starting like a URI scheme. The runtime does not produce such requests, so this is a second line of defense.
2. To also refuse symlinks that lead elsewhere, canonicalize the joined path and check it is under the canonical base directory, as the file loader above does.

`from_json` and `resolve` do not apply `LocalReferencePolicy` themselves; apply it in your loader or use `from_envelope`. `LocalReferencePolicy::check_resolver(resource, &value)` applies `check` to every `$ref` in a resolver's `sets.*.sources` and `modifiers.*.contexts.*`; inline `resolutionOrder` steps and nested resolvers are not walked.

## Token Editor envelopes

The RayX Token Editor saves projects as `.tokeneditor` files: a JSON envelope whose `data` member is a 2025.10 resolver. `DtcgRuntime::from_envelope` unwraps it, checks it and makes the resolver the root **under the envelope's own resource id**, so relative references resolve next to the project file.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime, Resolution};

fn main() -> Result<(), DtcgError> {
    let (runtime, envelope) = DtcgRuntime::from_envelope(
        "projects/acme.tokeneditor",
        r##"{
          "data": {
            "version": "2025.10",
            "name": "Acme",
            "sets": { "base": { "sources": [{ "$ref": "tokens/base.json" }] } },
            "resolutionOrder": [{ "$ref": "#/sets/base" }]
          }
        }"##,
    )?;
    assert_eq!(envelope.name(), Some("Acme"));

    let Resolution::Missing(requests) = runtime.resolve(&[])? else { unreachable!() };
    assert_eq!(requests[0].resource_id.as_str(), "projects/tokens/base.json");
    Ok(())
}
```

`ResolverEnvelope::parse` (and so `from_envelope`) requires an object `data` member, `data.version` equal to `"2025.10"`, every `$ref` in `data.sets.*.sources` and `data.modifiers.*.contexts.*` to pass `LocalReferencePolicy::check`, and `data` to be a valid resolver. Failures are `InvalidEnvelope`, `InvalidReference` or `InvalidDocument`. `replace_envelope(id, json)` re-parses a changed project file with the same checks and swaps the stored resolver; an invalid envelope leaves the catalog unchanged.

Next: [Typed projection](./typed-projection.md), or the [Cookbook](./cookbook.md).
