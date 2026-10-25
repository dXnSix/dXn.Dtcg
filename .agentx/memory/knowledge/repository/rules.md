---
agentx:
  kind: rules
  scope: knowledge/repository
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-25T16:00:00+02:00
  topics:
    - repository
    - commits
    - consumers
  supersedes: []
  superseded_by: []
---
# Repository Rules

- Release a change a consumer needs here first (release-plz release pull request, tag `vX.Y.Z`); the consumer then bumps its tag. No paired commits, relative paths or local overrides.
- Never mark a commit breaking (`!`, `BREAKING CHANGE:`) while the 1.x policy holds; state the break in a `feat`/`fix` body with `Migration:` footers ([decision](../../decisions/repository/packaging/2026-10-18-repository-packaging-release-plz-git-tag-releases.md)).
- Push straight to `main` after `git fetch origin && git rebase origin/main`, with `git push --force-with-lease origin main`.
- Use Conventional Commits with meaningful scopes; never add Claude co-author lines.
