# 7. Document loading

`dtcg_runtime` performs no I/O. Documents enter the runtime as strings, under logical names, and
the runtime tells you which ones it still needs. This chapter covers the catalog, resource ids and
fragments, the request/provide protocol, loader patterns, limits, caching, path safety and Token
Editor envelopes.

- [The catalog](#the-catalog)
- [Resource ids](#resource-ids)
- [Fragments](#fragments)
- [The request/provide protocol](#the-requestprovide-protocol)
- [Loader patterns](#loader-patterns)
- [Catalog API](#catalog-api)
- [Limits](#limits)
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

A `ResourceId` is a logical, portable document name. It is normalized when parsed, so the same
document always has the same id, and every valid id is an ordinary relative file name on Windows as
well as on Unix-like systems.

| Rule | Example |
|---|---|
| Relative, with `/` separators | `tokens/base.json` |
| Must not start with `/` or contain `\` | `/abs.json` and `tokens\base.json` are rejected |
| No `:` in any segment, so no drive letters, URI schemes or NTFS streams | `C:/x.json`, `c:x.json`, `a/C:/b.json`, `https://example.com/a.json` and `a.json:hidden` are rejected |
| No `#`: an id names a whole document | `library.json#/palette` is rejected |
| No control characters and none of `<` `>` `"` `\|` `?` `*` | `what?.json` and `star*.json` are rejected |
| No empty or `.` segments, and no segment that ends with `.` or a space | `a//b.json`, `./a.json`, `dir/`, `dir./a.json` and `"dir /a.json"` are rejected |
| No Windows device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`) in any case, with or without an extension | `con.json` and `themes/NUL.json` are rejected; `console.json` and `com10.json` are fine |
| `..` removes the previous segment, but cannot go above the root | `themes/../base.json` becomes `base.json`; `../base.json` is rejected |
| Case-sensitive; other characters, Unicode included, are allowed | `Tokens.json` and `tokens.json` are different documents; `ünïcödé/файл.json` is valid |

Every method that names a document by id applies these rules and fails with `InvalidResourceId`:
`ResourceId::parse`, `from_json`, `set_root`, `provide_json`, `replace_json`, `remove_document` and
`contains_document`.

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

    // An id never carries a fragment, and must be portable.
    for invalid in ["library.json#/palette", "con.json", "what?.json", "dir./a.json"] {
        assert!(ResourceId::parse(invalid).is_err(), "{invalid}");
    }

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

`ResourceId::resolve` keeps a reference's fragment separately; see [fragments](#fragments). A
reference may contain at most one `#`, and the id rules apply to its path only, so `#/colors/a:b`
is a valid fragment. URLs such as `https://cdn.example.com/a.json` are not valid references: the
path contains `:`, so the reference fails with `InvalidResourceId` and no document is requested.
See [path safety](#path-safety).

## Fragments

A fragment selects part of a document: `library.json#/palette` makes the `palette` member the
source root. Fragments are JSON Pointers in URI-fragment form (RFC 6901 §6). The runtime uses one
parser for them, for value-level `{ "$ref": … }` objects and for token-level `$ref`s (see
[Aliases and references](04-aliases-and-references.md)):

1. `%XX` escapes are decoded first and must form valid UTF-8, so `#/brand%20colors` addresses the
   member `brand colors`. A literal `#/brand colors` works too.
2. The decoded text must be empty (the whole document) or start with `/`. It is split on `/`, and
   `~1` and `~0` are unescaped to `/` and `~`. Any other `~` sequence is invalid.
3. An array index is `0` or a decimal number without a leading zero or sign (`01`, `+1` and `-1`
   are invalid), and it must name an existing element.

A malformed fragment fails with `InvalidPointer` when the reference is parsed. A well-formed
fragment that addresses nothing fails with `InvalidPointer` when the document is loaded.
`DocumentReference::fragment()` returns the fragment as written, without the `#`; the
[inspection APIs](10-source-inspection.md) report decoded, re-escaped pointers.

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime, ResourceId};

fn resolver(fragment: &str) -> String {
    format!(
        r##"{{ "version": "2025.10", "resolutionOrder": [
             {{ "type": "set", "name": "lib", "sources": [{{ "$ref": "lib.json#{fragment}" }}] }}
           ] }}"##
    )
}

const LIBRARY: &str = r#"{
  "brand colors": { "primary": { "$type": "number", "$value": 3 } },
  "themes": [{ "accent": { "$type": "number", "$value": 7 } }]
}"#;

fn main() -> Result<(), DtcgError> {
    let root = ResourceId::parse("root.json")?;
    assert_eq!(root.resolve("lib.json#/brand%20colors")?.fragment(), Some("/brand%20colors"));
    for malformed in ["lib.json#/a%2", "lib.json#/a%zz", "lib.json#/a~2", "lib.json#a"] {
        assert!(matches!(root.resolve(malformed), Err(DtcgError::InvalidPointer(_))), "{malformed}");
    }

    let mut runtime = DtcgRuntime::from_json("root.json", resolver("/brand%20colors"))?;
    runtime.provide_json("lib.json", LIBRARY)?;
    assert_eq!(runtime.resolve(&[])?.ready().unwrap().get("primary").unwrap().value, 3);

    runtime.replace_json("root.json", resolver("/themes/0"))?;
    assert_eq!(runtime.resolve(&[])?.ready().unwrap().get("accent").unwrap().value, 7);

    runtime.replace_json("root.json", resolver("/themes/00"))?;
    assert!(matches!(runtime.resolve(&[]), Err(DtcgError::InvalidPointer(_))));
    Ok(())
}
```

## The request/provide protocol

Methods that need documents return `Result<Resolution<T>, DtcgError>`:

```rust,ignore
pub enum Resolution<T> {
    Ready(T),
    Missing(Vec<DocumentRequest>),
}

#[non_exhaustive]
pub struct DocumentRequest {
    pub resource_id: ResourceId,     // the normalized id to provide
    pub referenced_from: ResourceId, // the document that contains the reference
    pub original_reference: Arc<str>,// the reference as written, fragment included
}
```

`Resolution` has a few helpers: `ready()` turns `Ready(value)` into `Some(value)` and `Missing(_)`
into `None`; `map(f)` converts a ready value and passes requests through; `is_ready()` and
`is_missing()` test the variant; and `missing()` borrows the requests, if any.

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
- **Before input errors.** A missing nested resolver may declare modifiers, so selection inputs
  are checked only once every reached document is present.
- **Root requests.** When the root itself is missing, the request has `referenced_from` and
  `original_reference` equal to the root's id.

The loop always makes progress, because each round provides at least one document that was not in
the catalog, and [`RuntimeLimits::max_documents`](#limits) bounds what one discovery can reach.

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
| `DtcgRuntime::default()` / `with_limits(limits)` / `with_options(options)` | An empty runtime with no root; see [limits](#limits) | 0 |
| `DtcgRuntime::from_json(id, json)` / `from_json_with_options(id, json, options)` | An empty runtime, then `provide_json(id, json)` and `set_root(id)` | 1 |
| `DtcgRuntime::from_envelope(id, json)` / `from_envelope_with_options(id, json, options)` | See [Token Editor envelopes](#token-editor-envelopes) | 1 |
| `replace_envelope(id, json)` | Replaces the resolver stored under `id` with the one in a new envelope | +1 |
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
batch, an unknown document or an invalid envelope, changes nothing: neither the documents nor the
revision. The revision increases on every successful mutation, even if the new text is identical to
the old.

`limits()` and `options()` return what the runtime was created with. `Debug` for `DtcgRuntime`
prints the root, the options, the revision and the document ids, never document text.

## Limits

`RuntimeLimits` bounds the work one call can do, so a hostile or accidental document set cannot
exhaust memory or time. It is `#[non_exhaustive]`: start from `RuntimeLimits::default()` and change
limits with the `with_*` setters. Pass it to `DtcgRuntime::with_limits`, or combine it with
[value extensions](05-value-types.md) in `RuntimeOptions` for `with_options`.

| Limit | Default | What it bounds |
|---|---|---|
| `max_documents` | 256 | Per discovery: the source occurrences collected (loaded and inline sources, and missing-document requests), and separately the expansion steps (every set reference, local reference, document load and nested resolver, even one that contributes no source) |
| `max_reference_depth` | 64 | Nesting of resolver and source references |
| `max_alias_depth` | 64 | Reference hops in one chain: aliases, token pointers and document pointers |
| `max_value_nodes` | 4,000,000 | JSON nodes copied into resolved values through references |
| `max_tokens` | 100,000 | Flattened tokens, `$extends` copies included, and separately the JSON nodes `$extends` copies |

Because expansion steps count, a small resolver whose sets reference each other many times fails
with `LimitExceeded { kind: "documents", .. }` even when every set is empty. Inspection and preview
calls apply the same limits. The value and token limits, and the fixed depth bounds, are covered in
[runtime limits](11-errors-limits-and-diagnostics.md#runtime-limits).

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime, RuntimeLimits, RuntimeOptions, ValueExtensions};

fn main() -> Result<(), DtcgError> {
    let mut runtime = DtcgRuntime::with_limits(RuntimeLimits::default().with_max_documents(4));
    assert_eq!(runtime.limits().max_documents, 4);
    runtime.provide_json(
        "r.json",
        r##"{
          "version": "2025.10",
          "sets": { "empty": { "sources": [] } },
          "resolutionOrder": [{ "type": "set", "name": "fan", "sources": [
            { "$ref": "#/sets/empty" }, { "$ref": "#/sets/empty" }, { "$ref": "#/sets/empty" },
            { "$ref": "#/sets/empty" }, { "$ref": "#/sets/empty" }
          ] }]
        }"##,
    )?;
    runtime.set_root("r.json")?;
    // The resolver and five set references are six expansion steps.
    assert!(matches!(
        runtime.resolve(&[]),
        Err(DtcgError::LimitExceeded { kind: "documents", limit: 4 })
    ));

    // Limits and value extensions together.
    let options = RuntimeOptions::default()
        .with_limits(RuntimeLimits::default().with_max_alias_depth(16))
        .with_extensions(ValueExtensions::rayx());
    let runtime = DtcgRuntime::with_options(options);
    assert_eq!(runtime.limits().max_alias_depth, 16);
    assert!(runtime.options().extensions.rayx_blur);
    Ok(())
}
```

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
escapes: no leading `/`, no `\`, no `:` in any segment, and no `..` above the root. Ids also avoid
names Windows treats specially (device names such as `NUL`, reserved characters, trailing dots and
spaces), so an id always maps to an ordinary file. Without `:`,
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

`RuntimeLimits` bounds the work a hostile document set can cause; see [limits](#limits).

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
  `LocalReferencePolicy::check`;
- `data` to be a valid resolver, with the same
  [structure rules](06-resolvers-and-modifiers.md#resolver-structure) as any other: only allowed
  members, a non-empty `resolutionOrder` and at least two contexts per modifier.

Failures are `DtcgError::InvalidEnvelope` (no object `data`, wrong version),
`DtcgError::InvalidReference` (reference policy) or `DtcgError::InvalidDocument` (resolver
structure). `envelope.name()` returns `data.name`, and `envelope.resolver_json()` the resolver as a
JSON string.

When the project file changes, `replace_envelope(id, json)` parses the new envelope with the same
checks and replaces the resolver stored under `id`, which must already be in the catalog
(`UnknownDocument` otherwise). Later resolutions use the new resolver; an invalid envelope leaves
the catalog unchanged.

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime};

fn envelope(gap: u32) -> String {
    format!(
        r#"{{ "data": {{
          "version": "2025.10",
          "name": "Acme",
          "resolutionOrder": [{{ "type": "set", "name": "base", "sources": [
            {{ "gap": {{ "$type": "number", "$value": {gap} }} }}
          ] }}]
        }} }}"#
    )
}

fn main() -> Result<(), DtcgError> {
    let (mut runtime, _) = DtcgRuntime::from_envelope("acme.tokeneditor", &envelope(8))?;
    runtime.replace_envelope("acme.tokeneditor", &envelope(12))?;
    assert_eq!(runtime.resolve(&[])?.ready().unwrap().get("gap").unwrap().value, 12);

    // The resolver inside an envelope follows the resolver structure rules.
    let invalid = r#"{ "data": { "version": "2025.10", "resolutionOrder": [] } }"#;
    assert!(matches!(
        runtime.replace_envelope("acme.tokeneditor", invalid),
        Err(DtcgError::InvalidDocument { .. })
    ));
    assert_eq!(runtime.resolve(&[])?.ready().unwrap().get("gap").unwrap().value, 12);
    Ok(())
}
```

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
