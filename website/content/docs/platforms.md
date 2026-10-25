---
title: Platforms and versions
description: Supported platforms, minimum Rust version, crate layout, features, dependencies, the expressions feature on WebAssembly, thread safety and how releases and the 1.x versioning policy work.
group: Project
order: 190
---

With the default features, `dxn_dtcg` is pure, safe Rust. The crates have no build scripts, no C code, no `unsafe` code, no async runtime and no file-system or network access, so the default build compiles for any target that has the Rust standard library. The opt-in `expressions` feature changes that on purpose: it adds a JavaScript engine written in C (QuickJS), compiled with the platform's C compiler, and a small amount of `unsafe` code around it. Everything below says which build it describes.

## Platform support

| Platform | Status |
|---|---|
| Linux x86_64 | Tested in CI: formatting, Clippy with `-D warnings`, tests, `cargo package`; the `--all-features` steps compile QuickJS with the system C compiler |
| Windows x86_64 | Tested in CI: the same steps, with MSVC compiling QuickJS |
| WebAssembly (`wasm32-unknown-unknown`) | Default features: `cargo check --workspace --target wasm32-unknown-unknown` in CI. With `expressions`: built with an LLVM clang and tested under Node in CI ([below](#webassembly-and-expressions)). Needs no host I/O, threads or system calls |
| macOS, iOS, Android and other `std` targets | Expected to work (no platform-specific code); not covered by CI. With `expressions`, the target also needs a C compiler that QuickJS builds with |

- **Rust** 1.97 or newer, edition 2024. `no_std` is not supported. The repository pins the toolchain to 1.97.0 in `rust-toolchain.toml`.
- **No threads started**, no clock, randomness or environment access, in either build. Expressions run without a clock or randomness (`Date` does not exist and `Math.random` throws).
- **Thread safety.** `DtcgRuntime`, `ResolvedTokens`, `ResolvedToken` and `DtcgError` are `Send + Sync`, with or without expression options (native functions must be `Send + Sync`). Resolution borrows the runtime immutably, so several threads can resolve against one runtime at once; with expressions, each resolution starts its own JavaScript engine. Mutating methods take `&mut self`; to update while others read, use a lock or a copy-on-write handle (see the [cookbook](./cookbook.md#share-one-runtime-across-threads)).

## Crates and features

| Crate | Kind | Contents |
|---|---|---|
| `dxn_dtcg` | library | `DtcgRuntime`, catalog, resolver, value validation, projection traits, source inspection, diagnostics, and with its feature the expression engine |
| `dxn_dtcg_derive` | proc-macro | `#[derive(DtcgTokens)]`, `#[derive(DtcgComponentTokens)]`, `#[expression_module]` |

`dxn_dtcg` depends on the derive crate and re-exports its macros (`#[expression_module]` as `dxn_dtcg::expressions::expression_module`, with the `expressions` feature), so applications depend only on `dxn_dtcg`.

| Feature | Default | Adds |
|---|---|---|
| `diagnostics` | off | `DtcgDiagnostic`, `DtcgDiagnosticCode` and `AffectedResolution::nodes_visited()`; see [Limits and diagnostics](./limits.md#the-diagnostics-feature) |
| `expressions` | off | Computed token values: the `dxn_dtcg::expressions` module (`ExpressionOptions`, `ExpressionModule`, `NativeModule`, `expression_module`), `RuntimeOptions::with_expressions`, and the QuickJS engine and oxc compiler behind them; see [Expressions](./expressions.md) |

Enabling the feature compiles the engine in but evaluates nothing: a runtime evaluates expressions only when its `RuntimeOptions` enable them, so feature unification in a larger build never changes how another crate's runtime resolves. The color API on `values::Color`, `DocumentKind`, the module catalog methods and `ResolvedToken::snapshot` are part of every build.

Runtime dependencies are `serde`, `serde_json` (with `preserve_order`, which token order depends on), `sha2` and `thiserror`. The `expressions` feature adds `quickjs-jit` (QuickJS bindings; its `quickjs-jit-sys` crate compiles QuickJS's C sources with the `cc` crate) and `oxc` (the JavaScript and TypeScript parser and transformer). The derive crate uses `syn`, `quote` and `proc-macro2`. Both crates are dual-licensed MIT OR Apache-2.0.

What the feature changes about the crate itself:

- **C code.** QuickJS is compiled from C for the target. The default build compiles no C and needs no C compiler.
- **`unsafe` code.** The default build has none. The feature adds a counting allocator for the engine (the memory and stack limits) and, on `wasm32-unknown-unknown`, one exported function, the clock symbol described below.
- **Build scripts.** The crates still have none of their own; the C build runs in the build script of the `quickjs-jit-sys` dependency.

## WebAssembly and expressions

With the default features, `dxn_dtcg` builds for `wasm32-unknown-unknown` with nothing but the Rust target. With `expressions`, QuickJS must be compiled for WebAssembly too, which needs an LLVM clang with the WebAssembly target and `llvm-ar`, named through the `cc` crate's variables `CC_wasm32_unknown_unknown` and `AR_wasm32_unknown_unknown`:

```sh
CC_wasm32_unknown_unknown=/opt/homebrew/opt/llvm/bin/clang \
AR_wasm32_unknown_unknown=/opt/homebrew/opt/llvm/bin/llvm-ar \
cargo build --target wasm32-unknown-unknown --features expressions
```

Apple's clang has no WebAssembly target; Homebrew's `llvm` or a distribution's `clang` and `llvm` packages do.

:::warning QuickJS's clock symbol belongs to the whole module
QuickJS's WebAssembly shim imports a clock from the host as `env.__rquickjs_host_now_us`. `dxn_dtcg` defines that symbol itself, always returning 0, because expressions have no clock, so the module needs no host import on QuickJS's behalf. The symbol belongs to the whole WebAssembly module, which links one copy of QuickJS: other code in the same module that uses `quickjs-jit` directly reads this clock too (`Date.now()` is 0) and cannot supply its own `env.__rquickjs_host_now_us`, because defining it again fails to link.
:::

- The default expression `stack_limit` (256 KiB) fits the default 1 MiB WebAssembly stack.
- Panics abort on `wasm32-unknown-unknown`, so a panicking native function stops the module instead of becoming a JavaScript error as it does natively.

CI covers both builds. The main job checks the default build for `wasm32-unknown-unknown`. A second job, `wasm-expressions` on `ubuntu-latest`, installs `clang` and `llvm`, installs `wasm-bindgen-cli` at the `wasm-bindgen` version locked in `Cargo.lock`, runs `cargo check -p dxn_dtcg --features expressions --target wasm32-unknown-unknown`, and runs the `expressions_wasm` test group under Node with `wasm-bindgen-test-runner`. [Contributing](./contributing.md#the-webassembly-group) shows how to run that group locally.

## Installing a release

Releases are git tags; nothing is published to crates.io. Depend on a tag, never on a branch or a path:

```toml
[dependencies]
dxn_dtcg = { git = "https://github.com/dXnSix/dXn.Dtcg", tag = "v1.1.0" }
```

`Cargo.lock` records the exact commit. To update, change the tag by hand; the newest is on the [Releases](https://github.com/dXnSix/dXn.Dtcg/releases) page. `dxn_dtcg_derive` resolves from the same tag. Add `features = ["expressions"]` for computed tokens, with a tag that includes the feature.

## Versioning

Both crates share one version, one tag `vX.Y.Z` and one `CHANGELOG.md`. The crates stay on 1.x: no commit is marked breaking, and a breaking change is released as a minor or patch version whose changelog entry carries a `Migration:` line for each affected audience, such as token documents or the Rust API. Plan for it accordingly and read the changelog when you move to a new tag. Types marked `#[non_exhaustive]` (including `DtcgError`, `DocumentKind` and `ExpressionOptions`) may gain variants and fields in minor releases.

:::note Renamed in v1.1.0
Releases up to v1.0.0 were published as `dtcg_runtime` and `dtcg_runtime_derive`. From v1.1.0 they are `dxn_dtcg` and `dxn_dtcg_derive` in `dXnSix/dXn.Dtcg`: rename the dependency and replace `dtcg_runtime::` paths with `dxn_dtcg::`. The API is otherwise unchanged.
:::

Next: [Contributing](./contributing.md).
