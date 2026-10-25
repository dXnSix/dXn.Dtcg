---
agentx:
  kind: knowledge
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
# Runtime Knowledge

- `serde_json`'s `preserve_order` feature is semantic, not cosmetic: token resolution order, ordered duplicate occurrences and `ResolvedTokens::iter()` all follow authored order.
- Two value extensions are opt-in through `ValueExtensions` (the `dxn.dtcg.blur` type and dimension-valued typography `lineHeight`, the form design tools export), and `LocalReferencePolicy` is an opt-in loader rule; documents and hosts that do not ask for them never see them.
