//! Tokens with the same expression text share one compiled module whose
//! name comes from the text, so nothing about it depends on which token
//! compiled it first.
#![cfg(feature = "expressions")]

use dxn_dtcg::{
    DtcgRuntime, EXPRESSION_EXTENSION, Resolution, RuntimeOptions, expressions::ExpressionOptions,
};
use serde_json::json;

#[test]
fn affected_resolution_matches_a_full_one_for_shared_expression_text() {
    // Same text, different inputs: an edit of `src` re-evaluates only `b`,
    // which then compiles the shared module first.
    let expression = |input: &str| {
        json!({ "$type": "fontFamily", "$value": "x", "$extensions": { EXPRESSION_EXTENSION: {
            "inputs": { "source": input }, "js": "String(source) + new Error().stack"
        } } })
    };
    let document = |source: u32| {
        json!({
            "src0": { "$type": "number", "$value": 0 },
            "src": { "$type": "number", "$value": source },
            "a": expression("{src0}"),
            "b": expression("{src}"),
        })
        .to_string()
    };
    let mut runtime = DtcgRuntime::from_json_with_options(
        "tokens.json",
        document(1),
        RuntimeOptions::default().with_expressions(ExpressionOptions::default()),
    )
    .unwrap();
    let mut tokens = runtime.resolve(&[]).unwrap().ready().unwrap();
    let stack = tokens.get("b").unwrap().value.as_str().unwrap().to_owned();
    assert!(stack.contains("dxn.dtcg/expression/"), "{stack}");
    let Resolution::Ready(affected) = runtime
        .replace_json_and_resolve_affected("tokens.json", document(2), &[], &["src"])
        .unwrap()
    else {
        panic!("everything is loaded");
    };
    tokens.apply_affected(&affected);
    let full = runtime.resolve(&[]).unwrap().ready().unwrap();
    assert_eq!(tokens.get("b").unwrap().value, full.get("b").unwrap().value);
    assert_eq!(tokens, full);
}
