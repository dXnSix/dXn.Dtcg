//! Expressions on `wasm32-unknown-unknown`, run under Node by the
//! wasm-bindgen test runner with the default 1 MiB stack.
#![cfg(all(target_arch = "wasm32", target_os = "unknown", feature = "expressions"))]

use dxn_dtcg::{
    DtcgError, DtcgRuntime, EXPRESSION_EXTENSION, ResolvedTokens, RuntimeOptions,
    expressions::ExpressionOptions,
};
use serde_json::{Value, json};
use wasm_bindgen_test::wasm_bindgen_test;

fn resolve(document: Value) -> Result<ResolvedTokens, DtcgError> {
    DtcgRuntime::from_json_with_options(
        "tokens.json",
        document.to_string(),
        RuntimeOptions::default().with_expressions(ExpressionOptions::default()),
    )
    .unwrap()
    .resolve(&[])
    .map(|resolution| resolution.ready().unwrap())
}

fn computed(token_type: &str, value: Value, expression: Value) -> Value {
    json!({ "$type": token_type, "$value": value, "$extensions": { EXPRESSION_EXTENSION: expression } })
}

#[wasm_bindgen_test]
fn an_expression_with_inputs_evaluates() {
    let tokens = resolve(json!({
        "base": { "$type": "dimension", "$value": { "value": 4, "unit": "px" } },
        "ratio": { "$type": "number", "$value": 1.5 },
        "large": computed(
            "dimension",
            json!({ "value": 0, "unit": "px" }),
            json!({ "inputs": { "base": "{base}", "ratio": "{ratio}" }, "js": "({ ...base, value: base.value * ratio ** 2 })" }),
        )
    }))
    .unwrap();
    let large = tokens.get("large").unwrap();
    assert_eq!(large.value, json!({ "value": 9, "unit": "px" }));
    assert_eq!(large.snapshot, Some(json!({ "value": 0, "unit": "px" })));
}

#[wasm_bindgen_test]
fn a_typescript_expression_evaluates() {
    let tokens = resolve(json!({
        "base": { "$type": "number", "$value": 4 },
        "third": computed(
            "number",
            json!(0),
            json!({ "inputs": { "base": "{base}" }, "ts": "((value: number, by: number): number => value / by)(base as number, 3)" }),
        )
    }))
    .unwrap();
    assert_eq!(tokens.get("third").unwrap().value, json!(1.333333));
}

#[wasm_bindgen_test]
fn an_infinite_loop_exceeds_the_step_budget() {
    let error = resolve(json!({
        "loop": computed("number", json!(0), json!({ "js": "(() => { for (;;) {} })()" }))
    }))
    .unwrap_err();
    assert!(
        matches!(
            error,
            DtcgError::LimitExceeded {
                kind: "expression steps",
                ..
            }
        ),
        "{error:?}"
    );
}

#[wasm_bindgen_test]
fn runaway_recursion_is_an_expression_error() {
    let error = resolve(json!({
        "recursion": computed(
            "number",
            json!(0),
            json!({ "js": "(() => { const f = (n) => { try { return f(n + 1) + 1; } catch { return 0; } }; return f(0); })()" }),
        )
    }))
    .unwrap_err();
    match error {
        DtcgError::Expression { message, .. } => {
            assert!(message.contains("stack overflow"), "{message}")
        }
        other => panic!("{other:?}"),
    }
}
