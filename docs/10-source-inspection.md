# 10. Source inspection

`resolve` answers "what are the tokens?". Editors, linters and migration tools also need to know
where tokens come from, which files reference which, and what an unsaved change would do. The
inspection APIs answer those questions using the same discovery rules as `resolve`, without
modifying the catalog.

- [Concepts](#concepts)
- [Listing reference sites: `source_references`](#listing-reference-sites-source_references)
- [Expanding a source: `inspect_source`](#expanding-a-source-inspect_source)
- [Checking a draft: `validate_source`](#checking-a-draft-validate_source)
- [Previewing: `preview_source` and `preview_occurrence`](#previewing-preview_source-and-preview_occurrence)
- [Resolver definition helpers](#resolver-definition-helpers)
- [Behavior shared with `resolve`](#behavior-shared-with-resolve)

## Concepts

### Authored sources and effective sources

Discovery turns a resolver into an ordered list of **terminal sources**, the token documents that
are flattened and stacked. Each is a `SourceOccurrence`:

| Method | Returns |
|---|---|
| `value()` | The source's authored JSON, before flattening: empty groups, duplicate definitions and metadata included, with the overrides of reference objects applied |
| `origin()` | Where that JSON lives, as a `SourceOrigin` |
| `member_origins()` | For each top-level member that a reference object replaced, the `SourceOrigin` of the object that supplied it. Members not listed belong to `origin()`. |

A `SourceOrigin` is an authored JSON location:

| Method | Returns |
|---|---|
| `resource()` | `Some(id)` for a catalog document, `None` for the inline source you passed to `inspect_source` |
| `pointer()` | A JSON Pointer within that document, without the leading `#` (`""` is the document root), for example `/sets/base/sources/1` |

Origins point at exact locations, so an editor can jump to them or rewrite them.

### Dormant and shadowed sources

- A **dormant** source is in a set or context that the current selection does not apply: an
  unselected context, or a set nothing references.
- A **shadowed** occurrence is one whose tokens are all overridden by later sources.

`resolve` gives no view of either. The APIs below expose both.

## Listing reference sites: `source_references`

`source_references()` lists every authored source `$ref` in the resolver graph, wherever it sits:

- in every set, including dormant ones;
- in every context of every modifier, selected or not;
- in inline `resolutionOrder` steps, in nested resolvers, and in source arrays inside reference
  objects.

It does **not** list token aliases, token-level `$ref`s, `$ref`s inside `$extensions` or other
unknown members, or the `{"$ref": "#/sets/…"}` steps of `resolutionOrder`, which are structure, not
sources.

Each `SourceReference` has an `origin()` that points at the `$ref` string itself, and a
`reference()`, the resolved `DocumentReference` (target id, fragment and the original text). Local
references such as `#/sets/shared` are included; their target is the containing document.

Since a referenced document can be a resolver with references of its own, the call returns
`Missing` until every referenced document, dormant ones included, is in the catalog.

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime, Resolution};

fn main() -> Result<(), DtcgError> {
    let mut runtime = DtcgRuntime::from_json(
        "themes/root.resolver.json",
        r##"{
          "version": "2025.10",
          "sets": {
            "base": { "sources": [{ "$ref": "base.json" }, { "$ref": "#/sets/shared" }] },
            "shared": { "sources": [{ "$ref": "shared/palette.json#/colors" }] },
            "legacy": { "sources": [{ "$ref": "legacy.json" }] }
          },
          "modifiers": { "theme": { "default": "light", "contexts": {
            "light": [], "dark": [{ "$ref": "dark.json" }]
          } } },
          "resolutionOrder": [{ "$ref": "#/sets/base" }, { "$ref": "#/modifiers/theme" }],
          "$extensions": { "com.example": { "$ref": "not-a-source.json" } }
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

- **Moving or renaming a file.** Find every site whose `reference().resource_id()` is the old id,
  and rewrite the string at `origin()` with a new relative path.
- **Dependency graphs.** Group the sites by `origin().resource()` to get document-to-document
  edges.
- **Unused files.** A file in the token directory that no site targets is not part of the design
  system.

## Expanding a source: `inspect_source`

```rust,ignore
pub fn inspect_source(&self, source: &serde_json::Value, selection: &[(&str, &str)])
    -> Result<Resolution<Vec<SourceOccurrence>>, DtcgError>;
```

`inspect_source` takes one source, anything that could appear in a set's `sources` array, and
expands it into its terminal occurrences, in order, **in the root resolver's context**:

- local references (`#/sets/…`, `#/$defs/…`) resolve against the root document;
- relative paths resolve against the root's resource id;
- the selection picks contexts in any resolver the source contains.

Token values are not resolved, so a source with invalid values or dangling aliases still inspects
fine. Structural problems, such as a malformed resolver or a forbidden reference, are errors.

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime, ResourceId};
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

The last call shows a reference object with overrides: its `sources` member replaced the set's
`sources`, so the occurrence comes from the supplied JSON.

## Checking a draft: `validate_source`

`validate_source(source)` expands a source with **every** context active, like `validate_complete`
does for the root. It returns `Missing` for documents any context needs and `Ready(())` once the
structure is valid. Like `validate_complete`, it does not resolve token values. Use it while a user
edits a set or modifier definition, before saving it into the resolver.

## Previewing: `preview_source` and `preview_occurrence`

- `preview_source(source, selection)` resolves the root for `selection`, then applies the
  occurrences of `source` **last**, as if it were appended to the end of `resolutionOrder`, and
  resolves aliases, types and validation as usual.
- `preview_occurrence(occurrence, selection)` does the same with one occurrence from
  `inspect_source`. It is the way to look at a shadowed occurrence on its own, including one inside
  a nested resolver.

Neither touches the catalog or builds a synthetic resolver, so a preview can never affect normal
resolution.

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime, Resolution};
use serde_json::json;

fn main() -> Result<(), DtcgError> {
    let mut runtime = DtcgRuntime::from_json(
        "root.json",
        r##"{
          "version": "2025.10",
          "sets": { "base": { "sources": [{ "gap": { "$type": "number", "$value": 8 } }] } },
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

    // Every context's documents are requested, relative to the root.
    let Resolution::Missing(requests) = runtime.validate_source(&draft)? else { unreachable!() };
    for request in &requests {
        let gap = if request.resource_id.as_str().ends_with("compact.json") { 4 } else { 16 };
        runtime.provide_requested_json(
            request,
            format!(r#"{{ "gap": {{ "$type": "number", "$value": {gap} }} }}"#),
        )?;
    }
    assert!(runtime.validate_source(&draft)?.ready().is_some());

    let revision = runtime.catalog_revision();
    let preview = runtime.preview_source(&draft, &[])?.ready().unwrap();
    assert_eq!(preview.get("gap").unwrap().value, 4); // the draft's default context, applied last

    // The catalog and the committed resolution are unchanged.
    assert_eq!(runtime.catalog_revision(), revision);
    assert_eq!(runtime.resolve(&[])?.ready().unwrap().get("gap").unwrap().value, 8);
    Ok(())
}
```

> **Selections are checked against the root too.** As with
> [nested resolvers](06-resolvers-and-modifiers.md#nested-resolvers), the selection passed to
> `inspect_source`, `preview_source` and `preview_occurrence` must name only modifiers the root
> resolver declares. To preview a non-default context of a modifier that exists only in a draft,
> give the draft that context as its `default`, or add the modifier to the root first.

## Resolver definition helpers

Two free functions read resolver definitions with the runtime's own rules, for editors that work
on resolver JSON directly:

| Function | Checks | Returns |
|---|---|---|
| `set_sources(resource, &definition)` | `definition.sources` is an array | the sources as `&[Value]` |
| `modifier_contexts(resource, &definition)` | the definition is an object; `contexts` is a non-empty object of arrays; `default`, if present, is a string naming a context | the contexts as `&Map<String, Value>` |

Neither follows references or resolves anything. `modifier_contexts` does not require a `default`,
which is optional in DTCG; an editor that wants every modifier to have one can enforce that itself.
`resource` is used only in error messages.

```rust
use dtcg_runtime::{ResourceId, modifier_contexts, set_sources};
use serde_json::json;

let id = ResourceId::parse("acme.resolver.json").unwrap();

assert_eq!(set_sources(&id, &json!({ "sources": [{ "$ref": "a.json" }] })).unwrap().len(), 1);
assert!(set_sources(&id, &json!({ "sources": {} })).is_err());

let theme = json!({ "default": "light", "contexts": { "light": [], "dark": [] } });
let contexts = modifier_contexts(&id, &theme).unwrap();
assert_eq!(contexts.keys().collect::<Vec<_>>(), ["light", "dark"]);
assert!(modifier_contexts(&id, &json!({ "default": "sepia", "contexts": { "light": [] } })).is_err());
assert!(modifier_contexts(&id, &json!({ "contexts": {} })).is_err());
```

## Behavior shared with `resolve`

- **Same discovery.** References, fragments, overrides, nested resolvers and selections behave as in
  [Resolvers and modifiers](06-resolvers-and-modifiers.md).
- **Same document protocol.** Missing documents come back as `Resolution::Missing`; see
  [Document loading](07-document-loading.md).
- **Same limits.** `RuntimeLimits` applies. Preview occurrences count toward `max_documents`
  together with the root's sources.
- **Same errors.** Structural errors are `DtcgError`s. Previews also report value errors, since
  they resolve.
