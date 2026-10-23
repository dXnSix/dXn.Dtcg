---
agentx:
  kind: decision
  scope: decisions/repository/validation
  status: current
  created: 2026-10-07T12:00:00+02:00
  updated: 2026-10-23T22:00:00+02:00
  topics:
    - corpus
    - tests
    - conformance
  supersedes: []
  superseded_by: []
---
## Decision: Vendor the official DTCG corpus unmodified and ratchet its known gaps

### Context: Conformance needs an executable shared bar that tracks upstream; [epic `dtcg-2025-10-conformance`](../../../../tasks/dtcg-2025-10-conformance/design.json), design `corpus-ratchet`.

### Alternatives considered: Hand-written conformance cases only; editing fixtures to match the runtime.

### Reasoning: The upstream `test-suite/tests/2025.10/` tree is vendored byte-identical (LF, `.gitattributes` `-text`) under `crates/dxn_dtcg/tests/fixtures/dtcg-test-suite/2025.10/` with its licence and update steps, excluded from the package. `tests/official_test_suite.rs` compares failures with a sorted `KNOWN_GAPS` list (empty since the epic) that may only shrink; fixtures are never edited.

### Trade-offs accepted: An upstream update can add failures that must be fixed or explicitly listed; the group runs as a final check, not per sprint.

### Supersedes: None.
