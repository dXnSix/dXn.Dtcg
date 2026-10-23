---
agentx:
  kind: decision
  scope: decisions/repository/packaging
  status: current
  created: 2026-10-18T23:30:00+02:00
  updated: 2026-10-23T22:00:00+02:00
  promoted_from: dtcg-releases
  topics:
    - release
    - versioning
    - release-plz
    - rayx-consumer
  supersedes: []
  superseded_by: []
---
## Decision: Release both crates as one git-tag unit with release-plz, stay on 1.x, and consume releases only by tag

### Context: Owner decisions of 2026-10-18 for the RayX libraries ([epic `dtcg-releases`](../../../../tasks/dtcg-releases/design.json), designs `d-release-plz-config`, `d-stay-on-1x`, `d-release-workflow`, `d-consume-by-tag`); dXn.Dtcg is the pilot of the shared library release template. Builds on RayX.CLI's [release-plz decision](../../../../../../../RayX/RayX.CLI.Old/.agentx/memory/decisions/distribution/2026-10-14-distribution-release-plz-tags-dist-releases.md), without cargo-dist. First release `v1.0.0`, 2026-10-18.

### Alternatives considered: crates.io publishing; independent crate versions with per-crate tags; major bumps for breaking changes; unpinned git, branch or path dependencies for consumers.

### Reasoning: `release-plz.toml` runs git-only (`git_only`, `publish = false`): `dxn_dtcg` owns the tag `v{{ version }}`, the GitHub Release (`git_release_enable`, titled `v{{ version }}` through `git_release_name`) and the root `CHANGELOG.md`; `dxn_dtcg_derive` shares `version_group = "dtcg"` with tag, release and changelog off and keeps the inherited `git_tag_name` (its previous-version lookup in git-only mode). `.github/workflows/release-plz.yml` (`release`, then `release-pr` with `needs: release`) authenticates with the shared fine-grained PAT `RELEASE_PLZ_TOKEN` and runs no tests; `ci.yml` (pull requests and pushes to `main`) is all a release pull request runs. Only `feat`/`fix`/`perf` release.

**1.x policy:** the owner is the only consumer, so no commit uses `!` or a `BREAKING CHANGE:` footer and `semver_check = false` (release-plz cannot cap a major bump). A breaking change is a `feat` (or `fix`) that states the break with `Migration:` footers, and a release pull request is merged only while it proposes `1.x` and changes only `Cargo.toml`, `Cargo.lock` and `CHANGELOG.md`. The policy ends when a second consumer appears.

Consumers pin `{ git = "https://github.com/dXnSix/dXn.Dtcg", tag = "vX.Y.Z" }`; a change RayX needs is released here first, then RayX bumps its tag and validates. Implementation work is pushed straight to `main` (`git fetch origin && git rebase origin/main`, `git push --force-with-lease origin main`).

### Trade-offs accepted: No registry; tag bumps in consumers are manual; semver no longer signals breaks while the 1.x policy holds; the PAT must cover every library repository (a missing grant fails at tag creation with 403, as the first run did).

### Invariants

- One tag `vX.Y.Z`, one GitHub Release and one root changelog section per version; never a per-crate tag or changelog.
- Tags come only from release-plz or the documented by-hand fallback; the "Release tags" ruleset (`refs/tags/v*`) blocks moving or deleting them, so bad releases are fixed forward.
- The manifests stay packageable (`cargo package --workspace`).

### Supersedes: None.
