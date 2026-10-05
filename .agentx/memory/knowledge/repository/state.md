---
agentx:
  kind: state
  scope: knowledge/repository
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-05T18:00:00+02:00
  topics:
    - repository
    - sibling-repositories
    - rayx-consumer
  supersedes: []
  superseded_by: []
---
# Repository State

## Purpose And Usage

Read before repository-level work: manifests, publishing, validation routing, cross-repository changes, or memory maintenance.

## Current Technical Shape

RayX.Dtcg was extracted with its history from RayX `crates/rayx/dtcg_runtime` and `crates/rayx/dtcg_runtime_derive` on 2026-10-05 so the crates publish to crates.io on their own cadence ([RayX decision](../../../../../RayX/.agentx/memory/decisions/runtime/workspace/2026-10-05-runtime-workspace-sibling-repositories.md)). It is checked out beside RayX; RayX's root `[workspace.dependencies]` points `dtcg_runtime` at `../RayX.Dtcg/crates/dtcg_runtime`, so a public API change RayX uses needs a paired RayX commit validated by RayX's affected crates and Test Suite specs. Repository instructions live in [AGENTS.md](../../../../AGENTS.md) and [CLAUDE.md](../../../../CLAUDE.md).

## Backlinks

- [Workspace manifest](../../../../Cargo.toml)
- [README](../../../../README.md)
- [CI](../../../../.github/workflows/ci.yml)
