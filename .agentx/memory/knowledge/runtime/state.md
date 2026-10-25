---
agentx:
  kind: state
  scope: knowledge/runtime
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-26T02:00:00+02:00
  topics:
    - runtime
    - dtcg-runtime
    - public-api
  supersedes: []
  superseded_by: []
---
# Runtime State

## Purpose And Usage

Read before changing the public API or deciding which module owns a behavior.

## Current Technical Shape

`dxn_dtcg` resolves DTCG 2025.10 token documents and resolvers from already-loaded JSON strings. Its only dependencies are `serde`, `serde_json` (with `preserve_order`), `sha2`, `thiserror` and the derive crate; nothing touches the filesystem, network, an async runtime or a UI toolkit. `DtcgRuntime` (`runtime.rs`) is the facade: it owns a `DocumentCatalog`, an optional root `ResourceId`, `RuntimeLimits` and a stateless `TypeRegistry`, and exposes resolution, projection and source-inspection entry points. `DtcgError` (`error.rs`) is the single error type; it is `#[non_exhaustive]`, `Clone` and `PartialEq`. Missing alias and `$extends` targets raise `UnresolvedReference { resource, path, target, message }`; `Projection` is reserved for projection. `ResolvedToken`, `RuntimeLimits` (built with `with_*` setters), `ModifierAxis`, `DocumentRequest`, `ValueValidationError` and `DtcgDiagnosticCode` are also `#[non_exhaustive]`. `DtcgRuntime`'s `Debug` prints ids and limits, never document bodies.

Value validation is strict DTCG 2025.10 by default; `RuntimeOptions { limits, extensions }` with `ValueExtensions` flags (`line_height_dimension`, `blur`; `ValueExtensions::all()` turns both on) opts into dimension typography `lineHeight` and the `dxn.dtcg.blur` type (`DtcgRuntime::with_options`, `from_json_with_options`, `validate_value_with`).

Token expressions (feature `expressions`, `src/expressions/`) are opt-in twice: the feature compiles QuickJS (`rquickjs` = `quickjs-jit =0.14.1`) and oxc in, and `RuntimeOptions::with_expressions(ExpressionOptions)` turns evaluation on. `Resolver::evaluate_expression` (`token_resolution.rs`) resolves `$value` as before, resolves the expression's `inputs` at an unchecked position, evaluates in a per-call QuickJS `Session` (`engine.rs`) and keeps the resolved `$value` as `ResolvedToken::snapshot`. The sandbox (`sandbox.rs`) counts allocations against the memory limit, treats an allocation near the stack limit as a stack overflow (every evaluation first moves QuickJS's stack top to where it starts: rquickjs does so only with its `parallel` feature), counts interrupt checks against the step budget (a tripped call drains QuickJS's carried counter and reruns once), and freezes every reachable built-in. `nesting.rs` bounds the syntax-tree depth of every source before oxc parses it (oxc recurses on the caller's stack), with weights measured against a 1 MiB unoptimized stack.

Expressions import ES modules (`modules.rs`, `engine.rs`): host modules (`ExpressionOptions::with_module`), document-declared modules (`./<resource id>` internally), native modules and the built-ins `dxn.dtcg` and `dxn.dtcg/color` (`builtins.rs`); the `dxn.dtcg` namespace is reserved. Modules evaluate once per resolution in ES order (Tarjan components, cycles allowed), and errors name the module that threw. Native modules (`native.rs`) export Rust closures over a JSON-text bridge (`dxn.dtcg/native`, unreachable from user code): frozen exports, `Err`/panic → catchable `Error` prefixed `<specifier>.<name>:`, failed argument deserialization → `TypeError`, export names checked as non-reserved identifiers (panic in the builder). `typescript_declarations()` (`declarations.rs`) renders a `.d.ts` of the value types, built-ins and native modules. quickjs-jit serves allocations up to 512 bytes from 4 KiB arenas (`JS_ARENA_*` in quickjs.c, no switch to disable), so the error QuickJS throws at its stack limit reaches the counting allocator only sometimes: the guard (threshold `limit - min(limit/8, 16 KiB)`) is a backstop, not the detector; session setup runs under at least 256 KiB because x86_64 needs more than the 16 KiB minimum for it. `compile.rs` instruments every `try` and the body of every `return` method or arrow written in source (text insertions only, positions mapped back; redeclared parameters passed through the wrapper arrow): caught values go to `check`/`checked` in the internal module `dxn.dtcg/freeze`, which trips the guard on QuickJS's overflow `RangeError` or the regexp compiler's `SyntaxError: stack overflow` (unwrapping `SuppressedError`/`AggregateError`); QuickJS's promise rejection tracker covers async functions; HARDEN bounds patterns built at run time to 20 groups and classes (a class nests only with the `v` flag) so the regexp compiler cannot overflow. By owner decision (SBX-5 supersedes SBX-2) an overflow inside a `return` method not written as one in source, discarded while closing an iterator after a throw, is not detected. Every JS/TS module ends with generated code that deep-freezes its namespace and top-level values (`freezeAll`; objects others inherit from are tamed into accessors first; charged to the module's budget, about 5 steps per 1,000 objects). By owner decision (SBX-4/EVAL-6 supersede SBX-3/EVAL-5) state freezing cannot reach (reassigned module-level variables, closures, collection and buffer contents) is shared within one resolution, whoever writes it. On wasm32 the crate defines QuickJS's clock import `__rquickjs_host_now_us` (always 0) for the whole module.

Conformance is measured against the official DTCG 2025.10 corpus vendored byte-identically under `crates/dxn_dtcg/tests/fixtures/dtcg-test-suite/2025.10/` (excluded from the package); `tests/official_test_suite.rs` ratchets the failing fixtures against its `KNOWN_GAPS` list.

## Backlinks

- [Public API](../../../../crates/dxn_dtcg/src/lib.rs)
- [DtcgRuntime facade](../../../../crates/dxn_dtcg/src/runtime.rs)
- [Errors](../../../../crates/dxn_dtcg/src/error.rs)
- 2026-10-05 current [Domain index](INDEX.md)
