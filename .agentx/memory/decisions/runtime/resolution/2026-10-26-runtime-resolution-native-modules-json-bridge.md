---
agentx:
  kind: decision
  scope: decisions/runtime/resolution
  status: current
  created: 2026-10-26T03:00:00+02:00
  updated: 2026-10-26T03:00:00+02:00
  topics:
    - expressions
    - native-modules
    - serde
  supersedes: []
  superseded_by: []
  promoted_from: dtcg-expressions
---
## Decision: Expose Rust functions to expressions as native ES modules over a JSON bridge

### Context: Hosts need typed helpers (units, colors, brand math) callable from expressions without lifetimes or unsafe code in their crates.

### Alternatives considered: rquickjs class bindings; a value-converting bridge; host functions as globals.

### Reasoning: `NativeModule::new(specifier).function(..).constant(..)` (and `#[expression_module]`) store type-erased closures over `serde_json::Value`; each session wraps them in generated, frozen ES modules that call one internal bridge with JSON text. `Err`s and caught panics become JavaScript `Error`s prefixed `<specifier>.<name>:`, failed deserialization and NaN or infinite arguments `TypeError`s; export names must be non-reserved identifiers. The built-ins `dxn.dtcg` and `dxn.dtcg/color` use the same mechanism, and `typescript_declarations()` renders a `.d.ts` from the registrations.

### Trade-offs accepted: Every call serializes its arguments and result as JSON; native functions must be pure and synchronous.

### Supersedes: None.
