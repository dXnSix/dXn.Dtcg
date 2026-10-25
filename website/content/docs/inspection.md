---
title: Source inspection
description: APIs for editors and tooling - list reference sites, expand sources with their origins, validate drafts and preview unsaved changes without touching the catalog.
group: Guides
order: 130
---

`resolve` answers "what are the tokens?". Editors, linters and migration tools also need to know where tokens come from, which files reference which, and what an unsaved change would do. The inspection APIs answer those questions with the same discovery rules as `resolve`, and none of them modifies the catalog.

## Concepts

Discovery turns a resolver into an ordered list of **terminal sources**, the token documents that are stacked. Each is a `SourceOccurrence`:

| Method | Returns |
|---|---|
| `resource()` | The document holding the source, which relative references inside it resolve against. A supplied inline source reports the root's id |
| `value()` | The source's authored JSON before merging: empty groups, duplicate definitions and metadata included, reference-object overrides applied |
| `origin()` | Where that JSON lives, as a `SourceOrigin` |
| `member_origins()` | For each top-level member a reference object replaced, the `SourceOrigin` of the object that supplied it |

A `SourceOrigin` is an authored JSON location: `resource()` is `Some(id)` for a catalog document and `None` for the inline source you passed to `inspect_source`; `pointer()` is a JSON Pointer inside it without the leading `#` (`""` is the document root), for example `/sets/base/sources/1`. Origins point at exact locations, so an editor can jump to or rewrite them.

A **dormant** source is in a set or context the current selection does not apply. A **shadowed** occurrence is one whose tokens are all replaced by later sources. `resolve` shows neither; these APIs do.

## Listing reference sites

`source_references()` lists every authored source `$ref` in the resolver graph: in every set (dormant ones included), in every context of every modifier (selected or not), in inline `resolutionOrder` steps, in nested resolvers and in source arrays inside reference objects. It does **not** list token aliases, token-level `$ref`s, `$ref`s inside `$extensions`, unreferenced `$defs` JSON, or the `{"$ref": "#/sets/..."}` steps of `resolutionOrder`.

Each `SourceReference` has an `origin()` pointing at the `$ref` string itself and a `reference()`, the resolved `DocumentReference` (target id, fragment and original text). Local references such as `#/sets/shared` are included. Because referenced documents can be resolvers with references of their own, the call returns `Missing` until every referenced document, dormant ones included, is in the catalog.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime, Resolution};

fn main() -> Result<(), DtcgError> {
    let mut runtime = DtcgRuntime::from_json(
        "themes/root.resolver.json",
        r##"{
          "version": "2025.10",
          "sets": {
            "base": { "sources": [{ "$ref": "base.json" }, { "$ref": "#/sets/shared" }] },
            "shared": { "sources": [{ "$ref": "shared/palette.json#/colors" }] },
            "legacy": {
              "sources": [{ "$ref": "legacy.json" }],
              "$extensions": { "com.example": { "$ref": "not-a-source.json" } }
            }
          },
          "modifiers": { "theme": { "default": "light", "contexts": {
            "light": [], "dark": [{ "$ref": "dark.json" }]
          } } },
          "resolutionOrder": [{ "$ref": "#/sets/base" }, { "$ref": "#/modifiers/theme" }]
        }"##,
    )?;

    // Every referenced document is needed, including the dormant `legacy` set and `dark` context.
    let Resolution::Missing(requests) = runtime.source_references()? else { unreachable!() };
    let ids: Vec<&str> = requests.iter().map(|request| request.resource_id.as_str()).collect();
    assert_eq!(
        ids,
        ["themes/base.json", "themes/dark.json", "themes/legacy.json", "themes/shared/palette.json"]
    );
    for request in &requests {
        runtime.provide_requested_json(request, r#"{ "colors": {} }"#)?;
    }

    let sites = runtime.source_references()?.ready().unwrap();
    let listed: Vec<(&str, &str)> = sites
        .iter()
        .map(|site| (site.origin().pointer(), site.reference().original()))
        .collect();
    assert_eq!(
        listed,
        [
            ("/sets/base/sources/0/$ref", "base.json"),
            ("/sets/base/sources/1/$ref", "#/sets/shared"),
            ("/sets/shared/sources/0/$ref", "shared/palette.json#/colors"),
            ("/sets/legacy/sources/0/$ref", "legacy.json"),
            ("/modifiers/theme/contexts/dark/0/$ref", "dark.json"),
        ]
    );
    Ok(())
}
```

Typical uses:

- **Moving or renaming a file.** Find every site whose `reference().resource_id()` is the old id and rewrite the string at `origin()` with a new relative path.
- **Dependency graphs.** Group sites by `origin().resource()` for document-to-document edges.
- **Unused files.** A file in the token directory that no site targets is not part of the design system.

:::note Limits apply
Repeated walks of a resolver or set reached again under different overrides count toward `max_documents`, so `source_references` can fail with `LimitExceeded { kind: "documents" }` on pathological graphs. Each location's first walk is free, and token-document targets are never walked.
:::

## Expanding a source

`inspect_source(&source, &selection)` takes one source, anything that could appear in a set's `sources` array, and expands it into its terminal occurrences, in order, **in the root resolver's context**: local references resolve against the root document, relative paths against the root's id, and the selection picks contexts in the source and in the root. An input is valid when the supplied source, the root resolver or any resolver the root graph reaches declares it. Token values are not resolved, so a source with invalid values or dangling aliases still inspects fine; structural problems are errors.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime, ResourceId};
use serde_json::json;

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "root.json",
        r##"{
          "version": "2025.10",
          "$defs": { "palette": { "accent": { "$type": "number", "$value": 1 }, "empty": {} } },
          "sets": {
            "brand": { "sources": [
              { "$ref": "#/$defs/palette", "accent": { "$type": "number", "$value": 2 } },
              { "$ref": "#/$defs/palette" }
            ] }
          },
          "resolutionOrder": [{ "$ref": "#/sets/brand" }]
        }"##,
    )?;

    let occurrences = runtime.inspect_source(&json!({ "$ref": "#/sets/brand" }), &[])?.ready().unwrap();
    assert_eq!(occurrences.len(), 2);

    let first = &occurrences[0];
    assert_eq!(first.origin().resource().map(ResourceId::as_str), Some("root.json"));
    assert_eq!(first.origin().pointer(), "/$defs/palette");
    // `accent` was replaced by the reference object at sources/0.
    assert_eq!(first.member_origins()["accent"].pointer(), "/sets/brand/sources/0");
    assert_eq!(first.value()["accent"]["$value"], 2);
    assert_eq!(first.value()["empty"], json!({})); // empty groups are kept

    // In normal resolution the second occurrence shadows the first.
    let resolved = runtime.resolve(&[])?.ready().unwrap();
    assert_eq!(resolved.get("accent").unwrap().value, 1);

    // Inline parts of a supplied source have no resource; pointers are relative to it.
    let inline = runtime
        .inspect_source(&json!({ "$ref": "#/sets/brand", "sources": [{ "x": {} }] }), &[])?
        .ready()
        .unwrap();
    assert_eq!(inline[0].origin().resource(), None);
    assert_eq!(inline[0].origin().pointer(), "/sources/0");
    Ok(())
}
```

## Checking and previewing a draft

`validate_source(&source)` expands a source with **every** context active, like `validate_complete` does for the root. It returns `Missing` for documents any context needs and `Ready(())` once the structure is valid, without resolving token values. Use it while a user edits a set or modifier definition, before saving.

- `preview_source(&source, &selection)` discovers the root's sources for `selection`, merges the occurrences of `source` **last** (as if appended to `resolutionOrder`) and resolves aliases, types and validation as usual. A previewed token inherits its group's `$type` from earlier sources. It accepts inputs that the root graph or the supplied source declares, so a draft resolver can be previewed in any of its own contexts.
- `preview_occurrence(&occurrence, &selection)` does the same with one occurrence from `inspect_source`; it is how you look at a shadowed occurrence on its own. The selection applies to the root graph only, because an occurrence already reflects the selection it was inspected with. Drop inputs that only a draft declares.

Neither touches the catalog or builds a synthetic resolver, so a preview cannot affect normal resolution. A supplied inline source is not a document root, so a `$schema` member in it is an unknown reserved property.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime, Resolution};
use serde_json::json;

fn main() -> Result<(), DtcgError> {
    let mut runtime = DtcgRuntime::from_json(
        "root.json",
        r##"{
          "version": "2025.10",
          "sets": { "base": { "sources": [{ "space": { "$type": "number", "gap": { "$value": 8 } } }] } },
          "resolutionOrder": [{ "$ref": "#/sets/base" }]
        }"##,
    )?;

    // A modifier the user is drafting in an editor, not yet saved anywhere.
    let draft = json!({
        "version": "2025.10",
        "modifiers": { "density": { "default": "compact", "contexts": {
            "comfortable": [],
            "compact": [{ "$ref": "density/compact.json" }],
            "spacious": [{ "$ref": "density/spacious.json" }]
        } } },
        "resolutionOrder": [{ "$ref": "#/modifiers/density" }]
    });

    // Every context's documents are requested, relative to the root. The density files
    // set only the value; `space.gap` keeps the `number` type of the root's group.
    let Resolution::Missing(requests) = runtime.validate_source(&draft)? else { unreachable!() };
    for request in &requests {
        let gap = if request.resource_id.as_str().ends_with("compact.json") { 4 } else { 16 };
        runtime.provide_requested_json(
            request,
            format!(r#"{{ "space": {{ "gap": {{ "$value": {gap} }} }} }}"#),
        )?;
    }
    assert!(runtime.validate_source(&draft)?.ready().is_some());

    let revision = runtime.catalog_revision();
    let preview = runtime.preview_source(&draft, &[])?.ready().unwrap();
    let gap = preview.get("space.gap").unwrap();
    assert_eq!(gap.value, 4); // the draft's default context, merged last
    assert_eq!(gap.token_type.as_deref(), Some("number"));

    // A draft-only modifier can be selected, case-insensitively.
    let spacious = runtime.preview_source(&draft, &[("Density", "SPACIOUS")])?.ready().unwrap();
    assert_eq!(spacious.get("space.gap").unwrap().value, 16);

    // The catalog and the committed resolution are unchanged.
    assert_eq!(runtime.catalog_revision(), revision);
    assert_eq!(runtime.resolve(&[])?.ready().unwrap().get("space.gap").unwrap().value, 8);

    // An occurrence already reflects the selection it was inspected with.
    let occurrences = runtime.inspect_source(&draft, &[("density", "spacious")])?.ready().unwrap();
    let occurrence = runtime.preview_occurrence(&occurrences[0], &[])?.ready().unwrap();
    assert_eq!(occurrence.get("space.gap").unwrap().value, 16);
    Ok(())
}
```

## Resolver definition helpers

Two free functions read resolver definitions with the runtime's own rules, for editors that work on resolver JSON directly. Neither follows references, checks allowed members or resolves anything; `resource` appears only in error messages.

| Function | Checks | Returns |
|---|---|---|
| `set_sources(resource, &definition)` | `definition.sources` is an array | the sources as `&[Value]` |
| `modifier_contexts(resource, &definition)` | an object; `contexts` is an object of at least two arrays; `default`, if present, names a context | the contexts as `&Map<String, Value>` |

```rust
use dxn_dtcg::{ResourceId, modifier_contexts, set_sources};
use serde_json::json;

fn main() {
    let id = ResourceId::parse("acme.resolver.json").unwrap();

    assert_eq!(set_sources(&id, &json!({ "sources": [{ "$ref": "a.json" }] })).unwrap().len(), 1);
    assert!(set_sources(&id, &json!({ "sources": {} })).is_err());

    let theme = json!({ "default": "light", "contexts": { "light": [], "dark": [] } });
    let contexts = modifier_contexts(&id, &theme).unwrap();
    assert_eq!(contexts.keys().collect::<Vec<_>>(), ["light", "dark"]);
    assert!(modifier_contexts(&id, &json!({ "default": "sepia", "contexts": { "light": [], "dark": [] } })).is_err());
    assert!(modifier_contexts(&id, &json!({ "contexts": { "light": [] } })).is_err()); // one context
}
```

## Shared behavior

The inspection APIs use the same discovery, merge, document protocol (`Missing`), limits (`RuntimeLimits`, with preview occurrences counting toward `max_documents`) and `DtcgError`s as `resolve`. Previews also report value errors, since they resolve.

Next: [Errors](./errors.md), or back to the [Cookbook](./cookbook.md).
