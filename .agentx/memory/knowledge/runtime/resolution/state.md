---
agentx:
  kind: state
  scope: knowledge/runtime/resolution
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-06T21:18:58+02:00
  topics:
    - resolution
    - catalog
    - resolver
    - aliases
    - incremental-resolution
  supersedes: []
  superseded_by: []
---
# Resolution State

## Purpose And Usage

Read before changing how documents are supplied, how resolvers select sources, or how tokens overlay and alias.

## Current Technical Shape

`ResourceId` is a fragment-free logical identifier; relative references canonicalize against the referencing document. `normalize` rejects a leading `/`, any `\`, `:` in any path segment (Windows drive prefixes such as `C:/x` and `c:x`, URI schemes, NTFS streams; `..` could otherwise move a later segment first), empty or `.` segments, and `..` above the root, so a rejected `$ref` fails with `InvalidResourceId` instead of becoming a `DocumentRequest`. Fragments may contain `:`. `DocumentCatalog` stores `Arc<str>` JSON per id and the runtime exposes a `catalog_revision()` counter. Supply documents with `from_json`, `from_envelope`, `provide_json`, `provide_requested_json`, the atomic `provide_json_batch`, `replace_json` and `remove_document`; `set_root` picks the root.

`resolve(selection)` returns `Resolution::Ready(ResolvedTokens)` or `Resolution::Missing(Vec<DocumentRequest>)` (each request carries the canonical id, the referencing id and the original reference); the caller loads the documents and resolves again. `validate_complete` also walks inactive contexts. Resolvers must declare `version: "2025.10"`. `modifiers()` lists `ModifierAxis` values (contexts in authored order, optional default) in resolution order, then the remaining root modifiers by name; `normalize_selection` fills defaults, rejects unknown modifiers or contexts, and orders the result by axis so equal selections compare equal.

Sources flatten in resolution order and a later definition replaces the whole token at its path; effective types and aliases resolve afterwards against the final active graph. The token order is fixed once by `keep_last_definitions`. `RuntimeLimits` defaults to 256 documents, reference depth 64 and alias depth 64. `ResolvedTokens` keeps tokens in resolution order with a SHA-256 fingerprint; each `ResolvedToken` carries `path`, `token_type`, `authored_value`, `value`, `extensions` (authored `$extensions` or `Null`) and `source`.

`replace_json_and_resolve_affected(resource, json, selection, changed_paths)` returns the changed tokens and their transitive dependents in source order as an `AffectedResolution`; `ResolvedTokens::apply_affected` merges it and refreshes the fingerprint. `ResolverEnvelope` parses `.tokeneditor` envelopes (`{ "data": <resolver> }`) and `LocalReferencePolicy` allows only relative local `$ref`s (no absolute paths, backslashes or URI schemes).

## Backlinks

- [Resolver](../../../../../crates/dtcg_runtime/src/resolver.rs)
- [Catalog](../../../../../crates/dtcg_runtime/src/catalog.rs)
- [Resource ids](../../../../../crates/dtcg_runtime/src/resource.rs)
- [Selection](../../../../../crates/dtcg_runtime/src/selection.rs)
- [Envelope](../../../../../crates/dtcg_runtime/src/envelope.rs)
- [Local reference policy](../../../../../crates/dtcg_runtime/src/local_reference.rs)
- [Tests](../../../../../crates/dtcg_runtime/tests)
- 2026-07-26 current [I/O-free string catalog decision](../../../decisions/runtime/resolution/2026-07-26-runtime-resolution-io-free-string-catalog.md)
- 2026-07-26 current [whole-token overlay decision](../../../decisions/runtime/resolution/2026-07-26-runtime-resolution-whole-token-overlay.md)
