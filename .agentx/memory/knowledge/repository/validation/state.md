---
agentx:
  kind: state
  scope: knowledge/repository/validation
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-26T04:00:00+02:00
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

CI runs the whole suite everywhere, with all features and with the default ones: the `native` job on Ubuntu, Windows and macOS (arm64) with toolchain 1.97.0 (`cargo fmt --check`, Clippy with `-D warnings`, `cargo test --workspace`, `cargo check --workspace --target wasm32-unknown-unknown`, `cargo package --workspace`), and the `wasm32` job (LLVM clang, `wasm-bindgen-cli` at the locked `wasm-bindgen` version) with Clippy and `cargo test --workspace --tests` for `wasm32-unknown-unknown` under Node. Tests are `#[wasm_bindgen_test(unsupported = test)]`; `tests/support/mod.rs` reads files through Node's `fs`, runs stack tests on the wasm main stack and times with `Date.now()`; only `trybuild` cases and panic-catching tests are native-only.

Each feature has one integration-test file under `crates/<crate>/tests/<feature>.rs` (for example `alias_overlay`, `lazy_catalog`, `incremental_resolution`, `source_inspection`, `dtcg_2025_10_conformance`, `value_extension_types`, `local_reference_policy`), selected with `cargo test -p dxn_dtcg --all-features --test <feature>`. The derive crate has `derive_projection.rs` and `trybuild.rs` (a plain attribute-shape test, not trybuild compile-fail cases). Expression groups are `expressions_*` (`disabled`, `format`, `evaluation`, `sandbox`, `typescript`, `apis`, `wasm`, `modules`, `document_modules`, `native`, `builtin`, `color`, `native_macro`, `declarations`), most gated on `#![cfg(feature = "expressions")]`; `color_conversion` and `value_serde` need no feature. `expressions_native_macro` runs trybuild compile-fail cases from `tests/expression_module_ui/`; trybuild and oxc are native-only dev-dependencies. The docs chapters are doctests (`documentation` module in `lib.rs`); `docs/15-expressions.md` is included only with the feature. `DtcgRuntime::from_json` carries a doc test, and `benches/resolver.rs` measures resolution.

## Backlinks

- [CI workflow](../../../../../.github/workflows/ci.yml)
- [dxn_dtcg tests](../../../../../crates/dxn_dtcg/tests)
- [dxn_dtcg_derive tests](../../../../../crates/dxn_dtcg_derive/tests)
- [Contributor validation list](../../../../../CLAUDE.md)
