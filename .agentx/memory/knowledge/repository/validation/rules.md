---
agentx:
  kind: rules
  scope: knowledge/repository/validation
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-07T13:00:00+02:00
  topics:
    - validation
    - tests
  supersedes: []
  superseded_by: []
---
# Validation Rules

- Give every new feature or behavior change its own `tests/<feature>.rs` group and run only that group while iterating; run the full contributor list once before committing.
- A change that alters behavior RayX observes is not proven until RayX's affected crates and Test Suite specs pass against it.
- The README and every `docs/` chapter are doctests (`cargo test -p dtcg_runtime --all-features --doc`): tag every code block, and keep `rust` examples compiling and passing.
- Measure conformance with `tests/official_test_suite.rs` against the vendored corpus; a fixed gap is removed from `KNOWN_GAPS`, and the corpus files are never edited.
