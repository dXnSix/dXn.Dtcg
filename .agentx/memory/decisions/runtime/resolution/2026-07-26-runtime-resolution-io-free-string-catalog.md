---
agentx:
  kind: decision
  scope: decisions/runtime/resolution
  status: current
  created: 2026-07-26T16:30:00+02:00
  updated: 2026-10-25T16:00:00+02:00
  topics:
    - resolution
    - catalog
    - lazy-loading
    - io-free
  supersedes: []
  superseded_by: []
---
## Decision: Resolve from an I/O-free string catalog with explicit missing-document requests

### Context: Design-token resolution had been mixed with theme compilation, component overrides and loading assumptions; a reusable runtime needed to resolve DTCG documents without knowing where they come from.

### Alternatives considered: Keep resolution inside an application's theme layer; embed a filesystem/HTTP loader in the runtime; take a loader trait or async callback.

### Reasoning: `dxn_dtcg` stores `Arc<str>` documents by fragment-free logical `ResourceId`, and `resolve` returns `Ready` or `Missing(Vec<DocumentRequest>)`. The caller loads requested documents from any source (asset services, HTTP, in-memory tests, preloaded catalogs) and resolves again. Without I/O, async or UI dependencies the crate is reusable by any host and builds for `wasm32-unknown-unknown`. Mutations are atomic and invalid input leaves the catalog unchanged.

### Trade-offs accepted: Callers run the request/provide loop themselves. Relative references are canonicalized logically, so callers map logical ids onto their storage.

### Supersedes: None.
