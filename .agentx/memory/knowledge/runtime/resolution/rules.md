---
agentx:
  kind: rules
  scope: knowledge/runtime/resolution
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-05T18:00:00+02:00
  topics:
    - resolution
    - overlay
    - aliases
  supersedes: []
  superseded_by: []
---
# Resolution Rules

- A later definition replaces the whole token at its path. Never deep-merge composite values, groups or Serde objects ([decision](../../../decisions/runtime/resolution/2026-07-26-runtime-resolution-whole-token-overlay.md)).
- Resolve effective types and aliases only after the complete overlay of the active selection.
- Keep catalog mutations atomic: an invalid id, reference, JSON body or request mismatch leaves the catalog unchanged, and affected resolution restores the previous document unless it is `Ready`.
- Do not reintroduce per-override scans of the token order; fix the order once per resolution.
- Keep the resolver version check strict (`2025.10`).
