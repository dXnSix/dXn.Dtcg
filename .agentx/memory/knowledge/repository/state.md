---
agentx:
  kind: state
  scope: knowledge/repository
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-25T16:00:00+02:00
  topics:
    - repository
    - sibling-repositories
    - consumers
  supersedes: []
  superseded_by: []
---
# Repository State

## Purpose And Usage

Read before repository-level work: manifests, publishing, validation routing, cross-repository changes, or memory maintenance.

## Current Technical Shape

dXn.Dtcg is a standalone library that models no consumer and uses `dxn.dtcg.*` for its extension identifiers ([standalone library decision](../../decisions/repository/packaging/2026-10-25-repository-packaging-standalone-library.md) (2026-10-25, current)). Since 2026-10-18 it is released by git tag (`v1.0.0` first) with release-plz ([decision](../../decisions/repository/packaging/2026-10-18-repository-packaging-release-plz-git-tag-releases.md), [docs/releasing.md](../../../../docs/releasing.md)); nothing goes to crates.io. Consumers depend on a release tag; a change a consumer needs is released here first. Repository instructions live in [AGENTS.md](../../../../AGENTS.md) and [CLAUDE.md](../../../../CLAUDE.md).

## Backlinks

- [Workspace manifest](../../../../Cargo.toml)
- [README](../../../../README.md)
- [CI](../../../../.github/workflows/ci.yml)
