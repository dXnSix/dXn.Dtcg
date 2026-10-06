---
agentx:
  kind: state
  scope: knowledge/runtime/resolution
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-07T07:00:00+02:00
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

`ResourceId` is a fragment-free, portable logical identifier; relative references canonicalize against the referencing document. `ResourceId::parse` rejects ids containing `#`. `normalize` rejects a leading `/`, any `\`, `:` in any path segment (Windows drive prefixes such as `C:/x` and `c:x`, URI schemes, NTFS streams; `..` could otherwise move a later segment first), control characters, `< > " | ? *`, `#`, empty or `.` segments, segments ending in a dot or space, Windows device names (`CON`, `NUL`, `COM1`… with or without extension), and `..` above the root, so a rejected `$ref` fails with `InvalidResourceId` instead of becoming a `DocumentRequest`. Fragments may contain `:`. One RFC 6901 parser (`json_pointer.rs`: percent-decoding, strict array indexes, one walker) serves resource fragments and every `$ref`. `DocumentCatalog` stores `Arc<str>` JSON per id and the runtime exposes a `catalog_revision()` counter. Supply documents with `from_json`, `from_envelope`, `provide_json`, `provide_requested_json`, the atomic `provide_json_batch`, `replace_json` and `remove_document`; `set_root` picks the root.

`resolve(selection)` returns `Resolution::Ready(ResolvedTokens)` or `Resolution::Missing(Vec<DocumentRequest>)` (each request carries the canonical id, the referencing id and the original reference); the caller loads the documents and resolves again. `validate_complete` also walks inactive contexts. Resolvers must declare `version: "2025.10"`. A string `$schema` is accepted and ignored at the root of a whole-document source only. `replace_envelope` swaps the stored resolver of an envelope root. `Resolution` exposes `map`, `is_ready`, `is_missing` and `missing`. `modifiers()` lists `ModifierAxis` values (contexts in authored order, optional default) in resolution order, then the remaining root modifiers in authored order; `normalize_selection` fills defaults, rejects unknown modifiers or contexts, and orders the result by axis so equal selections compare equal.

Active sources fold, in discovery order, into one merged structure (`flatten.rs`): groups merge member by member, group `$` properties per key (`$extensions` recursively), a later token replaces the whole token at its path, and a token/group collision fails naming both documents. The merged structure is flattened once, so types, `$extends` (iterative, dependency-ordered, memoized; copies carry an internal `$origin` so a group copied into itself is a cycle), `$deprecated` inheritance and every JSON Pointer see all sources as one document; order stays authored order with each path at its last definition. Flattening enforces Format structure (tokens have `$value` or a token-level `$ref`, never both, no children, `$root` or `$extends`; `$root` is a token; non-empty names; typed `$description`/`$deprecated`/`$extensions`). A token's type is its own `$type`, else the type of the token its whole value references, else the group's (Format §5.2.2); an own type differing from the referenced type is a `reference type mismatch`. Curly aliases are allowed only as the whole value or at the composite members and array elements listed in `reference_positions.rs`, where they must target the member's type; JSON Pointer objects are allowed anywhere. The token order is fixed once by `keep_last_definitions`. `RuntimeLimits` (256 documents, reference depth 64, alias depth 64, 4,000,000 value nodes, 100,000 tokens) bounds all work: `flatten.rs` counts tokens and `$extends` links against `max_tokens`, follows `$extends` chains iteratively and caps path depth at 128; `token_resolution.rs` counts every alias, token-pointer and document-pointer hop toward `max_alias_depth` (order-independent, literal tokens cost nothing), memoizes tokens and document pointers by (document, pointer), charges copied values against `max_value_nodes` and caps resolved value depth at 128; discovery counts every expansion step against `max_documents`. `ResolvedTokens` keeps tokens in resolution order with a SHA-256 fingerprint over canonical (sorted-key) values; each `ResolvedToken` carries `path`, `token_type`, `authored_value`, `value`, `extensions` (authored `$extensions` or `Null`) and `source`.

`ResolvedToken` carries `description` and `deprecated` (`Deprecation`, inherited from groups, `false` cancels); `ResolvedTokens::group`/`groups` expose `ResolvedGroup` metadata; the fingerprint ignores metadata. `replace_json_and_resolve_affected(resource, json, selection, changed_paths)` diffs the flattened tokens before and after the edit, adds named tokens and groups, document-pointer readers and the reverse-dependency closure (including edges to removed paths), validates them, and rolls back on any failure; `AffectedResolution` carries the tokens, `removed()`, `order()` and groups, and `ResolvedTokens::apply_affected` yields exactly the full resolution. `ResolverEnvelope` parses `.tokeneditor` envelopes (`{ "data": <resolver> }`) and `LocalReferencePolicy` allows only relative local `$ref`s (no absolute paths, backslashes or URI schemes).

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
