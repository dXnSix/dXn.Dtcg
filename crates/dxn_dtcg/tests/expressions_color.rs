//! The built-in module `dxn.dtcg/color`: the color API of `values::Color`
//! on DTCG color JSON.
#![cfg(feature = "expressions")]

use dxn_dtcg::{
    DtcgRuntime, EXPRESSION_EXTENSION, RuntimeOptions, ValueExtensions,
    expressions::ExpressionOptions,
    validate_value_with,
    values::{Color, ColorSpace},
};
use serde_json::{Value, json};
use wasm_bindgen_test::wasm_bindgen_test;

fn evaluate(token_type: &str, snapshot: Value, js: &str) -> Value {
    let accent = json!({ "colorSpace": "srgb", "components": [0.2, 0.4, 0.9] });
    let document = json!({
        "accent": { "$type": "color", "$value": accent },
        "computed": { "$type": token_type, "$value": snapshot, "$extensions": { EXPRESSION_EXTENSION: {
            "imports": { "dxn.dtcg/color": "color" }, "inputs": { "accent": "{accent}" }, "js": js
        } } }
    });
    DtcgRuntime::from_json_with_options(
        "tokens.json",
        document.to_string(),
        RuntimeOptions::default()
            .with_expressions(ExpressionOptions::default().with_precision(None)),
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

fn color(js: &str) -> Value {
    let value = evaluate(
        "color",
        json!({ "colorSpace": "srgb", "components": [0, 0, 0] }),
        js,
    );
    validate_value_with("color", &value, &ValueExtensions::default()).unwrap();
    value
}

fn accent() -> Color {
    Color::new(ColorSpace::Srgb, [0.2, 0.4, 0.9], 1.0)
}

#[wasm_bindgen_test(unsupported = test)]
fn the_module_matches_the_rust_api() {
    assert_eq!(
        color("color.convert(accent, 'oklch')"),
        accent().convert(ColorSpace::Oklch).to_json()
    );
    assert_eq!(
        color("color.convert(accent, 'display-p3')"),
        accent().convert(ColorSpace::DisplayP3).to_json()
    );
    let white = Color::new(ColorSpace::Srgb, [1.0, 1.0, 1.0], 1.0);
    assert_eq!(
        color("color.mix(accent, { colorSpace: 'srgb', components: [1, 1, 1] })"),
        accent().mix(&white, 0.5).to_json()
    );
    assert_eq!(
        color("color.mix(accent, { colorSpace: 'srgb', components: [1, 1, 1] }, 0.25, 'srgb')"),
        accent().mix_in(&white, 0.25, ColorSpace::Srgb).to_json()
    );
    assert_eq!(
        color("color.lighten(accent, 0.1)"),
        accent().lighten(0.1).to_json()
    );
    assert_eq!(
        color("color.darken(accent, 0.1)"),
        accent().darken(0.1).to_json()
    );
    assert_eq!(
        color("color.withAlpha(accent, 0.5)"),
        accent().with_alpha(0.5).to_json()
    );
    assert_eq!(
        evaluate("fontFamily", json!("x"), "color.toHex(accent)"),
        json!("#3366e6")
    );
    assert_eq!(
        color("color.fromHex('#3366e6')"),
        Color::from_hex("#3366e6").unwrap().to_json()
    );
    let contrast = evaluate(
        "number",
        json!(0),
        "color.contrast(accent, { colorSpace: 'srgb', components: [1, 1, 1] })",
    );
    assert!((contrast.as_f64().unwrap() - 5.0261420912568235).abs() < 1e-9);
    assert_eq!(
        evaluate(
            "fontFamily",
            json!("x"),
            "[color.inGamut(accent, 'srgb'), color.inGamut({ colorSpace: 'display-p3', components: [0, 1, 0] }, 'srgb')].join()"
        ),
        json!("true,false")
    );
    let p3 = Color::new(ColorSpace::DisplayP3, [0.0, 1.0, 0.0], 1.0);
    assert_eq!(
        color("color.clampToGamut({ colorSpace: 'display-p3', components: [0, 1, 0] }, 'srgb')"),
        p3.clamp_to_gamut(ColorSpace::Srgb).to_json()
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn conversions_match_css_color_4_reference_values() {
    // colorjs.io 0.7.1.
    let oklch = color("color.convert(accent, 'oklch')");
    let expected = [0.5518026071036082, 0.20148432547243425, 264.262272720753];
    for (actual, expected) in oklch["components"].as_array().unwrap().iter().zip(expected) {
        assert!(
            (actual.as_f64().unwrap() - expected).abs() < 1e-5,
            "{oklch}"
        );
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn invalid_arguments_throw_type_errors_with_one_prefix() {
    for (js, message) in [
        (
            "color.convert({ colorSpace: 'srgb', components: [2, 0, 0] }, 'oklch')",
            "dxn.dtcg/color.convert: argument 1 is not a valid color: resolved value does not match DTCG type `color`",
        ),
        (
            "color.convert(accent, 'p3')",
            "dxn.dtcg/color.convert: `p3` is not a DTCG color space",
        ),
        (
            "color.mix(accent, 'white')",
            "dxn.dtcg/color.mix: argument 2 is not a valid color: resolved value does not match DTCG type `color`",
        ),
        (
            "color.lighten(accent, 'more')",
            "dxn.dtcg/color.lighten: argument 2 must be a number",
        ),
        (
            "color.withAlpha(accent)",
            "dxn.dtcg/color.withAlpha: argument 2 must be a number",
        ),
        (
            "color.fromHex('#fff')",
            "dxn.dtcg/color.fromHex: expected a `#rrggbb` hex string",
        ),
        (
            "color.toHex(null)",
            "dxn.dtcg/color.toHex: argument 1 is not a valid color: resolved value does not match DTCG type `color`",
        ),
        (
            "color.inGamut(accent, 'rgb')",
            "dxn.dtcg/color.inGamut: `rgb` is not a DTCG color space",
        ),
    ] {
        let caught = evaluate(
            "fontFamily",
            json!("x"),
            &format!(
                "(() => {{ try {{ {js}; return 'no error'; }} catch (e) {{ return (e instanceof TypeError ? 'TypeError' : 'other') + '|' + e.message; }} }})()"
            ),
        );
        assert_eq!(caught, json!(format!("TypeError|{message}")), "{js}");
    }
}
