---
agentx:
  kind: decision
  scope: decisions/runtime/projection
  status: current
  created: 2026-10-26T03:00:00+02:00
  updated: 2026-10-26T03:00:00+02:00
  topics:
    - values
    - color
    - css-color-4
  supersedes: []
  superseded_by: []
  promoted_from: dtcg-expressions
---
## Decision: Provide CSS Color 4 color math as a public, feature-independent API on `values::Color`

### Context: Design tokens need conversions, mixing, contrast and gamut handling; the expressions' `dxn.dtcg/color` module needs one implementation.

### Alternatives considered: Only a JavaScript color library in expressions; an external color crate.

### Reasoning: `values::Color` converts between the 14 DTCG spaces with the CSS Color 4 matrices and transfer functions (XYZ D65 hub, Bradford for D50 spaces, BT.1886 for Rec. 2020) and offers mixing (premultiplied alpha, shorter hue arc, powerless hues), OKLCH lighten/darken, hex, WCAG contrast on sRGB-clipped colors and gamut tests in linear light; results are always valid DTCG colors and match colorjs.io within 1e-5. Every value model serializes through its DTCG JSON form. `dxn.dtcg/color` wraps the same functions.

### Trade-offs accepted: Linear values within 1e-14 of 0 encode as 0, so tiny A98 RGB and Rec. 2020 components read as 0; converted hues are zeroed only at rounding-noise chroma.

### Supersedes: None.
