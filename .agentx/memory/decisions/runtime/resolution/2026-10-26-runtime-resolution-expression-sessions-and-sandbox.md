---
agentx:
  kind: decision
  scope: decisions/runtime/resolution
  status: current
  created: 2026-10-26T03:00:00+02:00
  updated: 2026-10-26T03:00:00+02:00
  topics:
    - expressions
    - sandbox
    - determinism
    - limits
  supersedes: []
  superseded_by: []
  promoted_from: dtcg-expressions
---
## Decision: One sandboxed QuickJS session per resolving call; module values frozen, module state shared by owner decision

### Context: Expressions run author code inside resolution, which must stay deterministic, bounded and I/O-free.

### Alternatives considered: A session per token (no sharing, slower, modules evaluated per token); a long-lived engine across calls; time-based budgets.

### Reasoning: Each resolving call lazily creates one session (runtime, hardened context, compiled modules) and drops it on return. Hardening removes ambient capabilities and freezes every reachable built-in. Budgets are deterministic: steps are QuickJS interrupt checks, memory is counted by a wrapping allocator, and the stack limit is measured from each evaluation's start (`JS_UpdateStackTop`, because rquickjs moves the stack top only with `parallel`). quickjs-jit serves small objects from 4 KiB arenas, so a stack overflow is made uncatchable by compile-time instrumentation (every catch/finally and source-written iterator `return` body passes caught values to an internal check), the promise rejection tracker (async functions), unbounded unwrapping of wrapped errors, and a run-time nesting bound on regular expression patterns, not by allocation depth. Modules evaluate once per resolution and their namespace and top-level values are deep-frozen after their top-level code.

### Trade-offs accepted: Owner decisions (2026-10-25): module state any code changes (reassigned module variables, closures, collection and buffer contents) is shared within one resolution, and results depending on it are guaranteed neither order-independent nor equal between affected and full resolution (SBX-4, EVAL-6); a stack overflow inside an iterator `return` method not written as one in source, discarded while closing the iterator after a throw, is not detected (SBX-5).

### Supersedes: None.
