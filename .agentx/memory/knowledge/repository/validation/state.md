---
agentx:
  kind: state
  scope: knowledge/repository/validation
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-23T22:00:00+02:00
  topics:
    - validation
    - tests
    - ci
    - wasm
  supersedes: []
  superseded_by: []
---
# Validation State

## Purpose And Usage

Read before adding tests or choosing which checks a change needs.

## Current Technical Shape

CI runs on Ubuntu and Windows with toolchain 1.95.0: `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features`, `cargo check --workspace --target wasm32-unknown-unknown`, and `cargo package --workspace`.

Each feature has one integration-test file under `crates/<crate>/tests/<feature>.rs` (for example `alias_overlay`, `lazy_catalog`, `incremental_resolution`, `source_inspection`, `dtcg_2025_10_conformance`, `rayx_export_types`), selected with `cargo test -p dxn_dtcg --all-features --test <feature>`. The derive crate has `derive_projection.rs` and `trybuild.rs` (a plain attribute-shape test, not trybuild compile-fail cases). `DtcgRuntime::from_json` carries a doc test, and `benches/resolver.rs` measures resolution.

Runtime proof in a UI belongs to RayX: `rayx_theme`'s test groups and the Test Suite DTCG page (`apps/test_suite/src/pages/dxn_dtcg.rs` with `tools-node/test_suite/rayx-dtcg-runtime.spec.ts`).

## Backlinks

- [CI workflow](../../../../../.github/workflows/ci.yml)
- [dxn_dtcg tests](../../../../../crates/dxn_dtcg/tests)
- [dxn_dtcg_derive tests](../../../../../crates/dxn_dtcg_derive/tests)
- [Contributor validation list](../../../../../CLAUDE.md)
