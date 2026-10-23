---
agentx:
  kind: decision
  scope: decisions/repository/packaging
  status: current
  created: 2026-10-23T22:00:00+02:00
  updated: 2026-10-23T22:00:00+02:00
  topics:
    - naming
    - release
    - rayx-consumer
  supersedes: []
  superseded_by: []
---

## Decision: The crates are `dxn_dtcg` and `dxn_dtcg_derive` in `dXnSix/dXn.Dtcg`, released from `v1.1.0`

### Context: On 2026-10-23 the owner moved RayX.Dtcg, RayX.Reactive and RayX.SignalR from the `rayx-ui` organization to the `dXnSix` account as dXn.Dtcg, dXn.Reactive and dXn.SignalR (GitHub redirects the old URLs) and asked for crate names starting with `dxn`; the owner chose `dxn_dtcg` and `dxn_dtcg_derive` over prefixing the old names. RayX stays in `rayx-ui` (its repository is now `RayX.Old`, checked out at `../../RayX/RayX.Old`) and remains the first consumer.

### Alternatives considered: Keep `dtcg_runtime`/`dtcg_runtime_derive`; prefix them (`dxn_dtcg_runtime`, `dxn_dtcg_runtime_derive`); keep re-exporting shims under the old names.

### Reasoning: The runtime is named after its repository, as `dxn_reactive` and `dxn-signalr` are, and the crate directories follow the package names (`crates/dxn_dtcg`, `crates/dxn_dtcg_derive`). The rename breaks consumers, so under the [git-tag release decision](2026-10-18-repository-packaging-release-plz-git-tag-releases.md) (2026-10-18, current) it is a `feat` with a `Migration:` footer and ships as `v1.1.0`. release-plz's git-only mode looks the packages up by their new names inside `v1.0.0` and cannot compute a next version, so `v1.1.0`'s version bump and changelog section were committed by hand; from `v1.1.0` on, release-plz finds the packages again.

### Trade-offs accepted: Superseded decisions, `.agentx/tasks/` epics, the `1.0.0` changelog section and RayX's former `crates/rayx/dtcg_runtime*` paths keep the old names; read `dtcg_runtime` and `rayx-ui/RayX.Dtcg` there as `dxn_dtcg` and `dXnSix/dXn.Dtcg`. The RayX value extensions keep their names (`dev.rayx.blur`, `ValueExtensions::rayx`): they name RayX's token format, not this library. `RELEASE_PLZ_TOKEN` must now have `dXnSix` as its resource owner.

### Supersedes: None.
