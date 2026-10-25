---
agentx:
  kind: decision
  scope: decisions/runtime/resolution
  status: current
  created: 2026-10-26T03:00:00+02:00
  updated: 2026-10-26T03:00:00+02:00
  topics:
    - expressions
    - quickjs
    - features
  supersedes: []
  superseded_by: []
  promoted_from: dtcg-expressions
---
## Decision: Evaluate token expressions only when opted in twice, with QuickJS on every target

### Context: DTCG 2025.10 has no computed values; tools need derived tokens without breaking documents other tools read.

### Alternatives considered: Expressions in `$value`; a pluggable engine trait; Boa or a custom interpreter; always-on evaluation.

### Reasoning: An expression lives in `$extensions["dxn.dtcg.expressions"]` and the token keeps a valid `$value` snapshot (Format §5.2.3, §8). The off-by-default Cargo feature `expressions` compiles QuickJS (`rquickjs` = `quickjs-jit =0.14.1`, `default-features = false`, `std` + `loader`) and oxc `=0.153.0` in, and only `RuntimeOptions::with_expressions` turns evaluation on, so feature unification never changes resolution for code that did not opt in. One engine everywhere (no engine trait) keeps results identical across native targets and `wasm32-unknown-unknown`. The default build stays free of C.

### Trade-offs accepted: The feature needs a C compiler (an LLVM clang for wasm32) and adds unsafe code (counting allocator, wasm clock symbol, stack-top update).

### Supersedes: None.
