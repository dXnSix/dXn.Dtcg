# RayX.Dtcg contributor guidance

Rust runtime for DTCG 2025.10 design-token documents: `crates/dtcg_runtime` (resolution, typed
values, diagnostics) and `crates/dtcg_runtime_derive` (projection derives). The crates are
UI-neutral and must not depend on RayX or gpux.

## Validation

Run before committing:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo package --workspace
```

Test only what you changed: run the touched crate's focused tests while iterating, and the full
list above once before committing.

## Consumers

RayX (`../RayX`) depends on this repository by relative path (`../RayX.Dtcg/crates/...`) for local
co-development. A change to a public API used by RayX needs the matching RayX change; keep the
two commits paired and validate RayX's affected crates and Test Suite specs.

## Commits

Use Conventional Commits with meaningful scopes. Never add Claude co-author lines.
