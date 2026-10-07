---
agentx:
  kind: decision
  scope: decisions/runtime/resolution
  status: current
  created: 2026-10-07T12:00:00+02:00
  updated: 2026-10-07T12:00:00+02:00
  topics:
    - conformance
    - dtcg-2025-10
    - corpus
  supersedes: []
  superseded_by: []
---
## Decision: Judge conformance by spec text, then the official corpus, then recorded rulings

### Context: DTCG 2025.10 (Format, Color, Resolver) contradicts itself in places, and the official corpus is schema-oriented with known bugs; [epic `dtcg-2025-10-conformance`](../../../../tasks/dtcg-2025-10-conformance/design.json), design `conformance-bar`.

### Alternatives considered: Treat the corpus as the sole definition of conformance; treat every SHOULD as an error; follow the JSON Schema where it differs from the text.

### Reasoning: Normative MUST/MUST NOT text wins; the pinned corpus (`882ebd6`) decides where the text is silent or ambiguous; remaining conflicts are ruled once and recorded. Rulings: token-level `$ref` targets `.../$value` (Example 32); `xyz-*` stays in [0, 1] and hue is never wrapped; gradient positions outside [0, 1] are valid and only the typed model clamps; percent-encoded pointer fragments are decoded; `{group}` aliases are errors and roots are `{group.$root}` (schema issue #448 aside); SHOULDs become errors only where schema and corpus enforce them (single-context modifiers, unknown resolver members, empty `resolutionOrder`), others such as case-insensitive inputs are behavior; nested resolvers stay an extension; token order stays authored order.

### Trade-offs accepted: Some corpus fixtures are interpreted rather than mirrored, and future errata may flip a ruling; each such change belongs in the CHANGELOG and this record.

### Supersedes: None.
