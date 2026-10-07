---
agentx:
  kind: state
  scope: knowledge/runtime/source-inspection
  status: current
  created: 2026-10-05T18:00:00+02:00
  updated: 2026-10-07T12:00:00+02:00
  topics:
    - source-inspection
    - provenance
    - preview
    - diagnostics
  supersedes: []
  superseded_by: []
---
# Source Inspection State

## Purpose And Usage

Read before changing inspection, preview or diagnostics APIs, which editors rely on to show and relocate authored sources.

## Current Technical Shape

`source_references()` lists every authored resolver-source reference, including dormant sets, contexts and shallow overrides, without resolving values or touching aliases and unknown extensions, so references can be relocated. Reference objects are walked as discovery overlays them (a definition an override replaces is not visited), every reached resolver passes the same structure rules, and each site is listed once by where its `$ref` is written. The walk is linear in the documents' size: scopes are shared, targets already walked with the same overrides are skipped before copying, targets without source sites (token documents) are never walked, and only repeated walks of a location under different overrides count toward `max_documents`, in discovery's units (steps for expanded resolvers and followed references, separately sources for inline token sources), so every document `validate_complete` accepts is listed unless repeated walks pass through dormant definitions. `inspect_source(source)` returns `SourceOccurrence`s in the root resolver's reference scope: ordered duplicates, empty groups and override owners are kept; each occurrence has a `SourceOrigin` (resource, `None` for the supplied inline source, plus a JSON pointer) and per-member origins. `validate_source` discovers dependencies in every context and validates structure, not values. `preview_source` resolves with the supplied source's occurrences applied last, and `preview_occurrence` previews one terminal occurrence, even a shadowed one inside a referenced resolver; both use the same discovery, ordering, alias, inheritance and limit rules as `resolve` and never mutate the catalog. `set_sources` and `modifier_contexts` read a resolver's set sources and modifier contexts without traversing references.

The `diagnostics` feature adds `DtcgDiagnostic`/`DtcgDiagnosticCode` (stable categories derived from `DtcgError`, holding logical ids and paths, never document bodies or JSON values) and `AffectedResolution::nodes_visited()`.

## Backlinks

- [Inspection entry points](../../../../../crates/dtcg_runtime/src/runtime.rs)
- [Source discovery](../../../../../crates/dtcg_runtime/src/source_discovery.rs)
- [Source references](../../../../../crates/dtcg_runtime/src/source_references.rs)
- [Occurrences](../../../../../crates/dtcg_runtime/src/source_occurrence.rs)
- [Diagnostics](../../../../../crates/dtcg_runtime/src/diagnostics/mod.rs)
- [Inspection tests](../../../../../crates/dtcg_runtime/tests/source_inspection.rs)
- [Reference-site bounds tests](../../../../../crates/dtcg_runtime/tests/source_reference_bounds.rs)
- [Reference-site parity tests](../../../../../crates/dtcg_runtime/tests/source_reference_parity.rs)
