# 7. Document loading

`dtcg_runtime` performs no I/O. Documents enter the runtime as strings, under logical names, and
the runtime tells you which ones it still needs. This chapter covers the catalog, resource ids,
the request/provide protocol, loader patterns, caching, path safety and Token Editor envelopes.

- [The catalog](#the-catalog)
- [Resource ids](#resource-ids)
- [The request/provide protocol](#the-requestprovide-protocol)
- [Loader patterns](#loader-patterns)
- [Catalog API](#catalog-api)
- [Caching and sharing](#caching-and-sharing)
- [Path safety](#path-safety)
- [Token Editor envelopes](#token-editor-envelopes)
- [`LocalReferencePolicy`](#localreferencepolicy)

## The catalog

Each `DtcgRuntime` owns a catalog that maps `ResourceId`s to document text (`Arc<str>`), plus a
revision counter and the id of the root document.

- **On insert**, the runtime checks that the id is a valid resource id and that the text is
  well-formed JSON. Nothing else is checked yet. A syntactically valid but wrong DTCG document is
  accepted and reported when a resolution reaches it.
- **On resolve**, the documents involved are parsed and processed from their text. Nothing parsed
  is kept between calls.
- **The root** is the document that resolution starts from. `from_json` and `from_envelope` set it;
  otherwise call `set_root`. The root does not have to be in the catalog yet; if it is not, the first
  `resolve` requests it.

`DocumentCatalog` is a public type, but a runtime's catalog is reached only through `DtcgRuntime`'s
methods. On its own, `DocumentCatalog` offers just `new`, `len` and `is_empty`.

## Resource ids

A `ResourceId` is a logical document name. It is normalized when parsed, so the same document
always has the same id.

| Rule | Example |
|---|---|
| Relative, with `/` separators | `tokens/base.json` |
| Must not start with `/` or contain `\` | `/abs.json` and `tokens\base.json` are rejected |
| No `:` in any segment, so no drive letters, URI schemes or NTFS streams | `C:/x.json`, `c:x.json`, `a/C:/b.json`, `https://example.com/a.json` and `a.json:hidden` are rejected |
| No empty or `.` segments | `a//b.json`, `./a.json` and `dir/` are rejected |
| `..` removes the previous segment, but cannot go above the root | `themes/../base.json` becomes `base.json`; `../base.json` is rejected |
| A `#fragment` is not part of the id | `library.json#/palette` parses as `library.json` |
| Case-sensitive, any other characters allowed | `Tokens.json` and `tokens.json` are different documents |

References are resolved against the **directory** of the document that contains them, the way
relative URLs work:

```rust
use dtcg_runtime::{DtcgError, ResourceId};

fn main() -> Result<(), DtcgError> {
    let root = ResourceId::parse("themes/base/root.json")?;

    let reference = root.resolve("../shared/palette.json#/colors/primary")?;
    assert_eq!(reference.resource_id().as_str(), "themes/shared/palette.json");
    assert_eq!(reference.fragment(), Some("/colors/primary"));
    assert_eq!(reference.original(), "../shared/palette.json#/colors/primary");

    // A fragment-only reference stays in the same document.
    assert_eq!(root.resolve("#/tokens/button")?.resource_id(), &root);

    // Escaping the root, absolute paths, drive letters and malformed pointers are rejected.
    for invalid in [
        "",
        "/absolute.json",
        "C:/x.json",
        "../../../escape.json",
        "#not-a-pointer",
        "#/a/~2",
    ] {
        assert!(root.resolve(invalid).is_err(), "{invalid}");
    }
    Ok(())
}
```

A fragment must be empty or start with `/`, may contain only valid `~0`/`~1` escapes, and a
reference may contain at most one `#`. The `:` rule applies to the path only, so `#/colors/a:b` is
a valid fragment. URLs such as `https://cdn.example.com/a.json` are not valid references: the path
contains `:`, so the reference fails with `InvalidResourceId` and no document is requested. See
[path safety](#path-safety).

## The request/provide protocol

Methods that need documents return `Result<Resolution<T>, DtcgError>`:

```rust,ignore
pub enum Resolution<T> {
    Ready(T),
    Missing(Vec<DocumentRequest>),
}

pub struct DocumentRequest {
    pub resource_id: ResourceId,     // the normalized id to provide
    pub referenced_from: ResourceId, // the document that contains the reference
    pub original_reference: Arc<str>,// the reference as written, fragment included
}
```

`Resolution::ready()` turns `Ready(value)` into `Some(value)` and `Missing(_)` into `None`.

These methods can return `Missing`: `resolve`, `project`, `modifiers`, `normalize_selection`,
`validate_complete`, `replace_json_and_resolve_affected`, `source_references`, `inspect_source`,
`validate_source`, `preview_source` and `preview_occurrence`.

What a `Missing` response guarantees:

- **Complete for the pass.** It lists every missing document the runtime could reach without
  the missing documents' own contents.
- **De-duplicated and sorted** by resource id, so the same catalog state always yields the same
  requests.
- **Scoped to the active graph.** `resolve` requests only the documents of the selected contexts.
  `validate_complete`, `validate_source` and `source_references` request the documents of every
  context.
- **Possibly incremental.** A missing document may be a nested resolver whose own references only
  become visible once it is provided. Keep looping until `Ready`.
- **Root requests.** When the root itself is missing, the request has `referenced_from` and
  `original_reference` equal to the root's id.

The loop always makes progress, because each round provides at least one document that was not in
the catalog, and `RuntimeLimits::max_documents` bounds the total.

`provide_requested_json(&request, json)` is equivalent to
`provide_json(request.resource_id.as_str(), json)`, minus the re-parse of the id.

## Loader patterns

### Everything up front

When all documents are known in advance, for example embedded in the binary, provide them in one
batch. `include_str!` is a good fit for applications and WebAssembly builds that ship a fixed
design system:

```rust,ignore
use dtcg_runtime::{DtcgError, DtcgRuntime};

fn embedded_runtime() -> Result<DtcgRuntime, DtcgError> {
    let mut runtime = DtcgRuntime::from_json(
        "acme.resolver.json",
        include_str!("../design/acme.resolver.json"),
    )?;
    runtime.provide_json_batch([
        ("tokens/foundation.json", include_str!("../design/tokens/foundation.json")),
        ("tokens/semantic.json", include_str!("../design/tokens/semantic.json")),
        ("tokens/dark.json", include_str!("../design/tokens/dark.json")),
    ])?;
    Ok(runtime)
}
```

### File system

A synchronous loader that keeps reads inside one directory (see [path safety](#path-safety)):

```rust,no_run
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

use dtcg_runtime::{DocumentRequest, DtcgRuntime, LocalReferencePolicy, Resolution, ResolvedTokens};

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

The runtime has no async API because it does not need one: resolution is CPU-only and fast. Run the
loop in your async code and await the fetches between calls. Fetching all requests of a round
concurrently (for example with `futures::future::try_join_all`) saves round trips.

```rust,no_run
use std::error::Error;

use dtcg_runtime::{DtcgRuntime, Resolution, ResolvedTokens};

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

In a browser (`wasm32-unknown-unknown`), the same loop works with `fetch` through `wasm-bindgen`.

### Preloading every context

To fetch everything any selection could need, for an offline cache or a bundle, drive the loop with
`validate_complete` instead of `resolve`:

```rust
use std::collections::HashMap;

use dtcg_runtime::{DtcgError, DtcgRuntime, Resolution};

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
| `DtcgRuntime::default()` / `with_limits(limits)` | An empty runtime with no root | 0 |
| `DtcgRuntime::from_json(id, json)` | An empty runtime, then `provide_json(id, json)` and `set_root(id)` | 1 |
| `DtcgRuntime::from_envelope(id, json)` | See [Token Editor envelopes](#token-editor-envelopes) | 1 |
| `set_root(id)` | Names the root document. It need not be provided yet. | unchanged |
| `provide_json(id, json)` | Inserts the document, replacing any previous text for `id` | +1 |
| `provide_requested_json(&request, json)` | `provide_json` keyed by `request.resource_id` | +1 |
| `provide_json_batch(iter)` | Inserts several documents, **all or nothing**. A repeated id in the batch fails with `DuplicateDocument`. | +1 for the batch |
| `replace_json(id, json)` | Like `provide_json`, but the document must already exist (`UnknownDocument` otherwise) | +1 |
| `replace_json_and_resolve_affected(…)` | See [Incremental resolution](09-incremental-resolution.md) | +1, or +2 when it rolls back |
| `remove_document(id)` | Removes a document, which must exist (`UnknownDocument` otherwise) | +1 |
| `contains_document(id)` | `Ok(true)` if present. `Err` only for an invalid id. | unchanged |
| `catalog_revision()` | The current revision (`u64`) | n/a |

**Atomicity.** A mutation that fails, because of an invalid id, malformed JSON, a duplicate in a
batch or an unknown document, changes nothing: neither the documents nor the revision. The revision
increases on every successful mutation, even if the new text is identical to the old.

## Caching and sharing

`resolve` recomputes from the catalog every time. That keeps the runtime simple and the results
consistent, but in a UI you will want to cache. The catalog revision and a normalized selection
together identify a resolution exactly:

```rust
use std::{collections::HashMap, sync::Arc};

use dtcg_runtime::{DtcgError, DtcgRuntime, Resolution, ResolvedTokens};

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

To skip downstream work such as re-rendering, compare `ResolvedTokens::fingerprint()` values; see
[fingerprints](09-incremental-resolution.md#fingerprints).

For sharing across threads: `DtcgRuntime` is `Send + Sync` and every reading method takes `&self`,
so an `Arc<RwLock<DtcgRuntime>>` allows concurrent resolution with exclusive updates. Cloning a
runtime is cheap, since documents are reference-counted, so a copy-on-write approach (clone,
modify, swap) works too.

## Path safety

Resource ids come from documents, which may not be trustworthy. The id rules prevent the usual
escapes: no leading `/`, no `\`, no `:` in any segment, and no `..` above the root. Without `:`,
no id can carry a Windows drive prefix, either absolute (`C:/Windows/win.ini`) or drive-relative
(`c:win.ini`), even after `..` is applied (`a/../C:/x.json` is rejected too). Without `/` and `\`
at the start, no id can be a UNC path. Joining an id onto a base directory with `Path::join`
therefore stays inside it on Windows as well as on Unix-like systems, symbolic links aside. A
reference that breaks these rules fails with `InvalidResourceId` and never becomes a
`DocumentRequest`.

A loader that reads files should still check each request:

1. Call `LocalReferencePolicy::check(&request.referenced_from, &request.original_reference)`. It
   rejects absolute paths, backslashes and anything that starts like a URI scheme, which covers
   `http:`, `file:` and drive letters such as `C:`. The runtime does not produce such requests, so
   this is a second line of defense. It also keeps the loader safe if it is ever given requests
   built some other way.
2. To also refuse symbolic links that lead elsewhere, canonicalize the joined path and check that it
   is still under the canonical base directory, as the [file-system loader](#file-system) does.

`RuntimeLimits` bounds the work a hostile document set can cause; see
[runtime limits](11-errors-limits-and-diagnostics.md#runtime-limits).

## Token Editor envelopes

The RayX Token Editor saves projects as `.tokeneditor` files: a JSON envelope whose `data` member is
a 2025.10 resolver. `DtcgRuntime::from_envelope` unwraps the envelope, checks it, and makes the
resolver the root, **under the envelope's own resource id**, so relative references resolve next to
the project file.

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime, Resolution};

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

`ResolverEnvelope::parse` (and so `from_envelope`) requires:

- an object `data` member;
- `data.version` equal to `"2025.10"`;
- every `$ref` in `data.sets.*.sources` and `data.modifiers.*.contexts.*` to pass
  `LocalReferencePolicy::check`.

Failures are `DtcgError::InvalidEnvelope` or `DtcgError::InvalidReference`. `envelope.name()`
returns `data.name`, and `envelope.resolver_json()` the resolver as a JSON string.

## `LocalReferencePolicy`

The relative-local reference rule shared by RayX themes and the Token Editor.

- `LocalReferencePolicy::check(resource, reference)` validates one authored reference. It rejects:
  an empty reference; a path that starts with `/`; a path containing `\`; and a path that starts
  with a URI scheme (a letter followed by letters, digits, `+`, `-` or `.`, then `:`). A
  fragment-only reference such as `#/sets/base` always passes. The error is
  `DtcgError::InvalidReference`.
- `LocalReferencePolicy::check_resolver(resource, &resolver_value)` applies `check` to every `$ref`
  in a resolver's `sets.*.sources` and `modifiers.*.contexts.*` arrays. Inline steps in
  `resolutionOrder` and nested resolvers are not walked; check those when you load them.

`from_json` and `resolve` do not apply the policy, since many callers load from sources where it
does not matter. Apply it in your loader, or use `from_envelope`.
