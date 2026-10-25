---
title: Releasing
description: How dxn_dtcg releases are cut with release-plz - commit types, the 1.x policy, what to check before merging a release pull request, the token secret and the by-hand fallback.
group: Project
order: 210
---

A release is a version tag `vX.Y.Z` on `main` plus a GitHub Release whose notes are that version's `CHANGELOG.md` section. Both crates share one version, one tag, one Release and one root changelog. Nothing is published to crates.io, although the manifests stay packageable. This page is for maintainers; consumers only need [Platforms and versions](./platforms.md#installing-a-release).

release-plz does the work (`release-plz.toml`, `.github/workflows/release-plz.yml`) in git-only mode (`git_only = true`, `publish = false`): versions come from `v*` tags, not from a registry.

## How a release happens

1. Commit to `main` with [Conventional Commits](https://www.conventionalcommits.org/). The types decide the next version.
2. On every push to `main`, the `release-pr` job opens or updates the release pull request (branch `release-plz-...`): the version bump in `Cargo.toml` (the `[workspace.package]` version and both `[workspace.dependencies]` requirements), `Cargo.lock` and a new `CHANGELOG.md` section.
3. Review and merge it.
4. On that push the `release` job finds a version with no tag, pushes `vX.Y.Z` and creates the GitHub Release with the new changelog section as notes.

`v1.0.0` was tagged without a pull request because `Cargo.toml` already said `1.0.0` and no tag existed. `v1.1.0`, which renamed the crates, was cut with the by-hand fallback below, because release-plz looks packages up by name in the previous tag and found none; from `v1.1.0` on it works again.

## Commit types and the 1.x policy

| Commit | Release |
|---|---|
| `fix:`, `perf:` | patch |
| `feat:` | minor |
| `docs:`, `test:`, `ci:`, `build:`, `chore:`, `refactor:` | none on their own (`release_commits = "^(feat\|fix\|perf)"`); listed in the next release's changelog |

`chore(agentx)`, `chore(release)` and `chore(deps)` commits are left out of the changelog.

**1.x policy.** The owner is the only consumer, so the crates stay on 1.x. No commit is marked breaking: no `!` after the type and no `BREAKING CHANGE:` footer, because release-plz cannot cap a major bump. A breaking change is a `feat` (or a `fix`) whose body states the break and whose `Migration:` footers name each consumer's migration:

```text
feat(resolver): return contexts in declaration order

`ResolvedContexts::iter` no longer sorts by name; callers that relied on the order must sort.

Migration: RayX: sort the contexts in crates/rayx_theme/src/loader.rs before building the picker
Migration: RayX.TokenEditor: none needed, the editor already sorts
```

The changelog prints each `Migration:` footer under its entry, so it reaches the Release notes. `semver_check = false` for the same reason. The policy ends when a second consumer appears; then `!` and major bumps return.

## Before merging a release pull request

- The proposed version is still `1.x`. A `2.0.0` proposal means a stray `!` or `BREAKING CHANGE:` slipped in: reword that commit instead of merging.
- The pull request changes only `Cargo.toml`, `Cargo.lock` and `CHANGELOG.md`.
- The changelog section reads well. Fix a bad line on the pull request right before merging; when `main` moves on, release-plz replaces a release pull request that carries human commits.

The pull request runs only `ci.yml` (formatting, Clippy, tests, the wasm check and `cargo package` on Linux and Windows). `release-plz.yml` runs no tests.

## The RELEASE_PLZ_TOKEN secret

Events made with the default `GITHUB_TOKEN` start no other workflows, so a release pull request opened with it would get no CI. release-plz therefore uses a fine-grained personal access token stored as the Actions secret `RELEASE_PLZ_TOKEN`, shared by the dXn libraries: owner `dXnSix`, access to the library repositories, **Contents** and **Pull requests** read and write, an expiry of at most one year.

:::warning Expiry fails silently
When the token expires, release-plz runs turn red at checkout or push, no release pull request is updated and no tag is pushed; releases stop without a loud failure. Rotate before the expiry date with the same settings.
:::

## Tags cannot move

The tag ruleset "Release tags" covers `refs/tags/v*`: tags can be created but not updated, deleted or force-moved. A bad release is fixed forward with a new version.

## By-hand fallback

If release-plz cannot cut a release, the owner releases by hand on an up-to-date `main`:

```bash
# 1. Set X.Y.Z in [workspace.package] version and in both [workspace.dependencies]
#    requirements (dxn_dtcg, dxn_dtcg_derive) of the root Cargo.toml, then:
cargo update --workspace
# 2. Add a `## [X.Y.Z] - YYYY-MM-DD` section to CHANGELOG.md, right below the header
# 3. Commit and push it, then tag that commit and push the tag
git tag vX.Y.Z
git push origin vX.Y.Z
# 4. Save that section's body to notes.md and create the GitHub Release with it
gh release create vX.Y.Z --repo dXnSix/dXn.Dtcg --title vX.Y.Z --notes-file notes.md
```

release-plz continues from the by-hand tag on its next run. Implementation work goes straight to `main`, rebased on the remote first (`git fetch origin && git rebase origin/main`, then `git push --force-with-lease origin main`); there are no feature branches and no pull requests except release-plz's.
