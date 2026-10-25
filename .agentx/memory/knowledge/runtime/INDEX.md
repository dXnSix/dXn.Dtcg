---
agentx:
  kind: index
  scope: knowledge/runtime
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-25T16:00:00+02:00
  topics:
    - runtime
    - dtcg-runtime
  supersedes: []
  superseded_by: []
---
# Runtime Domain Index

The two published crates: `dxn_dtcg` (resolution, values, inspection, diagnostics) and `dxn_dtcg_derive` (projection derives re-exported by `dxn_dtcg`).

- [state](state.md) · [knowledge](knowledge.md) · [rules](rules.md) · [hypotheses](hypotheses.md)

## Subdomains

- [resolution](resolution/INDEX.md): document catalog, resource ids, resolver documents and selection, whole-token overlay, aliases, limits, affected resolution, `LocalReferencePolicy`.
- [projection](projection/INDEX.md): value validation and value extension types, `FromDtcgValue`/`TypeRegistry`, `DtcgTokens`/`DtcgComponentTokens` derives.
- [source-inspection](source-inspection/INDEX.md): authored source references, occurrence inspection, preview, and the redacted `diagnostics` feature.
