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

#[wasm_bindgen_test]
fn a_token_imports_a_host_module() {
    use dxn_dtcg::expressions::ExpressionModule;
    let options = RuntimeOptions::default().with_expressions(ExpressionOptions::default().with_module(
        "brand/math",
        ExpressionModule::typescript(
            "export const ratio: number = 1.5;\nexport function step(base: number, n: number): number { return base * ratio ** n; }",
        ),
    ));
    let document = json!({
        "computed": computed("number", json!(0), json!({ "imports": { "brand/math": ["step"] }, "js": "step(4, 2)" }))
    });
    let tokens = DtcgRuntime::from_json_with_options("tokens.json", document.to_string(), options)
        .unwrap()
        .resolve(&[])
        .unwrap()
        .ready()
        .unwrap();
    assert_eq!(tokens.get("computed").unwrap().value, json!(9));
}

#[wasm_bindgen_test]
fn a_token_calls_a_host_native_module() {
    use dxn_dtcg::expressions::NativeModule;
    let native = NativeModule::new("brand/native")
        .function("double", |value: f64| value * 2.0)
        .function("divide", |a: f64, b: f64| {
            if b == 0.0 {
                Err("division by zero")
            } else {
                Ok(a / b)
            }
        });
    let options = RuntimeOptions::default()
        .with_expressions(ExpressionOptions::default().with_native_module(native));
    let document = json!({
        "doubled": computed("number", json!(0), json!({ "imports": { "brand/native": ["double"] }, "js": "double(21)" })),
        "caught": computed("fontFamily", json!("x"), json!({
            "imports": { "brand/native": ["divide"] },
            "js": "(() => { try { return String(divide(1, 0)); } catch (e) { return e.message; } })()"
        }))
    });
    let tokens = DtcgRuntime::from_json_with_options("tokens.json", document.to_string(), options)
        .unwrap()
        .resolve(&[])
        .unwrap()
        .ready()
        .unwrap();
    assert_eq!(tokens.get("doubled").unwrap().value, json!(42));
    assert_eq!(
        tokens.get("caught").unwrap().value,
        json!("brand/native.divide: division by zero")
    );
}

#[wasm_bindgen_test]
fn a_token_converts_a_color_with_the_color_module() {
    let tokens = resolve(json!({
        "accent": { "$type": "color", "$value": { "colorSpace": "srgb", "components": [0.2, 0.4, 0.9] } },
        "oklch": computed(
            "color",
            json!({ "colorSpace": "srgb", "components": [0, 0, 0] }),
            json!({ "imports": { "dxn.dtcg/color": ["convert"] }, "inputs": { "accent": "{accent}" }, "js": "convert(accent, 'oklch')" }),
        )
    }))
    .unwrap();
    // The native groups' values: colorjs.io's 0.551803, 0.201484, 264.262273 rounded to six places.
    assert_eq!(
        tokens.get("oklch").unwrap().value,
        json!({ "colorSpace": "oklch", "components": [0.551803, 0.201484, 264.262273] })
    );
}
