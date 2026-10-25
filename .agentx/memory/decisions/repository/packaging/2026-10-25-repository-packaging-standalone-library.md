---
agentx:
  kind: decision
  scope: decisions/repository/packaging
  status: current
  created: 2026-10-25T16:00:00+02:00
  updated: 2026-10-25T16:00:00+02:00
  topics:
    - standalone-library
    - extension-namespace
    - release-policy
  supersedes: []
  superseded_by: []
---
## Decision: Keep the library standalone: no consumer-modeled code, `dxn.dtcg.*` extension ids, and a 1.x policy without an end condition

### Context: On 2026-10-25 the owner decided that dXn.Dtcg is a dxn library tied to no application. The crates still carried one application's conventions: an extension type named after that application, a `ValueExtensions` profile and flag named after it, a project-file envelope (`ResolverEnvelope`, `from_envelope*`, `replace_envelope`, `DtcgError::InvalidEnvelope`, diagnostic code `Envelope`) for that application's editor format, and repository rules that named it as the reference consumer.

### Alternatives considered: Keep the names as aliases next to new ones; keep the envelope as a generic `{ "data": <resolver> }` format; keep the consumer-based release rules.

### Reasoning: A library that names or models one consumer leaks that consumer's format into every user's API. Library-defined extension identifiers use the `dxn.dtcg.*` namespace: the blur type is `dxn.dtcg.blur` (`BLUR_TYPE`), its flag is `ValueExtensions::blur` (`with_blur`) and `ValueExtensions::all()` turns every value extension on. The envelope is an application's file format, so it was removed rather than generalized; `LocalReferencePolicy` stays as a generic loader rule with its own test group. Extensions are justified by their use case, never by a consumer. The 1.x policy no longer ends "when a second consumer appears": the crates stay on 1.x until a 2.0 is deliberately decided, and `Migration:` footers name an audience (token documents, the Rust API), not a consumer. History records (old epics, released changelog entries) were rewritten or removed on the owner's request.

### Trade-offs accepted: Documents using the old blur type name and code using the removed envelope API or the old `ValueExtensions` names break once; no compatibility aliases are kept.

### Supersedes: None as a whole. It replaces the [rename decision](2026-10-23-repository-packaging-dxn-rename.md)'s (2026-10-23, current) earlier trade-off that kept the old extension names, and the [git-tag release decision](2026-10-18-repository-packaging-release-plz-git-tag-releases.md)'s (2026-10-18, current) consumer-based end condition; both records now point here.
