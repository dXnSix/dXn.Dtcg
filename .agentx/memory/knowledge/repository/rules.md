---
agentx:
  kind: rules
  scope: knowledge/repository
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-18T23:30:00+02:00
  topics:
    - repository
    - commits
    - rayx-consumer
  supersedes: []
  superseded_by: []
---
# Repository Rules

- Release a change RayX needs here first (release-plz release pull request, tag `vX.Y.Z`); RayX then bumps its tag and validates its affected crates and Test Suite specs. No paired commits, relative paths or local overrides.
- Never mark a commit breaking (`!`, `BREAKING CHANGE:`) while the 1.x policy holds; state the break in a `feat`/`fix` body with `Migration:` footers ([decision](../../decisions/repository/packaging/2026-10-18-repository-packaging-release-plz-git-tag-releases.md)).
- Push straight to `main` after `git fetch origin && git rebase origin/main`, with `git push --force-with-lease origin main`.
- Use Conventional Commits with meaningful scopes; never add Claude co-author lines.
