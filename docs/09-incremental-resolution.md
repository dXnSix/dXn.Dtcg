# 9. Incremental resolution

A full `resolve` is cheap for typical design systems, but editors that re-resolve on every
keystroke in large token files need less work per edit. This chapter covers the tools for that:
fingerprints for detecting change, the catalog revision for caching, and affected re-resolution,
which recomputes only the tokens an edit can have changed.

- [Fingerprints](#fingerprints)
- [Catalog revisions](#catalog-revisions)
- [Affected re-resolution](#affected-re-resolution)
- [Merging into a snapshot](#merging-into-a-snapshot)
- [Rollback](#rollback)
- [Caller responsibilities](#caller-responsibilities)
- [What is and is not saved](#what-is-and-is-not-saved)

## Fingerprints

`ResolvedTokens::fingerprint()` returns a lowercase hexadecimal SHA-256 digest (64 characters) of
the resolved token set. For each token in resolution order it hashes:

1. the token path,
2. the effective type,
3. the resolved value, serialized as compact JSON,
4. the resource id of the winning definition,

each followed by a zero byte.

Consequences:

- The fingerprint is **stable**: the same documents and selection produce the same fingerprint in
  every run, process and platform.
- It **changes** when any resolved value, type, winning source document or the token order
  changes.
- It **does not change** for edits that leave all of those intact: `$description`, `$deprecated`,
  `$extensions`, or re-pointing an alias at a different token with the same value.
- Values are hashed as JSON text, so `1` and `1.0` hash differently.

Typical uses are skipping a re-render when a theme switch or file reload produced identical tokens,
keying caches of derived data (generated CSS, GPU uniforms), and comparing two selections
(`light.fingerprint() == dark.fingerprint()`).

## Catalog revisions

`DtcgRuntime::catalog_revision()` is a counter that goes up on every successful catalog mutation
(see the [catalog API](07-document-loading.md#catalog-api)). The pair (revision, normalized
selection) identifies a resolution exactly, which makes it a cache key that needs no resolution to
compute. See the cache example in [Document loading](07-document-loading.md#caching-and-sharing).

## Affected re-resolution

```rust,ignore
impl DtcgRuntime {
    pub fn replace_json_and_resolve_affected(
        &mut self,
        resource_id: impl AsRef<str>,
        json: impl Into<Arc<str>>,
        selection: &[(&str, &str)],
        changed_paths: &[&str],
    ) -> Result<Resolution<AffectedResolution>, DtcgError>;
}
```

The call replaces one existing document and resolves only what the edit can have affected:

1. The document must already be in the catalog (`UnknownDocument` otherwise). Its text is
   replaced.
2. Discovery and flattening run for `selection` as in a full resolution.
3. The **affected set** starts with those `changed_paths` that exist in the new token set. It then
   grows to every token that depends on an affected token, transitively, through curly-brace
   aliases, token pointers (`#/path/$value`) and document pointers whose JSON contains either.
4. If any active source uses `$extends`, every token is treated as affected, because group
   extension copies tokens in ways the dependency graph does not track.
5. Only the affected tokens, and anything they depend on, are resolved and validated.
6. The result lists the affected tokens in resolution order.

`AffectedResolution::iter()` yields those tokens as `ResolvedToken`s. With the `diagnostics`
feature, `AffectedResolution::nodes_visited()` reports how many tokens were resolved in total,
including unaffected dependencies.

## Merging into a snapshot

`ResolvedTokens::apply_affected` merges an affected result into an earlier full resolution of the
same selection and recomputes the fingerprint:

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime};

fn tokens(base: u32) -> String {
    format!(
        r#"{{
          "space": {{
            "$type": "number",
            "base": {{ "$value": {base} }},
            "inset": {{ "$value": "{{space.base}}" }}
          }},
          "radius": {{ "$type": "number", "$value": 4 }}
        }}"#
    )
}

fn main() -> Result<(), DtcgError> {
    let mut runtime = DtcgRuntime::from_json("tokens.json", tokens(8))?;
    let mut snapshot = runtime.resolve(&[])?.ready().unwrap();

    // The user edited `space.base` from 8 to 12.
    let affected = runtime
        .replace_json_and_resolve_affected("tokens.json", tokens(12), &[], &["space.base"])?
        .ready()
        .unwrap();

    // `space.inset` aliases `space.base`, so it is affected; `radius` is not.
    let paths: Vec<&str> = affected.iter().map(|token| &*token.path).collect();
    assert_eq!(paths, ["space.base", "space.inset"]);

    snapshot.apply_affected(&affected);
    assert_eq!(snapshot.get("space.inset").unwrap().value, 12);

    // The patched snapshot matches a full resolution.
    let full = runtime.resolve(&[])?.ready().unwrap();
    assert_eq!(snapshot.fingerprint(), full.fingerprint());
    Ok(())
}
```

`apply_affected` replaces tokens that already exist, in place, and appends new ones at the end.

## Rollback

If the replacement does not produce a ready result, because the new text has an error or
references a document that is not in the catalog, the previous document text is put back. The
catalog's contents are as before the call; only its revision has moved on (+2: the replacement and
the restore). A failed edit can never leave the runtime holding a broken document.

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime};

const GOOD: &str = r#"{ "space": { "$type": "number", "base": { "$value": 8 } } }"#;
const BAD: &str = r#"{ "space": { "$type": "number", "base": { "$value": "wide" } } }"#;

fn main() -> Result<(), DtcgError> {
    let mut runtime = DtcgRuntime::from_json("tokens.json", GOOD)?;
    let before = runtime.resolve(&[])?.ready().unwrap();
    let revision = runtime.catalog_revision();

    let result = runtime.replace_json_and_resolve_affected("tokens.json", BAD, &[], &["space.base"]);
    assert!(matches!(result, Err(DtcgError::TokenType { .. })));

    let after = runtime.resolve(&[])?.ready().unwrap();
    assert_eq!(after.fingerprint(), before.fingerprint());
    assert_eq!(runtime.catalog_revision(), revision + 2);
    Ok(())
}
```

## Caller responsibilities

Affected re-resolution trusts the caller on a few points:

- **`changed_paths` is yours to compute.** The runtime does not compare the old and new documents.
  An editor knows which token the user changed. A file watcher can diff the two flattened token
  sets or fall back to a full `resolve`. Paths that are missing from the new token set are ignored.
- **Use the snapshot's selection.** Merge an affected result only into a full resolution of the
  same selection.
- **Deletions and renames need a full resolve.** `apply_affected` only adds and replaces. It does
  not remove tokens that the edit deleted, so the snapshot would keep a stale token.
- **Additions change order.** A token the edit added is appended to the snapshot, which may differ
  from its position, and so the fingerprint, in a full resolution.
- **Structural edits need a full resolve.** After adding or removing sources, changing a
  resolver's sets or modifiers, or replacing a document other than the edited one, resolve fully.

## What is and is not saved

Affected re-resolution saves alias resolution and type validation for the tokens the edit cannot
reach. Like a full resolution, it still parses every active document, flattens every source and
builds the dependency graph from the whole input. The savings therefore grow with the number of
tokens and the cost of resolving them, not with the number of documents. For a design system with
a few hundred tokens, a full `resolve` is usually simpler and fast enough.
