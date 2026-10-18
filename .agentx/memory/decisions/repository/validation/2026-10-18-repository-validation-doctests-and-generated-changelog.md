---
agentx:
  kind: decision
  scope: decisions/repository/validation
  status: current
  created: 2026-10-18T23:30:00+02:00
  updated: 2026-10-18T23:30:00+02:00
  promoted_from: dtcg-releases
  topics:
    - docs
    - doctests
    - changelog
    - release-plz
  supersedes:
    - 2026-10-07-repository-validation-docs-as-doctests.md
  superseded_by: []
---
## Decision: Compile the README and every docs chapter as doctests; release-plz writes the changelog and migrations travel in `Migration:` footers

### Context: Releases became git tags cut by release-plz ([epic `dtcg-releases`](../../../../tasks/dtcg-releases/design.json), design `d-changelog-from-commits`, 2026-10-18). release-plz inserts generated sections below the changelog header and never merges hand-written text, so the hand-edited changelog half of the [2026-10-07 decision](2026-10-07-repository-validation-docs-as-doctests.md) no longer works.

### Alternatives considered: Keep hand-editing `CHANGELOG.md` beside the generated sections; a `BREAKING CHANGE:` footer for migrations (forces a major bump release-plz cannot cap).

### Reasoning: The doctest half carries over unchanged: `lib.rs` includes `README.md` and every `docs/*.md` chapter in `#[cfg(doctest)]` modules, every code block is language-tagged, and an API or behavior change updates the affected chapter examples in the same commit. `CHANGELOG.md` is written by release-plz from Conventional Commits (the `[changelog]` template in `release-plz.toml`); a change a consumer must react to carries one `Migration: <consumer>: <what to change>` footer per consumer (RayX, RayX.TokenEditor, files), which the template prints as a nested line under the entry and so reaches the GitHub Release. A line that reads badly is fixed on the release pull request right before merging. The `1.0.0` section is the hand-written history up to the first release.

### Trade-offs accepted: Changelog quality depends on commit messages; migrations must be written in the commit, not later.

### Supersedes: [Compile the README and every docs chapter as doctests, and log behavior changes with consumer migrations](2026-10-07-repository-validation-docs-as-doctests.md).
