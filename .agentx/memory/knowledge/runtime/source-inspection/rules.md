---
agentx:
  kind: rules
  scope: knowledge/runtime/source-inspection
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-05T18:00:00+02:00
  topics:
    - source-inspection
    - diagnostics
    - privacy
  supersedes: []
  superseded_by: []
---
# Source Inspection Rules

- Keep preview on the same discovery and resolution path as `resolve`; never build a synthetic resolver or mutate the catalog to preview.
- Diagnostics must never retain document text or JSON values; expose only logical ids, paths and stable codes.
