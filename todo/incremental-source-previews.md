# Previews and selections resolve every token again after an edit (performance)

Found while improving Six.TokenEditor (tag `v1.3.0`, release build, macOS arm64).

## Symptom

An editor that validates an edit the way a strict DTCG tool must — the default resolution, plus a preview of every
source the open editors show (each modifier context, each shadowed source) — pays a full resolution per preview, even
when the edit changed one token's value. `replace_json_and_resolve_affected` re-resolves only the affected closure,
but only for the root resolution and one selection; `preview_source`, `preview_occurrence`, `inspect_source` and
`resolve` with another selection always resolve the whole merged structure.

**Repro.** A project of 4,000 color tokens in one set (`base.tokens.json`, a `$ref` source) and a `theme` modifier
with two contexts of 2,000 aliases each into it, the theme last in the resolution order. Change one primitive's value
and validate as Six.TokenEditor does (`ProjectResolution::candidate`): compile, `resolve(&[])`, then
`preview_source` of each source the editors show (`base`, `theme/light`, `theme/dark`).

| Step | Time |
| --- | --- |
| Compile (provide and validate every document) | ~12 ms |
| `resolve(&[])` | ~16 ms |
| `preview_source` × 3 (in parallel on native) | ~20–30 ms (~60 ms one after another) |
| Re-projecting the edited set afterwards (`inspect_source` + `preview_occurrence`) | ~30 ms |
| **One value edit, end to end** | **~77 ms** (126 ms before the editor parallelized) |

Reusing the compiled runtime and re-providing only the changed document (`provide_json` on a clone) saved ~5 ms of
compile time but nothing measurable end to end: the time is in the full resolutions. Only one edit path avoids them,
`replace_json_and_resolve_affected`, and only for a root document that the default resolution covers.

## Cause (as understood from the outside)

Each `preview_*` call and each `resolve` with a selection runs discovery, the merge and resolution from scratch.
Nothing is kept between calls: no merged structure per selection or per previewed source, no resolved tokens to update
from the changed paths, as `replace_json_and_resolve_affected` does for the root resolution.

## Workaround in the app

Six.TokenEditor runs the default resolution and the source previews of a candidate on scoped threads on native targets
(sequentially on wasm32), and projects a modifier's contexts in parallel. That brought an edit from 126 ms to 77 ms
on the project above; the remaining time is the full resolutions themselves.

## Suggested API

- A cache of prepared selections and previews in `DtcgRuntime` (or a `PreparedPreview` handle the caller keeps) that
  `replace_json_and_resolve_affected` updates alongside the root resolution, so a preview after an edit re-resolves
  only the changed tokens and their dependents.
- Or an affected variant of the previews:
  `preview_source_affected(&self, source, selection, previous: &ResolvedTokens, changed_paths) ->
  Resolution<AffectedResolution>` (and the same for `preview_occurrence`), taking the previous preview and the paths an
  edit changed, as `replace_json_and_resolve_affected` takes them for the root.
- `replace_json_and_resolve_affected` for any loaded document (not only the root) under several selections at once,
  returning one `AffectedResolution` per selection, so an editor validating every modifier context does one call per
  edit.
- Cheaper compile for an unchanged root: `DtcgRuntime::clone` already shares documents; documenting that re-providing a
  document on a clone (`replace_json`) keeps the other documents' validation would let callers skip recompiling.
