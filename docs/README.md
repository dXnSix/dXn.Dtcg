# dXn.Dtcg documentation

This is the technical documentation for `dxn_dtcg` and `dxn_dtcg_derive`, a Rust runtime
for [DTCG 2025.10](https://www.designtokens.org/) design-token documents. For a short
introduction and quick examples, start with the [project README](../README.md).

## Chapters

| # | Chapter | What it covers |
|---|---|---|
| 1 | [Overview](01-overview.md) | What the runtime does, DTCG concepts, the resolution pipeline, design principles, platform support, the opt-in RayX conveniences, glossary |
| 2 | [Getting started](02-getting-started.md) | A tutorial: from one token file to themed, typed tokens loaded on demand |
| 3 | [Token documents](03-token-documents.md) | Structure rules, token paths, names, reserved properties, `$schema`, the merged document, type inheritance, `$root`, `$extends`, metadata and groups, ordering, export with `to_document` |
| 4 | [Aliases and references](04-aliases-and-references.md) | `{curly.brace}` aliases and where they may appear, type determination and reference type checks, JSON Pointer `$ref`s and their syntax, cycles and limits |
| 5 | [Value types](05-value-types.md) | The 13 DTCG types, the exact validation rules, strict defaults and the opt-in RayX extensions, the typed `values` module |
| 6 | [Resolvers and modifiers](06-resolvers-and-modifiers.md) | Resolver documents, sets, modifiers, sources, merging sources, selections and inputs, nested resolvers |
| 7 | [Document loading](07-document-loading.md) | The catalog, logical resource ids, the request/provide protocol, loading strategies, envelopes, path safety |
| 8 | [Typed projection](08-typed-projection.md) | `#[derive(DtcgTokens)]`, attributes, `FromDtcgValue`, custom conversions, component tokens |
| 9 | [Incremental resolution](09-incremental-resolution.md) | Fingerprints, catalog revisions, re-resolving only affected tokens |
| 10 | [Source inspection](10-source-inspection.md) | Authored-source inspection, provenance, previews and reference sites for editors |
| 11 | [Errors, limits and diagnostics](11-errors-limits-and-diagnostics.md) | `DtcgError`, `Resolution::Missing`, `RuntimeLimits`, the `diagnostics` feature |
| 12 | [DTCG 2025.10 conformance](12-conformance.md) | What is supported, extensions, implementation-defined behavior, what is not supported |
| 13 | [API reference](13-api-reference.md) | Every public type, trait, function and method |
| 14 | [Architecture and contributing](14-architecture-and-contributing.md) | Source layout, pipeline internals, invariants, tests, CI and releases |

## Reading paths

- **Using the crate in an application:** chapters 1, 2, 6, 7 and 8, with 11 for error handling.
- **Writing token files and resolvers:** chapters 3, 4, 5 and 6, with 12 for the edge cases.
- **Building a token editor or linter:** chapters 9 and 10 on top of the application path.
- **Contributing to the crates:** chapter 14, after chapter 1.

## Conventions

- Rust examples are complete programs or self-contained snippets written against the current API.
  Examples that touch the file system are marked `no_run`.
- JSON examples are DTCG documents. When a document is embedded in Rust, it uses a raw string
  (`r#"..."#`, or `r##"..."##` when the JSON contains `"#`, as JSON Pointers like `"#/sets/base"`
  do).
- *Token path* means the dotted path the runtime uses to identify a token, such as
  `color.brand.primary`. *JSON Pointer* means an RFC 6901 pointer such as `#/color/brand/$value`.
