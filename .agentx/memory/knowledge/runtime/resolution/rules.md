---
agentx:
  kind: rules
  scope: knowledge/runtime/resolution
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-07T06:00:00+02:00
  topics:
    - resolution
    - overlay
    - aliases
  supersedes: []
  superseded_by: []
---
# Resolution Rules

- A later definition replaces the whole token at its path; never deep-merge token values or composite values ([decision](../../../decisions/runtime/resolution/2026-07-26-runtime-resolution-whole-token-overlay.md)). Groups are the exception: active sources fold into one merged structure where groups merge member by member and group `$` properties merge per key (`$extensions` recursively), as epic `dtcg-2025-10-conformance` design `merged-structure` decided.
- Resolve effective types, `$extends`, `$deprecated` inheritance, aliases and JSON Pointers only on the complete merged structure of the active selection.
- Keep catalog mutations atomic: an invalid id, reference, JSON body or request mismatch leaves the catalog unchanged, and affected resolution restores the previous document unless it is `Ready`.
- Do not reintroduce per-override scans of the token order; fix the order once per resolution.
- Keep the resolver version check strict (`2025.10`).
