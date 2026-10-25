---
agentx:
  kind: decision
  scope: decisions/runtime/resolution
  status: current
  created: 2026-10-26T03:00:00+02:00
  updated: 2026-10-26T04:00:00+02:00
  topics:
    - expressions
    - wasm32
    - platforms
  supersedes: []
  superseded_by: []
  promoted_from: dtcg-expressions
---
## Decision: Define QuickJS's wasm32 clock import inside the crate, always returning 0

### Context: quickjs-jit's `wasm32-unknown-unknown` shim imports `env.__rquickjs_host_now_us`, which would force every host to supply a clock.

### Alternatives considered: Requiring hosts to provide the import; patching quickjs-jit.

### Reasoning: Expressions have no clock, so the crate defines `__rquickjs_host_now_us` (returning 0) under `cfg(all(target_arch = "wasm32", target_os = "unknown"))` and the module needs no host import. CI builds with an LLVM clang and runs the whole test suite on wasm32 under `wasm-bindgen-test-runner`.

### Trade-offs accepted: The symbol is defined for the whole WebAssembly module: other `quickjs-jit` users in it read a clock of 0 and cannot define their own (documented).

### Supersedes: None.
