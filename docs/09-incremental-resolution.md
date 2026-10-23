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
3. the resolved value as canonical JSON, with object members sorted by key at every level,
4. the resource id of the winning definition,

each followed by a zero byte.

Consequences:

- The fingerprint is **stable**: the same documents and selection produce the same fingerprint in
  every run, process and platform.
- It **changes** when any resolved value, type, winning source document or the token order
  changes.
- It **does not change** for edits that leave all of those intact: `$description`, `$deprecated`,
  `$extensions`, group metadata, the member order of an object value, or re-pointing an alias at a
  different token with the same value.
- Scalars are hashed as JSON text, so `1` and `1.0` hash differently.

Typical uses are skipping a re-render when a theme switch or file reload produced identical tokens,
keying caches of derived data (generated CSS, GPU uniforms), and comparing two selections
(`light.fingerprint() == dark.fingerprint()`). To also notice metadata changes, compare the
`ResolvedTokens` values with `==`, which includes metadata and groups but, like the fingerprint,
ignores the member order of object values.

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

1. The document must already be in the catalog (`UnknownDocument` otherwise).
2. The runtime flattens the active sources for `selection` **before** the edit, replaces the text,
   and flattens them again. Discovery runs again too, so edits to resolver documents (new sources,
   changed sets or contexts) are covered.
3. The **affected set** starts with:
   - every token whose flattened definition is new or differs from before: its value, own or
     inherited type, `$description`, `$deprecated`, `$extensions` or winning source;
   - every token at or below a path in `changed_paths` (a group path selects every token under
     it; `""` selects everything);
   - every token that reads a document pointer (a `$ref` into JSON that is not a token value),
     because the edit may have changed what it reads.
4. It then grows to every token that depends on an affected token or on a **removed** path,
   transitively, through curly-brace aliases and token pointers (`#/path/$value`).
5. If any group in the merged structure uses `$extends`, or the state before the edit could not be
   flattened, every token is affected.
6. The affected tokens, and anything they depend on, are resolved and validated.

`changed_paths` is therefore optional: the comparison finds changed definitions on its own. Name
paths to force re-resolution of tokens the comparison cannot see change.

The result, an `AffectedResolution`, describes the new state:

| Method | Returns |
|---|---|
| `iter()` | The re-resolved tokens as `ResolvedToken`s, in the new resolution order, metadata included |
| `removed()` | The token paths the edit removed |
| `order()` | Every token path after the edit, in resolution order |
| `nodes_visited()` | With the `diagnostics` feature: how many tokens were resolved in total, including unaffected dependencies |

## Merging into a snapshot

`ResolvedTokens::apply_affected` merges an affected result into the full resolution of the same
selection from before the edit. It removes the removed paths, replaces and adds the re-resolved
tokens, adopts the new order and groups, and recomputes the fingerprint. **The patched snapshot
equals a full `resolve`** of the edited documents, metadata included.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};
use serde_json::{Value, json};

fn number(value: Value) -> Value {
    json!({ "$type": "number", "$value": value })
}

fn main() -> Result<(), DtcgError> {
    let mut runtime = DtcgRuntime::from_json(
        "tokens.json",
        json!({
            "space": { "base": number(json!(8)), "inset": number(json!("{space.base}")) },
            "radius": number(json!(4))
        })
        .to_string(),
    )?;
    let mut snapshot = runtime.resolve(&[])?.ready().unwrap();

    // The user edited `space.base` from 8 to 12.
    let affected = runtime
        .replace_json_and_resolve_affected(
            "tokens.json",
            json!({
                "space": { "base": number(json!(12)), "inset": number(json!("{space.base}")) },
                "radius": number(json!(4))
            })
            .to_string(),
            &[],
            &["space.base"],
        )?
        .ready()
        .unwrap();

    // `space.inset` aliases `space.base`, so it is affected; `radius` is not.
    let paths: Vec<&str> = affected.iter().map(|token| &*token.path).collect();
    assert_eq!(paths, ["space.base", "space.inset"]);
    snapshot.apply_affected(&affected);
    assert_eq!(snapshot.get("space.inset").unwrap().value, 12);
    assert_eq!(snapshot, runtime.resolve(&[])?.ready().unwrap());

    // A file reload: `radius` was removed, `gap` added and a description changed. No paths named.
    let affected = runtime
        .replace_json_and_resolve_affected(
            "tokens.json",
            json!({
                "gap": number(json!(2)),
                "space": {
                    "base": { "$type": "number", "$value": 12, "$description": "Base unit" },
                    "inset": number(json!("{space.base}"))
                }
            })
            .to_string(),
            &[],
            &[],
        )?
        .ready()
        .unwrap();
    assert_eq!(affected.removed(), ["radius"]);
    let paths: Vec<&str> = affected.iter().map(|token| &*token.path).collect();
    // `space.base` changed (its description), so its dependent `space.inset` is re-resolved too.
    assert_eq!(paths, ["gap", "space.base", "space.inset"]);
    assert_eq!(affected.order(), ["gap", "space.base", "space.inset"]);

    let fingerprint = snapshot.fingerprint().to_owned();
    snapshot.apply_affected(&affected);
    let full = runtime.resolve(&[])?.ready().unwrap();
    assert_eq!(snapshot, full);
    assert_eq!(snapshot.get("space.base").unwrap().description.as_deref(), Some("Base unit"));
    assert_ne!(snapshot.fingerprint(), fingerprint); // `radius` and `gap` changed the token set
    Ok(())
}
```

## Rollback

If the replacement does not produce a ready result, because the new text has an error, breaks a
token that depends on it, or references a document that is not in the catalog, the previous
document text is put back. The catalog's contents are as before the call; only its revision has
moved on (+2: the replacement and the restore). A failed edit can never leave the runtime holding
a broken document.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

const GOOD: &str = r#"{
  "space": { "$type": "number", "base": { "$value": 8 }, "inset": { "$value": "{space.base}" } }
}"#;
const BAD_VALUE: &str = r#"{
  "space": { "$type": "number", "base": { "$value": "wide" }, "inset": { "$value": "{space.base}" } }
}"#;
const DELETED_TARGET: &str = r#"{
  "space": { "$type": "number", "inset": { "$value": "{space.base}" } }
}"#;

fn main() -> Result<(), DtcgError> {
    let mut runtime = DtcgRuntime::from_json("tokens.json", GOOD)?;
    let before = runtime.resolve(&[])?.ready().unwrap();
    let revision = runtime.catalog_revision();

    let result = runtime.replace_json_and_resolve_affected("tokens.json", BAD_VALUE, &[], &[]);
    assert!(matches!(result, Err(DtcgError::TokenType { .. })));
    assert_eq!(runtime.catalog_revision(), revision + 2);

    // Deleting a token that another token aliases breaks the alias.
    let result = runtime.replace_json_and_resolve_affected("tokens.json", DELETED_TARGET, &[], &[]);
    assert!(matches!(result, Err(DtcgError::UnresolvedReference { .. })));

    assert_eq!(runtime.resolve(&[])?.ready().unwrap(), before);
    Ok(())
}
```

## Caller responsibilities

Affected re-resolution relies on the caller for a few points:

- **Keep the snapshot in step with the catalog.** The comparison is against the catalog as it is
  when the call starts, so merge each affected result into a full resolution of the same selection
  taken after the previous change. After any other mutation (`provide_json`, `replace_json`,
  `remove_document`, `replace_envelope`), resolve fully.
- **Use the snapshot's selection.** Merge an affected result only into a full resolution of the
  same selection.
- **One document per call.** Edits to several documents need one call each, applied in order, or a
  full resolve.

## What is and is not saved

Affected re-resolution saves alias resolution and type validation for the tokens the edit cannot
reach. It still parses every active document and flattens the active sources twice, before and
after the edit, and builds the dependency graph from the whole input. A `$extends` anywhere in the
merged structure makes every token affected. The savings therefore grow with the number of tokens
and the cost of resolving them, not with the number of documents. For a design system with a few
hundred tokens, a full `resolve` is usually simpler and fast enough.
