//! NaN and Infinity cannot cross to Rust: JSON would turn them into `null`,
//! so a native call with one throws a `TypeError` naming the function.
#![cfg(feature = "expressions")]

use dxn_dtcg::{
    DtcgRuntime, EXPRESSION_EXTENSION, RuntimeOptions,
    expressions::{ExpressionOptions, NativeModule},
};
use serde_json::{Value, json};

fn caught(js: &str) -> Value {
    let document = json!({ "computed": { "$type": "fontFamily", "$value": "x", "$extensions": { EXPRESSION_EXTENSION: {
        "imports": { "dxn.dtcg/color": "color", "brand": ["double"] },
        "js": format!("(() => {{ try {{ {js}; return 'no error'; }} catch (e) {{ return e.name + '|' + e.message; }} }})()")
    } } } });
    DtcgRuntime::from_json_with_options(
        "tokens.json",
        document.to_string(),
        RuntimeOptions::default().with_expressions(
            ExpressionOptions::default().with_native_module(
                NativeModule::new("brand").function("double", |value: Option<f64>| {
                    value.map(|value| value * 2.0)
                }),
            ),
        ),
    )
    .unwrap()
    .resolve(&[])
    .unwrap()
    .ready()
    .unwrap()
    .get("computed")
    .unwrap()
    .value
    .clone()
}

#[test]
fn non_finite_numbers_are_type_errors() {
    let accent = "{ colorSpace: 'srgb', components: [0.2, 0.4, 0.9] }";
    for (js, message) in [
        (
            "double(NaN)".to_owned(),
            "TypeError|brand.double: argument 1 must be a finite number",
        ),
        (
            "double(Infinity)".to_owned(),
            "TypeError|brand.double: argument 1 must be a finite number",
        ),
        (
            format!("color.mix({accent}, {accent}, NaN)"),
            "TypeError|dxn.dtcg/color.mix: argument 3 must be a finite number",
        ),
        (
            format!("color.lighten({accent}, -Infinity)"),
            "TypeError|dxn.dtcg/color.lighten: argument 2 must be a finite number",
        ),
        (
            "color.toHex({ colorSpace: 'srgb', components: [NaN, 0, 0] })".to_owned(),
            "TypeError|dxn.dtcg/color.toHex: arguments must not contain NaN or Infinity",
        ),
    ] {
        assert_eq!(caught(&js), json!(message), "{js}");
    }
    // `null` and a missing argument still arrive as `null`.
    assert_eq!(caught("double(null)"), json!("no error"));
    assert_eq!(caught("double()"), json!("no error"));
}
