---
agentx:
  kind: decision
  scope: decisions/repository/validation
  status: current
  created: 2026-10-07T12:00:00+02:00
  updated: 2026-10-07T12:00:00+02:00
  topics:
    - docs
    - doctests
    - changelog
  supersedes: []
  superseded_by: []
---
## Decision: Compile the README and every docs chapter as doctests, and log behavior changes with consumer migrations

### Context: The guide drifted from the API; consumers need a precise migration list for a strict-by-default release; [epic `dtcg-2025-10-conformance`](../../../../tasks/dtcg-2025-10-conformance/design.json), design `docs-as-tests`.

### Alternatives considered: Unchecked Markdown examples; release notes written at publish time.

### Reasoning: `lib.rs` includes `README.md` and each `docs/*.md` in `#[cfg(doctest)]` modules, so every Rust example compiles and runs under `cargo test --doc` without affecting builds or the package. `CHANGELOG.md` keeps Keep a Changelog sections plus a Migration subsection per consumer (RayX, RayX.TokenEditor) naming affected files.

### Trade-offs accepted: Every API or behavior change must update the affected chapter examples and the changelog in the same change.

### Supersedes: None.
