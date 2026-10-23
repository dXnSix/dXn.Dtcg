---
agentx:
  kind: state
  scope: knowledge/repository
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-23T22:00:00+02:00
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

dXn.Dtcg was extracted with its history from RayX `crates/rayx/dtcg_runtime` and `crates/rayx/dtcg_runtime_derive` on 2026-10-05 so the crates release on their own cadence ([RayX decision](../../../../../../RayX/RayX.Old/.agentx/memory/decisions/runtime/workspace/2026-10-05-runtime-workspace-sibling-repositories.md)). Since 2026-10-18 it is released by git tag (`v1.0.0` first) with release-plz ([decision](../../decisions/repository/packaging/2026-10-18-repository-packaging-release-plz-git-tag-releases.md), [docs/releasing.md](../../../../docs/releasing.md)); nothing goes to crates.io. RayX still points `dxn_dtcg` at `../dXn.Dtcg/crates/dxn_dtcg` until its epic `rayx-released-libraries` switches to `tag = "v1.0.0"`; from then on a change RayX needs is released here first and RayX bumps its tag. Repository instructions live in [AGENTS.md](../../../../AGENTS.md) and [CLAUDE.md](../../../../CLAUDE.md).

## Backlinks

- [Workspace manifest](../../../../Cargo.toml)
- [README](../../../../README.md)
- [CI](../../../../.github/workflows/ci.yml)
