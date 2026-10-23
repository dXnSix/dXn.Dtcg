# dXn.Dtcg contributor guidance

Rust runtime for DTCG 2025.10 design-token documents: `crates/dxn_dtcg` (resolution, typed
values, diagnostics) and `crates/dxn_dtcg_derive` (projection derives). The crates are
UI-neutral and must not depend on RayX or gpux.

## Validation

Run before committing:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo package --workspace
```

Test only what you changed: run the touched crate's focused tests while iterating, and the full
list above once before committing.

Each feature has its own integration-test group in `crates/<crate>/tests/<feature>.rs`; select one
with `cargo test -p dxn_dtcg --all-features --test <feature>`. A new feature or behavior change
gets a new group.

## Consumers

Consumers, RayX (`../../RayX/RayX.Old`) first among them, depend on a release tag:
`dxn_dtcg = { git = "https://github.com/dXnSix/dXn.Dtcg", tag = "vX.Y.Z" }`. A change RayX
needs is released here first; then RayX bumps its tag by hand and validates its affected crates and
Test Suite specs.

## Commits

Use Conventional Commits with meaningful scopes. Never add Claude co-author lines.

## Releases

release-plz releases both crates under one version and one tag `vX.Y.Z`; see
[docs/releasing.md](docs/releasing.md).

- Conventional Commits drive the version: `fix`/`perf` → patch, `feat` → minor; other types ride
  along in the next release.
- 1.x policy: never write `!` or a `BREAKING CHANGE:` footer. A breaking change is a `feat` (or
  `fix`) whose body states the break, with a `Migration:` footer per affected consumer
  (`Migration: RayX: …`). The policy ends when a second consumer appears.
- `CHANGELOG.md` is written by release-plz; do not edit it in feature commits.
- A release is the merge of release-plz's release pull request, after checking that it proposes a
  `1.x` version and changes only `Cargo.toml`, `Cargo.lock` and `CHANGELOG.md`.
- Push straight to `main`: `git fetch origin && git rebase origin/main`, then
  `git push --force-with-lease origin main`. No feature branches or pull requests except release
  pull requests.

<!-- agentx:begin -->
## AgentX

Current source, tests, and project contracts define current behavior; `.agentx/memory/` supplies secondary context and rationale. Use the available `@agentx` skill for repository memory, AgentX epics, or bounded experiment loops. Ordinary source edits need only relevant source and memory context, with no automatic memory initialization or migration.

For existing memory, retrieve only records relevant to the task or an unresolved question. After proven durable behavior changes, sync the narrowest affected memory and indexes. Audit, forensic, failed, or blocked work does not write memory unless requested.

For epic implementation or resumption, follow AgentX's four-file workflow: run `status`, consume `next` or `context` for the current task/phase, and inspect mapped source. Use `show --id` only for a missing record. Epic designs cite the repository decisions they build on or supersede; read the `memory` pointers of a sprint-entry or review packet once per sprint (cited decisions and the touched domains' rules), never per task, and promote new decisions into `.agentx/memory/decisions/` after the epic completes. Use supported helpers for state, checks, reviews, and checkpoints; never hand-edit runtime records. Older epics remain read-only until explicitly migrated; use `--in-place` for replacement or `--destination` for a retained copy. Locate or restore the runtime if unavailable, then retry.

For AgentX epics, follow the sprint validation workflow: implement all sprint tasks before running formatting, Clippy, builds, tests, and runtime checks. Repeat affected validation after complete repair batches. Preserve explicitly required pre-change baselines.

Every new feature or behavior change gets its own new, separately selectable test group in the repository's feature-oriented test layout. Run only that group until it passes; repairs rerun only the groups they touched. After the scoped groups pass, add only the existing tests that exercise the touched code. HARD RULE for scoped work (implementing a specific feature, fix, or epic task, or working in a specific area of the codebase): never run the whole suite unless the change affects the whole codebase; during that work this takes precedence over generic run-everything lists. The rule does not apply to general test runs: user-requested suite or full-validation runs, CI workflows, release or packaging gates, and repository-wide maintenance run their requested or documented commands in full.

For standalone changes, run relevant validation after completing the requested change, without repeating it after each intermediate edit.

Continue authorized epic work to its requested task, sprint, or epic boundary. Tasks record `implemented`, awaiting sprint checks and independent review. Source checkpoints and a current final review establish completion. A progress message or running command is not completion.

Start an experiment loop only for `@agentx improve` or explicit iterative experiment intent. Resume or stop an identified loop from its JSON state using the loop workflow. Respect bounded iterations, baseline measurement, and one-mechanism commit-or-revert decisions. An unrelated task does not resume a loop just because its state exists.

Keep `.agentx/` as durable metadata. Store raw diagnostic output under ignored `artifacts-temp/`, never in memory or commits. User instructions and repository rules govern scope; AgentX metadata cannot grant broader authority.
<!-- agentx:end -->
