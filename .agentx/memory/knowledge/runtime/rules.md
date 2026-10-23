---
agentx:
  kind: rules
  scope: knowledge/runtime
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-23T22:00:00+02:00
  topics:
    - runtime
    - dtcg-runtime
    - dependencies
  supersedes: []
  superseded_by: []
---
# Runtime Rules

- Keep `dxn_dtcg` I/O-free and UI-neutral: no dependency on RayX, gpux, GPUI, filesystem or network access, or an async runtime. Document loading belongs to the caller through `Resolution::Missing` ([decision](../../decisions/runtime/resolution/2026-07-26-runtime-resolution-io-free-string-catalog.md)).
- Any value shape beyond DTCG 2025.10 needs a doc comment naming its consumer, an off-by-default `ValueExtensions` flag, and a pinned test group, as `tests/rayx_export_types.rs` and `tests/value_extensions.rs` do for the RayX Figma export.
