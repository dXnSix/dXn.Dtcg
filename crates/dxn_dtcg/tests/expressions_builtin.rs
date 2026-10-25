//! The built-in module `dxn.dtcg`: DTCG value constructors, conversions,
//! rounding, clamping and validation.
#![cfg(feature = "expressions")]

use dxn_dtcg::{
    DtcgError, DtcgRuntime, EXPRESSION_EXTENSION, RuntimeOptions, ValueExtensions,
    expressions::ExpressionOptions,
};
use serde_json::{Value, json};

fn evaluate_with(
    token_type: &str,
    snapshot: Value,
    js: &str,
    options: RuntimeOptions,
) -> Result<Value, DtcgError> {
    let document = json!({
        "computed": { "$type": token_type, "$value": snapshot, "$extensions": { EXPRESSION_EXTENSION: {
            "imports": { "dxn.dtcg": "dtcg" }, "js": js
        } } }
    });
    DtcgRuntime::from_json_with_options("tokens.json", document.to_string(), options)
        .unwrap()
        .resolve(&[])
        .map(|resolution| {
            resolution
                .ready()
                .unwrap()
                .get("computed")
                .unwrap()
                .value
                .clone()
        })
}

fn options() -> RuntimeOptions {
    RuntimeOptions::default().with_expressions(ExpressionOptions::default())
}

fn evaluate(token_type: &str, snapshot: Value, js: &str) -> Value {
    evaluate_with(token_type, snapshot, js, options()).unwrap()
}

fn number(js: &str) -> Value {
    evaluate("number", json!(0), js)
}

fn text(js: &str) -> Value {
    evaluate("fontFamily", json!("x"), js)
}

const DIMENSION: fn() -> Value = || json!({ "value": 0, "unit": "px" });
const DURATION: fn() -> Value = || json!({ "value": 0, "unit": "ms" });

#[test]
fn constructors_return_valid_dtcg_values() {
    assert_eq!(
        evaluate("dimension", DIMENSION(), "dtcg.px(4)"),
        json!({ "value": 4, "unit": "px" })
    );
    assert_eq!(
        evaluate("dimension", DIMENSION(), "dtcg.rem(1.5)"),
        json!({ "value": 1.5, "unit": "rem" })
    );
    assert_eq!(
        evaluate("dimension", DIMENSION(), "dtcg.dimension(2, 'rem')"),
        json!({ "value": 2, "unit": "rem" })
    );
    assert_eq!(
        evaluate("duration", DURATION(), "dtcg.ms(150)"),
        json!({ "value": 150, "unit": "ms" })
    );
    assert_eq!(
        evaluate("duration", DURATION(), "dtcg.s(0.2)"),
        json!({ "value": 0.2, "unit": "s" })
    );
    assert_eq!(
        evaluate("duration", DURATION(), "dtcg.duration(2, 's')"),
        json!({ "value": 2, "unit": "s" })
    );
    // Named imports work too.
    let document = json!({
        "computed": { "$type": "dimension", "$value": DIMENSION(), "$extensions": { EXPRESSION_EXTENSION: {
            "imports": { "dxn.dtcg": ["px", "rem"] }, "js": "px(rem(2).value * 8)"
        } } }
    });
    let tokens =
        DtcgRuntime::from_json_with_options("tokens.json", document.to_string(), options())
            .unwrap()
            .resolve(&[])
            .unwrap()
            .ready()
            .unwrap();
    assert_eq!(
        tokens.get("computed").unwrap().value,
        json!({ "value": 16, "unit": "px" })
    );
}

#[test]
fn conversions_rounding_and_clamping() {
    assert_eq!(number("dtcg.toPx(dtcg.rem(1.5), 16)"), json!(24));
    assert_eq!(
        number("dtcg.toPx({ value: 12, unit: 'px' }, 16)"),
        json!(12)
    );
    assert_eq!(number("dtcg.toMillis(dtcg.s(0.25))"), json!(250));
    assert_eq!(
        number("dtcg.toMillis({ value: 80, unit: 'ms' })"),
        json!(80)
    );
    assert_eq!(number("dtcg.round(13, 4)"), json!(12));
    assert_eq!(number("dtcg.round(14, 4)"), json!(16));
    assert_eq!(number("dtcg.round(1.26, 0.25)"), json!(1.25));
    assert_eq!(number("dtcg.round(2.5)"), json!(3));
    assert_eq!(number("dtcg.clamp(12, 0, 10)"), json!(10));
    assert_eq!(number("dtcg.clamp(-1, 0, 10)"), json!(0));
    assert_eq!(number("dtcg.clamp(5, 0, 10)"), json!(5));
}

#[test]
fn is_valid_validates_with_the_runtime_value_extensions() {
    assert_eq!(
        text("String(dtcg.isValid('dimension', dtcg.px(4)))"),
        json!("true")
    );
    assert_eq!(
        text("String(dtcg.isValid('dimension', { value: 4, unit: 'em' }))"),
        json!("false")
    );
    assert_eq!(
        text("String(dtcg.isValid('color', { colorSpace: 'srgb', components: [1, 0, 0] }))"),
        json!("true")
    );
    assert_eq!(
        text("String(dtcg.isValid('number', 'four'))"),
        json!("false")
    );
    let blur = "String(dtcg.isValid('dxn.dtcg.blur', { kind: 'layer', radius: dtcg.px(4) }))";
    assert_eq!(text(blur), json!("false"));
    let extended = options().with_extensions(ValueExtensions::all());
    assert_eq!(
        evaluate_with("fontFamily", json!("x"), blur, extended).unwrap(),
        json!("true")
    );
}

#[test]
fn invalid_arguments_throw_type_errors_with_one_prefix() {
    for (js, message) in [
        ("dtcg.px('4')", "dxn.dtcg.px: argument 1 must be a number"),
        ("dtcg.rem()", "dxn.dtcg.rem: argument 1 must be a number"),
        (
            "dtcg.dimension(4, 'em')",
            "dxn.dtcg.dimension: unit `em` is not one of px, rem",
        ),
        (
            "dtcg.dimension('4', 'px')",
            "dxn.dtcg.dimension: argument 1 must be a number",
        ),
        ("dtcg.ms(null)", "dxn.dtcg.ms: argument 1 must be a number"),
        (
            "dtcg.s(NaN)",
            "dxn.dtcg.s: argument 1 must be a finite number",
        ),
        (
            "dtcg.duration(1, 'min')",
            "dxn.dtcg.duration: unit `min` is not one of ms, s",
        ),
        (
            "dtcg.toPx({ value: 1, unit: 'em' }, 16)",
            "dxn.dtcg.toPx: argument 1 is not a valid dimension: resolved value does not match DTCG type `dimension`",
        ),
        (
            "dtcg.toPx(dtcg.px(1), 'x')",
            "dxn.dtcg.toPx: argument 2 must be a number",
        ),
        (
            "dtcg.toMillis(dtcg.px(1))",
            "dxn.dtcg.toMillis: argument 1 is not a valid duration: resolved value does not match DTCG type `duration`",
        ),
        (
            "dtcg.round('1')",
            "dxn.dtcg.round: argument 1 must be a number",
        ),
        (
            "dtcg.round(1, 0)",
            "dxn.dtcg.round: step must be a positive number",
        ),
        (
            "dtcg.clamp(1, 'a', 2)",
            "dxn.dtcg.clamp: argument 2 must be a number",
        ),
        (
            "dtcg.clamp(1, 3, 2)",
            "dxn.dtcg.clamp: min must not exceed max",
        ),
        (
            "dtcg.isValid(4, 1)",
            "dxn.dtcg.isValid: argument 1 must be a string",
        ),
    ] {
        let caught = text(&format!(
            "(() => {{ try {{ {js}; return 'no error'; }} catch (e) {{ return (e instanceof TypeError ? 'TypeError' : 'other') + '|' + e.message; }} }})()"
        ));
        assert_eq!(caught, json!(format!("TypeError|{message}")), "{js}");
    }
}
