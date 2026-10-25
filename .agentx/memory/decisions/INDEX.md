---
agentx:
  kind: index
  scope: decisions
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-25T16:00:00+02:00
  topics:
    - decisions
    - dtcg
  supersedes: []
  superseded_by: []
---
# dXn.Dtcg Decision Index

Durable architecture choices for the DTCG runtime. Current source remains the first evidence to inspect.

## Domains

- runtime
  - [Resolve from an I/O-free string catalog with explicit missing-document requests](runtime/resolution/2026-07-26-runtime-resolution-io-free-string-catalog.md)
  - [Overlay whole tokens by path, then resolve aliases on the final graph](runtime/resolution/2026-07-26-runtime-resolution-whole-token-overlay.md)
  - [Project narrow typed views through derives and FromDtcgValue](runtime/projection/2026-07-26-runtime-projection-narrow-typed-projection.md)
  - [Judge conformance by spec text, then the official corpus, then recorded rulings](runtime/resolution/2026-10-07-runtime-resolution-conformance-bar.md)
  - [Determine a token type from its own type, then its whole-reference target, then its group](runtime/resolution/2026-10-07-runtime-resolution-type-order.md)
  - [Give resolver structure one owner and derive every step view from it](runtime/resolution/2026-10-07-runtime-resolution-single-resolver-definition.md)
  - [Match inputs case-insensitively and validate them against the union of reached resolvers](runtime/resolution/2026-10-07-runtime-resolution-input-union-semantics.md)

- repository
  - [Keep the library standalone: no consumer-modeled code, `dxn.dtcg.*` extension ids, and a 1.x policy without an end condition](repository/packaging/2026-10-25-repository-packaging-standalone-library.md)
  - [Rename the crates to dxn_dtcg and dxn_dtcg_derive in dXnSix/dXn.Dtcg, released from v1.1.0](repository/packaging/2026-10-23-repository-packaging-dxn-rename.md)
  - [Organize the AgentX memory by crate area](repository/agentx-memory/2026-10-05-repository-agentx-memory-crate-area-taxonomy.md)
  - [Vendor the official DTCG corpus unmodified and ratchet its known gaps](repository/validation/2026-10-07-repository-validation-corpus-ratchet.md)
  - [Compile the README and every docs chapter as doctests; release-plz writes the changelog and migrations travel in `Migration:` footers](repository/validation/2026-10-18-repository-validation-doctests-and-generated-changelog.md) (supersedes the [2026-10-07 doctests and hand-written changelog decision](repository/validation/2026-10-07-repository-validation-docs-as-doctests.md))
  - [Release both crates as one git-tag unit with release-plz, stay on 1.x, and consume releases only by tag](repository/packaging/2026-10-18-repository-packaging-release-plz-git-tag-releases.md)
