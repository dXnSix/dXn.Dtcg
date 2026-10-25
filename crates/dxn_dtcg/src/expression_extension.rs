/// The `$extensions` key of token expressions (`dxn.dtcg.expressions`).
///
/// On a token, it holds the expression that computes the token's value: `{
/// "inputs"?: { name: <DTCG value> }, "imports"?: { specifier: [name, …] |
/// namespace }, "js" | "ts": "<expression>" }`. At a token document's root
/// `$extensions`, it may declare the document's JavaScript and TypeScript
/// modules: `{ "modules": { specifier: "relative/reference.js" } }`.
///
/// The token keeps its `$value`, the DTCG-valid snapshot every tool reads
/// (Format §5.2.3, §8). Only a runtime built with the `expressions` feature
/// and configured with `RuntimeOptions::with_expressions` reads the
/// extension; everywhere else it is opaque metadata.
pub const EXPRESSION_EXTENSION: &str = "dxn.dtcg.expressions";
