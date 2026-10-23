# Releasing `dxn_dtcg`

A release is a version tag `vX.Y.Z` on `main` plus a GitHub Release whose notes are that version's
`CHANGELOG.md` section. Both crates, `dxn_dtcg` and `dxn_dtcg_derive`, share one version,
one tag, one GitHub Release and one root `CHANGELOG.md`; `dxn_dtcg_derive` never gets a tag or
a Release of its own. Nothing is published to crates.io, although the manifests stay packageable
(`cargo package --workspace` keeps passing).

release-plz does all of it (`release-plz.toml`, `.github/workflows/release-plz.yml`) in git-only
mode (`git_only = true`, `publish = false`): versions come from `v*` tags, not from a registry.

- [How a release happens](#how-a-release-happens)
- [Commit types and the 1.x policy](#commit-types-and-the-1x-policy)
- [Before merging a release pull request](#before-merging-a-release-pull-request)
- [The changelog](#the-changelog)
- [CI on a release pull request](#ci-on-a-release-pull-request)
- [The `RELEASE_PLZ_TOKEN` secret](#the-release_plz_token-secret)
- [Tags cannot move](#tags-cannot-move)
- [By-hand fallback](#by-hand-fallback)
- [Consuming a release](#consuming-a-release)
- [Pushing to `main`](#pushing-to-main)

## How a release happens

1. Commit to `main` with [Conventional Commits](https://www.conventionalcommits.org/). The commit
   types decide the next version (see below).
2. On every push to `main` the Release-plz workflow's `release-pr` job opens or updates the
   release pull request (head branch `release-plz-…`): the version bump in `Cargo.toml` (the
   `[workspace.package]` version and both `[workspace.dependencies]` requirements), `Cargo.lock`
   and a new `CHANGELOG.md` section.
3. Review it (see [before merging](#before-merging-a-release-pull-request)) and merge it.
4. On that push the `release` job finds a version in `Cargo.toml` with no tag, pushes `vX.Y.Z` and
   creates the GitHub Release with the new changelog section as its notes.

The first release, `v1.0.0`, was tagged without a pull request: `Cargo.toml` already said `1.0.0`
and no `v*` tag existed, and in git-only mode release-plz treats a package without a matching tag
as an initial release, so the `release` job tagged it on the first push that contained the
workflow.

`v1.1.0`, which renamed the crates from `dtcg_runtime`/`dtcg_runtime_derive` to
`dxn_dtcg`/`dxn_dtcg_derive`, was cut with the [by-hand fallback](#by-hand-fallback): in git-only
mode release-plz looks the packages up by their new names inside `v1.0.0`, finds nothing and
cannot compute a next version. From `v1.1.0` on it finds them again.

## Commit types and the 1.x policy

| Commit | Release |
| --- | --- |
| `fix:`, `perf:` | patch (`1.0.0` → `1.0.1`) |
| `feat:` | minor (`1.0.0` → `1.1.0`) |
| `docs:`, `test:`, `ci:`, `build:`, `chore:`, `refactor:` | none on their own (`release_commits = "^(feat\|fix\|perf)"`); they are listed in the next release's changelog |

`chore(agentx)`, `chore(release)` and `chore(deps)` commits are left out of the changelog.

**1.x policy.** The owner is the only consumer, so the crates stay on `1.x`. No commit is marked
breaking: no `!` after the type or scope and no `BREAKING CHANGE:` footer. A breaking change is a
`feat` (or a `fix`, when it is one) whose body states the break and whose `Migration:` footers name
each consumer's migration (RayX, RayX.TokenEditor, token files):

```text
feat(resolver): return contexts in declaration order

`ResolvedContexts::iter` no longer sorts by name; callers that relied on the order must sort.

Migration: RayX: sort the contexts in crates/rayx_theme/src/loader.rs before building the picker
Migration: RayX.TokenEditor: none needed, the editor already sorts
```

The changelog prints each `Migration:` footer under its entry, so the migration reaches the GitHub
Release notes. `semver_check = false` in `release-plz.toml` because release-plz cannot cap a major
bump: a `!`, a `BREAKING CHANGE:` footer or an API break found by cargo-semver-checks would all
propose `2.0.0`.

The policy ends when a second consumer appears; then `!` and major bumps return, and
`semver_check` is turned back on.

## Before merging a release pull request

- The proposed version is still `1.x`. A `2.0.0` proposal means a stray `!` or `BREAKING CHANGE:`
  slipped into a commit: reword that commit (`git rebase -i`, force push allowed) instead of
  merging; release-plz then recomputes the pull request.
- The pull request changes only `Cargo.toml`, `Cargo.lock` and `CHANGELOG.md`.
- The changelog section reads well (see below).

## The changelog

`CHANGELOG.md` is written by release-plz from the commits (git-cliff, the `[changelog]` section of
`release-plz.toml`): no hand edits in feature commits. A line that reads badly is fixed on the
release pull request right before merging it. Do it last: when `main` moves on, release-plz
replaces a release pull request that carries human commits.

The `1.0.0` section is the hand-written history up to the first release; generated sections start
with the next version.

## CI on a release pull request

A release pull request runs only `ci.yml`, like any other pull request: one job on `ubuntu-latest`
and `windows-latest` with `cargo fmt --check`, Clippy, `cargo test --workspace --all-features`, the
`wasm32-unknown-unknown` check and `cargo package --workspace` (a few minutes). There are no e2e,
performance or larger matrix suites to filter. `ci.yml` also runs on every push to `main`;
`release-plz.yml` runs no tests.

## The `RELEASE_PLZ_TOKEN` secret

Events made with the workflow's default `GITHUB_TOKEN` start no other workflows, so a release pull
request opened with it would get no CI. release-plz therefore checks out and pushes with a
fine-grained personal access token stored as the Actions secret `RELEASE_PLZ_TOKEN` (a repository
secret, or an organization secret shared by the library repositories). The crates never read it.

It is one PAT shared by the dXn libraries:

- resource owner `dXnSix` (the account that owns the repositories);
- repository access: the three library repositories (dXn.Dtcg, dXn.Reactive, dXn.SignalR);
- repository permissions: **Contents** read and write, **Pull requests** read and write (Metadata
  read is added automatically);
- an expiry of at most one year, with the date noted.

Release pull requests, tags, Releases and release-plz's commits appear as the token's owner.

**When it expires, nothing fails loudly:** the Release-plz runs turn red at checkout or push, no
release pull request is updated and no tag is pushed; releases silently stop until the token is
replaced. The same happens when the token's repository access does not include this repository.
Rotate before the expiry date: generate a new token with the same settings and replace the
secret's value; nothing else changes.

## Tags cannot move

The tag ruleset "Release tags" covers `refs/tags/v*`: tags can be created (release-plz does), but
not updated, deleted or force-moved, and nobody can bypass it. A bad release is fixed forward with a
new version, never by moving or deleting its tag.

## By-hand fallback

If release-plz cannot cut a release (a broken workflow, an expired token you cannot replace yet),
the owner releases by hand on an up-to-date `main` (tags are pushed only by release-plz or the
owner):

```bash
# 1. Set X.Y.Z in [workspace.package] version and in both [workspace.dependencies]
#    requirements (dxn_dtcg, dxn_dtcg_derive) of the root Cargo.toml, then:
cargo update --workspace
# 2. Add a `## [X.Y.Z] - YYYY-MM-DD` section to CHANGELOG.md, right below the header
# 3. Commit and push it (see "Pushing to main"), then tag that commit and push the tag
git tag vX.Y.Z
git push origin vX.Y.Z
# 4. Save that section's body to notes.md and create the GitHub Release with it
gh release create vX.Y.Z --repo dXnSix/dXn.Dtcg --title vX.Y.Z --notes-file notes.md
```

release-plz continues from the by-hand tag on its next run.

## Consuming a release

Consumers depend on a release tag, never on a branch, a path or crates.io:

```toml
[dependencies]
dxn_dtcg = { git = "https://github.com/dXnSix/dXn.Dtcg", tag = "vX.Y.Z" }
```

`Cargo.lock` records the exact commit. Bump the tag by hand to take a new release; the latest one is
on the repository's Releases page. `dxn_dtcg` re-exports the derive macros, and
`dxn_dtcg_derive` resolves from the same tag. A change RayX needs is released here first; then
RayX bumps its tag and validates its affected crates and Test Suite specs.

## Pushing to `main`

Implementation work goes straight to `main`, always rebased on the remote first:

```bash
git fetch origin && git rebase origin/main
git push --force-with-lease origin main
```

`--force-with-lease` refuses to overwrite commits you have not fetched, so a merged release pull
request or a tagged commit is never dropped. There are no feature branches and no pull requests
except release-plz's release pull requests.
