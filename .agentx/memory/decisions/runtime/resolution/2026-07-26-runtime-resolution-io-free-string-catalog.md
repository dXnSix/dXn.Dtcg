---
agentx:
  kind: decision
  scope: decisions/runtime/resolution
  status: current
  created: 2026-07-26T16:30:00+02:00
  updated: 2026-10-05T18:00:00+02:00
  topics:
    - resolution
    - catalog
    - lazy-loading
    - io-free
  supersedes: []
  superseded_by: []
---
## Decision: Resolve from an I/O-free string catalog with explicit missing-document requests

### Context: RayX resolved design tokens through `rayx_runtime_theming_dtcg`, which mixed DTCG resolution with RayX theme compilation, component overrides and loading assumptions. A reusable runtime was needed before RayX theme integration could be simplified (epic `dtcg-runtime`, DES-001 and DES-003).

### Alternatives considered: Keep resolution inside the RayX theme crate; embed a filesystem/HTTP loader in the runtime; take a loader trait or async callback.

### Reasoning: `dtcg_runtime` stores `Arc<str>` documents by fragment-free logical `ResourceId`, and `resolve` returns `Ready` or `Missing(Vec<DocumentRequest>)`. The caller loads requested documents from any source (RayX `AssetService`, HTTP, in-memory tests, preloaded catalogs) and resolves again. Without I/O, async or UI dependencies the crate is reusable outside RayX and builds for `wasm32-unknown-unknown`. Mutations are atomic and invalid input leaves the catalog unchanged.

### Trade-offs accepted: Callers run the request/provide loop themselves. Relative references are canonicalized logically, so callers map logical ids onto their storage.

### Supersedes: None.
