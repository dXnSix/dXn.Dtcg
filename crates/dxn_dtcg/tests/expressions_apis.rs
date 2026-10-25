//! Every API that resolves values evaluates expressions alike, affected
//! resolution re-evaluates what an edit reaches, and exports round-trip.
#![cfg(feature = "expressions")]

use dxn_dtcg::{
    DtcgRuntime, DtcgTokens, EXPRESSION_EXTENSION, Resolution, ResolvedTokens, RuntimeOptions,
    expressions::ExpressionOptions,
};
use serde_json::{Value, json};

fn options() -> RuntimeOptions {
    RuntimeOptions::default().with_expressions(ExpressionOptions::default())
}

fn scaled(input: &str, factor: f64) -> Value {
    json!({
        "$type": "number",
        "$value": 0,
        "$extensions": { EXPRESSION_EXTENSION: { "inputs": { "x": format!("{{{input}}}") }, "js": format!("x * {factor}") } }
    })
}

/// `size.base` (4), `size.large` = base × 2, `size.huge` = large × 2,
/// `size.alias` → huge, and an unrelated `other`.
fn tokens_document(base: u32) -> Value {
    json!({
        "size": {
            "base": { "$type": "number", "$value": base },
            "large": scaled("size.base", 2.0),
            "huge": scaled("size.large", 2.0),
            "alias": { "$type": "number", "$value": "{size.huge}" }
        },
        "other": { "$type": "number", "$value": 1 }
    })
}

fn value(tokens: &ResolvedTokens, path: &str) -> Value {
    tokens.get(path).unwrap().value.clone()
}

#[derive(Debug, DtcgTokens, PartialEq)]
#[dtcg(root = "size")]
struct Sizes {
    base: f64,
    large: f64,
    huge: f64,
    alias: f64,
}

#[test]
fn resolve_and_project_evaluate_expressions() {
    let runtime = DtcgRuntime::from_json_with_options(
        "tokens.json",
        tokens_document(4).to_string(),
        options(),
    )
    .unwrap();
    let tokens = runtime.resolve(&[]).unwrap().ready().unwrap();
    assert_eq!(value(&tokens, "size.large"), json!(8));
    assert_eq!(value(&tokens, "size.huge"), json!(16));
    assert_eq!(value(&tokens, "size.alias"), json!(16));
    assert_eq!(
        runtime.project::<Sizes>(&[]).unwrap().ready(),
        Some(Sizes {
            base: 4.0,
            large: 8.0,
            huge: 16.0,
            alias: 16.0
        })
    );
}

fn resolver_runtime() -> DtcgRuntime {
    let resolver = json!({
        "version": "2025.10",
        "sets": { "base": { "sources": [{ "$ref": "tokens.json" }] } },
        "resolutionOrder": [{ "$ref": "#/sets/base" }]
    });
    let mut runtime = DtcgRuntime::with_options(options());
    runtime
        .provide_json("root.resolver.json", resolver.to_string())
        .unwrap();
    runtime
        .provide_json("tokens.json", tokens_document(4).to_string())
        .unwrap();
    runtime.set_root("root.resolver.json").unwrap();
    runtime
}

#[test]
fn previews_evaluate_expressions() {
    let runtime = resolver_runtime();
    let draft = json!({ "size": { "base": { "$type": "number", "$value": 10 } } });
    let preview = runtime
        .preview_source(&draft, &[])
        .unwrap()
        .ready()
        .unwrap();
    assert_eq!(value(&preview, "size.large"), json!(20));
    assert_eq!(value(&preview, "size.alias"), json!(40));

    let draft = json!({ "size": { "large": scaled("size.base", 3.0) } });
    let occurrences = runtime
        .inspect_source(&draft, &[])
        .unwrap()
        .ready()
        .unwrap();
    let preview = runtime
        .preview_occurrence(&occurrences[0], &[])
        .unwrap()
        .ready()
        .unwrap();
    assert_eq!(value(&preview, "size.large"), json!(12));
    assert_eq!(value(&preview, "size.huge"), json!(24));
    assert_eq!(preview.get("size.large").unwrap().snapshot, Some(json!(0)));
}

/// Applies an edit through affected resolution and compares it with a full
/// resolution of the edited documents; returns the re-resolved paths.
fn edit(document: Value, changed: &[&str]) -> Vec<String> {
    let mut runtime = DtcgRuntime::from_json_with_options(
        "tokens.json",
        tokens_document(4).to_string(),
        options(),
    )
    .unwrap();
    let mut tokens = runtime.resolve(&[]).unwrap().ready().unwrap();
    let Resolution::Ready(affected) = runtime
        .replace_json_and_resolve_affected("tokens.json", document.to_string(), &[], changed)
        .unwrap()
    else {
        panic!("the document is complete");
    };
    tokens.apply_affected(&affected);
    let full = runtime.resolve(&[]).unwrap().ready().unwrap();
    assert_eq!(tokens, full);
    assert_eq!(tokens.fingerprint(), full.fingerprint());
    affected
        .iter()
        .map(|token| token.path.to_string())
        .collect()
}

#[test]
fn editing_an_input_target_re_evaluates_its_dependents() {
    let affected = edit(tokens_document(5), &[]);
    assert_eq!(
        affected,
        ["size.base", "size.large", "size.huge", "size.alias"]
    );
    let affected = edit(tokens_document(5), &["size.base"]);
    assert_eq!(
        affected,
        ["size.base", "size.large", "size.huge", "size.alias"]
    );
}

#[test]
fn editing_an_expression_object_re_evaluates_it_and_its_dependents() {
    let mut document = tokens_document(4);
    document["size"]["huge"] = scaled("size.large", 3.0);
    assert_eq!(edit(document, &[]), ["size.huge", "size.alias"]);
}

#[test]
fn editing_an_unrelated_token_re_evaluates_nothing_else() {
    let mut document = tokens_document(4);
    document["other"]["$value"] = json!(2);
    assert_eq!(edit(document, &[]), ["other"]);
}

#[test]
fn exports_round_trip_with_expressions_on_and_off() {
    let runtime = DtcgRuntime::from_json_with_options(
        "tokens.json",
        tokens_document(4).to_string(),
        options(),
    )
    .unwrap();
    let tokens = runtime.resolve(&[]).unwrap().ready().unwrap();
    let exported = tokens.to_document();
    assert_eq!(exported["size"]["large"]["$value"], json!(8));
    assert_eq!(
        exported["size"]["large"]["$extensions"],
        tokens_document(4)["size"]["large"]["$extensions"]
    );

    for options in [options(), RuntimeOptions::default()] {
        let again =
            DtcgRuntime::from_json_with_options("export.json", exported.to_string(), options)
                .unwrap()
                .resolve(&[])
                .unwrap()
                .ready()
                .unwrap();
        for token in tokens.iter() {
            assert_eq!(
                again.get(&token.path).unwrap().value,
                token.value,
                "{}",
                token.path
            );
            assert_eq!(again.get(&token.path).unwrap().extensions, token.extensions);
        }
    }
}
