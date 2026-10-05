---
agentx:
  kind: knowledge
  scope: knowledge/runtime
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-05T18:00:00+02:00
  topics:
    - runtime
    - dtcg-runtime
  supersedes: []
  superseded_by: []
---
# Runtime Knowledge

- `serde_json`'s `preserve_order` feature is semantic, not cosmetic: token resolution order, ordered duplicate occurrences and `ResolvedTokens::iter()` all follow authored order.
- The crate is UI-neutral but carries three RayX-facing conveniences that other users can ignore: the `dev.rayx.blur` value type and dimension-valued typography `lineHeight` (both used by the RayX Figma export), `ResolverEnvelope` for Token Editor `.tokeneditor` projects, and `LocalReferencePolicy`, the relative-local-reference rule shared by RayX themes and the Token Editor.
