---
agentx:
  kind: state
  scope: knowledge/repository/packaging
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-23T22:00:00+02:00
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

The workspace has two members sharing `[workspace.package]` metadata (version 1.1.0, edition 2024, `rust-version` 1.95, `MIT OR Apache-2.0`, repository `github.com/dXnSix/dXn.Dtcg`); both crates use the root README. `[workspace.dependencies]` declares each path dependency once with both `path` and `version`, so packaging rewrites them to registry versions. `dxn_dtcg` depends on the derive crate and re-exports its macros; the derive crate's dev-dependency on `dxn_dtcg` is path-only, so `cargo package` strips it and the runtime/derive cycle disappears from the published crates. `dxn_dtcg` dev-depends on itself with the `diagnostics` feature. Both crates share one version: release-plz (`release-plz.toml`, git-only, nothing published) bumps `[workspace.package]` and both `[workspace.dependencies]` requirements together under the version group `dtcg`, and tags the release `vX.Y.Z`. `rust-toolchain.toml` pins 1.95.0 with clippy, rustfmt and the `wasm32-unknown-unknown` target.

## Backlinks

- [Workspace manifest](../../../../../Cargo.toml)
- [dxn_dtcg manifest](../../../../../crates/dxn_dtcg/Cargo.toml)
- [dxn_dtcg_derive manifest](../../../../../crates/dxn_dtcg_derive/Cargo.toml)
- [Toolchain](../../../../../rust-toolchain.toml)
- [release-plz configuration](../../../../../release-plz.toml)
- 2026-10-23 current [Renamed to dxn_dtcg and dxn_dtcg_derive](../../../decisions/repository/packaging/2026-10-23-repository-packaging-dxn-rename.md)
