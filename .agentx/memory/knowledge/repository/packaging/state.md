---
agentx:
  kind: state
  scope: knowledge/repository/packaging
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-05T18:00:00+02:00
  topics:
    - packaging
    - crates-io
    - workspace
  supersedes: []
  superseded_by: []
---
# Packaging State

## Purpose And Usage

Read before changing manifests, dependencies, versions or the toolchain.

## Current Technical Shape

The workspace has two members sharing `[workspace.package]` metadata (version 0.1.0, edition 2024, `rust-version` 1.95, `MIT OR Apache-2.0`, repository `github.com/rayx-ui/RayX.Dtcg`); both crates use the root README. `[workspace.dependencies]` declares each path dependency once with both `path` and `version`, so packaging rewrites them to registry versions. `dtcg_runtime` depends on the derive crate and re-exports its macros; the derive crate's dev-dependency on `dtcg_runtime` is path-only, so `cargo package` strips it and the runtime/derive cycle disappears from the published crates. `dtcg_runtime` dev-depends on itself with the `diagnostics` feature. `rust-toolchain.toml` pins 1.95.0 with clippy, rustfmt and the `wasm32-unknown-unknown` target.

## Backlinks

- [Workspace manifest](../../../../../Cargo.toml)
- [dtcg_runtime manifest](../../../../../crates/dtcg_runtime/Cargo.toml)
- [dtcg_runtime_derive manifest](../../../../../crates/dtcg_runtime_derive/Cargo.toml)
- [Toolchain](../../../../../rust-toolchain.toml)
