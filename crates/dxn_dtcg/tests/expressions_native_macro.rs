//! `#[expression_module]` turns an `impl` block's public functions into a
//! native module with declarations; misuse is a compile error.
#![cfg(feature = "expressions")]

use dxn_dtcg::{
    DtcgRuntime, EXPRESSION_EXTENSION, RuntimeOptions,
    expressions::{ExpressionOptions, expression_module},
    values::{Color, Dimension, Shadow, Typography},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use wasm_bindgen_test::wasm_bindgen_test;

#[derive(Deserialize, Serialize)]
pub struct Pair {
    a: f64,
    b: f64,
}

pub struct Brand;

/// Brand helpers for expressions.
#[expression_module(specifier = "brand/macro")]
impl Brand {
    /// Doubles a number.
    ///
    /// Used for spacing scales.
    pub fn double_it(value: f64) -> f64 {
        value * 2.0
    }

    /// Greets someone.
    #[expression(name = "greeting")]
    pub fn greet(name: &str, excited: Option<bool>) -> String {
        format!(
            "hello {name}{}",
            if excited == Some(true) { "!" } else { "" }
        )
    }

    pub fn safe_divide(a: f64, b: f64) -> Result<f64, String> {
        if b == 0.0 {
            Err("division by zero".into())
        } else {
            Ok(a / b)
        }
    }

    pub fn scale(dimension: Dimension, factor: f64) -> Dimension {
        Dimension {
            value: dimension.value * factor,
            ..dimension
        }
    }

    pub fn sum(values: &[f64]) -> f64 {
        values.iter().sum()
    }

    pub fn names(count: u32) -> Vec<String> {
        (0..count).map(|index| format!("n{index}")).collect()
    }

    pub fn identity(value: serde_json::Value) -> serde_json::Value {
        value
    }

    #[expression(ts = "(color: Color) => string")]
    pub fn describe(color: Color) -> String {
        color.color_space.name().to_owned()
    }

    pub fn nothing() {}

    pub fn swap(pair: Pair) -> Pair {
        Pair {
            a: pair.b,
            b: pair.a,
        }
    }

    pub fn text_shadow(typography: Typography, shadow: Shadow) -> Vec<Shadow> {
        vec![shadow; usize::from(typography.font_weight.0 > 500.0) + 1]
    }

    pub fn r#type(value: bool) -> bool {
        !value
    }

    fn private_helper() -> f64 {
        1.0
    }
}

fn evaluate(token_type: &str, snapshot: Value, js: &str) -> Value {
    let document = json!({
        "computed": { "$type": token_type, "$value": snapshot, "$extensions": { EXPRESSION_EXTENSION: {
            "imports": { "brand/macro": "brand" }, "js": js
        } } }
    });
    let options = RuntimeOptions::default().with_expressions(
        ExpressionOptions::default().with_native_module(Brand::expression_module()),
    );
    DtcgRuntime::from_json_with_options("tokens.json", document.to_string(), options)
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

#[wasm_bindgen_test(unsupported = test)]
fn public_functions_become_camel_case_exports() {
    let module = Brand::expression_module();
    assert_eq!(module.specifier(), "brand/macro");
    assert_eq!(
        evaluate("number", json!(0), "brand.doubleIt(21)"),
        json!(42)
    );
    assert_eq!(
        evaluate("fontFamily", json!("x"), "brand.greeting('dtcg', true)"),
        json!("hello dtcg!")
    );
    assert_eq!(
        evaluate("number", json!(0), "brand.safeDivide(9, 3)"),
        json!(3)
    );
    assert_eq!(
        evaluate(
            "fontFamily",
            json!("x"),
            "(() => { try { brand.safeDivide(1, 0); } catch (e) { return e.message; } })()"
        ),
        json!("brand/macro.safeDivide: division by zero")
    );
    assert_eq!(
        evaluate(
            "dimension",
            json!({ "value": 0, "unit": "px" }),
            "brand.scale({ value: 2, unit: 'rem' }, 3)"
        ),
        json!({ "value": 6, "unit": "rem" })
    );
    assert_eq!(
        evaluate("number", json!(0), "brand.sum([1, 2, 3.5])"),
        json!(6.5)
    );
    assert_eq!(
        evaluate("fontFamily", json!("x"), "brand.names(2)"),
        json!(["n0", "n1"])
    );
    assert_eq!(evaluate("number", json!(0), "brand.identity(7)"), json!(7));
    assert_eq!(
        evaluate(
            "fontFamily",
            json!("x"),
            "brand.describe({ colorSpace: 'oklch', components: [0.5, 0.1, 30] })"
        ),
        json!("oklch")
    );
    assert_eq!(
        evaluate("fontFamily", json!("x"), "String(brand.nothing())"),
        json!("null")
    );
    assert_eq!(
        evaluate("number", json!(0), "brand.swap({ a: 1, b: 2 }).a"),
        json!(2)
    );
    assert_eq!(
        evaluate(
            "number",
            json!(0),
            "brand.textShadow({ fontFamily: 'Inter', fontSize: { value: 16, unit: 'px' }, fontWeight: 'bold', \
             letterSpacing: { value: 0, unit: 'px' }, lineHeight: 1.5 }, { color: { colorSpace: 'srgb', \
             components: [0, 0, 0] }, offsetX: { value: 0, unit: 'px' }, offsetY: { value: 1, unit: 'px' }, \
             blur: { value: 2, unit: 'px' }, spread: { value: 0, unit: 'px' } }).length"
        ),
        json!(2)
    );
    assert_eq!(
        evaluate("fontFamily", json!("x"), "String(brand.type(false))"),
        json!("true")
    );
    assert_eq!(
        evaluate(
            "fontFamily",
            json!("x"),
            "typeof brand.privateHelper + typeof brand.greet"
        ),
        json!("undefinedundefined")
    );
    assert_eq!(Brand::private_helper(), 1.0);
}

#[wasm_bindgen_test(unsupported = test)]
fn declarations_carry_docs_and_mapped_signatures() {
    let declarations = ExpressionOptions::default()
        .with_native_module(Brand::expression_module())
        .typescript_declarations();
    let module = &declarations[declarations.find("declare module \"brand/macro\"").unwrap()..];
    for expected in [
        "  /**\n   * Doubles a number.\n   *\n   * Used for spacing scales.\n   */\n  export const doubleIt: (value: number) => number;",
        "  /**\n   * Greets someone.\n   */\n  export const greeting: (name: string, excited: boolean | null) => string;",
        "export const safeDivide: (a: number, b: number) => number;",
        "export const scale: (dimension: Dimension, factor: number) => Dimension;",
        "export const sum: (values: number[]) => number;",
        "export const names: (count: number) => string[];",
        "export const identity: (value: unknown) => unknown;",
        "export const describe: (color: Color) => string;",
        "export const nothing: () => void;",
        "export const swap: (pair: unknown) => unknown;",
        "export const textShadow: (typography: Typography, shadow: Shadow) => Shadow[];",
        "export const type: (value: boolean) => boolean;",
    ] {
        assert!(
            module.contains(expected),
            "missing `{expected}` in\n{module}"
        );
    }
    assert!(
        declarations.contains(
            "/**\n * Brand helpers for expressions.\n */\ndeclare module \"brand/macro\""
        )
    );
    assert!(!module.contains("privateHelper"));
}

// Compile-fail cases need a native compiler.
#[cfg(not(target_family = "wasm"))]
#[test]
fn misuse_is_a_compile_error() {
    trybuild::TestCases::new().compile_fail("tests/expression_module_ui/*.rs");
}
