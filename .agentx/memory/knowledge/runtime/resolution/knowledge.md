---
agentx:
  kind: knowledge
  scope: knowledge/runtime/resolution
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-25T16:00:00+02:00
  topics:
    - resolution
    - incremental-resolution
    - performance
  supersedes: []
  superseded_by: []
---
# Resolution Knowledge

- `replace_json_and_resolve_affected` restores the previous catalog document when the replacement fails or does not produce `Resolution::Ready`. `AffectedResolution::nodes_visited()` exists only with the `diagnostics` feature and counts resolved nodes, not the whole-input serialization, flattening or dependency-discovery work. Source flattening and dependency discovery still inspect the whole input, and group extensions conservatively mark all tokens affected.
- Fixing the token order once with `keep_last_definitions` (each path at its last definition), instead of scanning the order on every override in `insert_raw_token`, removed ~2 s per edit on large editor fixtures (measured 2026-09-10 in a WASM diagnostics host: undo/redo 2.2 s before, 0.33 s after).
- A missing document is data, not an error: `Resolution::Missing` lets the same runtime serve asset services, HTTP loaders, in-memory tests and preloaded catalogs.
