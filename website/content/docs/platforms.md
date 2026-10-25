---
title: Platforms and versions
description: Supported platforms, minimum Rust version, crate layout, features, dependencies, thread safety and how releases and the 1.x versioning policy work.
group: Project
order: 190
---

`dxn_dtcg` is pure, safe Rust. It has no build scripts, no `unsafe` code, no async runtime and no file-system or network access, so it builds for any target that has the Rust standard library.

## Platform support

| Platform | Status |
|---|---|
| Linux x86_64 | Tested in CI: formatting, Clippy with `-D warnings`, tests, `cargo package` |
| Windows x86_64 | Tested in CI: the same steps |
| WebAssembly (`wasm32-unknown-unknown`) | `cargo check --workspace --target wasm32-unknown-unknown` in CI; needs no host I/O, threads or system calls |
| macOS, iOS, Android and other `std` targets | Expected to work (no platform-specific code); not covered by CI |

- **Rust** 1.97 or newer, edition 2024. `no_std` is not supported. The repository pins the toolchain to 1.97.0 in `rust-toolchain.toml`.
- **No threads started**, no clock, randomness or environment access.
- **Thread safety.** `DtcgRuntime`, `ResolvedTokens`, `ResolvedToken` and `DtcgError` are `Send + Sync`. Resolution borrows the runtime immutably, so several threads can resolve against one runtime at once. Mutating methods take `&mut self`; to update while others read, use a lock or a copy-on-write handle (see the [cookbook](./cookbook.md#share-one-runtime-across-threads)).

## Crates and features

| Crate | Kind | Contents |
|---|---|---|
| `dxn_dtcg` | library | `DtcgRuntime`, catalog, resolver, value validation, projection traits, source inspection, envelopes, diagnostics |
| `dxn_dtcg_derive` | proc-macro | `#[derive(DtcgTokens)]`, `#[derive(DtcgComponentTokens)]` |

`dxn_dtcg` depends on the derive crate and re-exports both macros, so applications depend only on `dxn_dtcg`.

| Feature | Default | Adds |
|---|---|---|
| `diagnostics` | off | `DtcgDiagnostic`, `DtcgDiagnosticCode` and `AffectedResolution::nodes_visited()`; see [Limits and diagnostics](./limits.md#the-diagnostics-feature) |

Runtime dependencies are `serde`, `serde_json` (with `preserve_order`, which token order depends on), `sha2` and `thiserror`. The derive crate uses `syn`, `quote` and `proc-macro2`. Both crates are dual-licensed MIT OR Apache-2.0.

## Installing a release

Releases are git tags; nothing is published to crates.io. Depend on a tag, never on a branch or a path:

```toml
[dependencies]
dxn_dtcg = { git = "https://github.com/dXnSix/dXn.Dtcg", tag = "v1.1.0" }
```

`Cargo.lock` records the exact commit. To update, change the tag by hand; the newest is on the [Releases](https://github.com/dXnSix/dXn.Dtcg/releases) page. `dxn_dtcg_derive` resolves from the same tag.

## Versioning

Both crates share one version, one tag `vX.Y.Z` and one `CHANGELOG.md`. The crates stay on 1.x: no commit is marked breaking, and a breaking change is released as a minor version whose changelog entry carries a `Migration:` line for each affected consumer. Plan for it accordingly and read the changelog when you move to a new tag. Types marked `#[non_exhaustive]` (including `DtcgError`) may gain variants and fields in minor releases.

:::note Renamed in v1.1.0
Releases up to v1.0.0 were published as `dtcg_runtime` and `dtcg_runtime_derive` from the `rayx-ui/RayX.Dtcg` repository. From v1.1.0 they are `dxn_dtcg` and `dxn_dtcg_derive` in `dXnSix/dXn.Dtcg`: rename the dependency and replace `dtcg_runtime::` paths with `dxn_dtcg::`. The API is otherwise unchanged.
:::

Next: [Contributing](./contributing.md).
