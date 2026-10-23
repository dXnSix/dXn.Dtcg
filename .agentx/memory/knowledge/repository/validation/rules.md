---
agentx:
  kind: rules
  scope: knowledge/repository/validation
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-23T22:00:00+02:00
  topics:
    - validation
    - tests
  supersedes: []
  superseded_by: []
---
# Validation Rules

- Give every new feature or behavior change its own `tests/<feature>.rs` group and run only that group while iterating; run the full contributor list once before committing.
- A change that alters behavior RayX observes is not proven until RayX, after bumping to the release tag that contains it, passes its affected crates and Test Suite specs.
- The README and every `docs/` chapter are doctests (`cargo test -p dxn_dtcg --all-features --doc`): tag every code block, and keep `rust` examples compiling and passing.
- Do not hand-edit `CHANGELOG.md`; release-plz writes it, and consumer migrations go into `Migration:` footers ([decision](../../../decisions/repository/validation/2026-10-18-repository-validation-doctests-and-generated-changelog.md)).
- Measure conformance with `tests/official_test_suite.rs` against the vendored corpus; a fixed gap is removed from `KNOWN_GAPS`, and the corpus files are never edited.
