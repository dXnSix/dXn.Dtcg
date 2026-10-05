# RayX.Dtcg

Rust runtime for [Design Tokens Community Group](https://www.designtokens.org/) (DTCG 2025.10)
documents: string-backed resolution of token documents, resolvers, modifiers and aliases, with
narrow typed projection into Rust structs through `#[derive(DtcgTokens)]` and
`#[derive(DtcgComponentTokens)]`.

| Crate | Purpose |
|---|---|
| [`dtcg_runtime`](crates/dtcg_runtime) | Document catalog, resolver, typed values, diagnostics (`diagnostics` feature) |
| [`dtcg_runtime_derive`](crates/dtcg_runtime_derive) | Derive macros for typed token projection |

It also understands the RayX extension type `dev.rayx.blur` used by the RayX Figma export.

```toml
[dependencies]
dtcg_runtime = "0.1"
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT)
at your option.
