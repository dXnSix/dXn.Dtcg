---
agentx:
  kind: decision
  scope: decisions/runtime/resolution
  status: current
  created: 2026-10-26T03:00:00+02:00
  updated: 2026-10-26T03:00:00+02:00
  topics:
    - expressions
    - typescript
    - oxc
  supersedes: []
  superseded_by: []
  promoted_from: dtcg-expressions
---
## Decision: Accept TypeScript by stripping types with oxc, never checking them

### Context: Token authors and hosts write TypeScript; the runtime must stay small and deterministic.

### Alternatives considered: A TypeScript type checker; JavaScript only; a WebAssembly build of `tsc`.

### Reasoning: `ts` expressions and `.ts`/`.mts` modules go through oxc's parser, semantic checks, TypeScript transformer and codegen with a source map; error positions map back to the authored text. oxc also validates syntax and early errors with positions, rejects dynamic `import()`, and every source's nesting is bounded before parsing (oxc recurses on the caller's stack).

### Trade-offs accepted: Type errors are not reported; positions are as coarse as QuickJS records them.

### Supersedes: None.
