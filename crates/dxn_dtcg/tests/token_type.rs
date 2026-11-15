// The public token type enum: its `$type` spellings, the resolved token's typed accessor and the
// type a `TypedValue` was converted by.
use dxn_dtcg::{
    BLUR_TYPE, DtcgRuntime, RuntimeOptions, TokenType, ValueExtensions, ValueValidationError,
    validate_value, validate_value_with,
};
use serde_json::{Value, json};
use wasm_bindgen_test::wasm_bindgen_test;

const DTCG_NAMES: [&str; 13] = [
    "color",
    "dimension",
    "fontFamily",
    "fontWeight",
    "duration",
    "cubicBezier",
    "number",
    "strokeStyle",
    "border",
    "transition",
    "shadow",
    "gradient",
    "typography",
];

fn color() -> Value {
    json!({"colorSpace": "srgb", "components": [0, 0, 0]})
}

fn px(value: f64) -> Value {
    json!({"value": value, "unit": "px"})
}

fn ms(value: f64) -> Value {
    json!({"value": value, "unit": "ms"})
}

/// A valid value of `token_type`.
fn sample(token_type: TokenType) -> Value {
    match token_type {
        TokenType::Color => color(),
        TokenType::Dimension => px(4.0),
        TokenType::FontFamily => json!(["Inter", "sans-serif"]),
        TokenType::FontWeight => json!("bold"),
        TokenType::Duration => ms(100.0),
        TokenType::CubicBezier => json!([0.4, 0, 0.2, 1]),
        TokenType::Number => json!(1.5),
        TokenType::StrokeStyle => json!("dashed"),
        TokenType::Border => json!({"color": color(), "width": px(1.0), "style": "solid"}),
        TokenType::Transition => {
            json!({"duration": ms(100.0), "delay": ms(0.0), "timingFunction": [0, 0, 1, 1]})
        }
        TokenType::Shadow => json!({
            "color": color(), "offsetX": px(0.0), "offsetY": px(1.0),
            "blur": px(2.0), "spread": px(0.0)
        }),
        TokenType::Gradient => json!([{"color": color(), "position": 0}]),
        TokenType::Typography => json!({
            "fontFamily": "Inter", "fontSize": px(16.0), "fontWeight": 400,
            "letterSpacing": px(0.0), "lineHeight": 1.5
        }),
        TokenType::Blur => json!({"kind": "layer", "radius": px(4.0)}),
        other => panic!("no sample for {other}"),
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn every_type_round_trips_through_its_type_spelling() {
    for token_type in TokenType::ALL {
        assert_eq!(TokenType::from_name(token_type.name()), Some(*token_type));
        assert_eq!(token_type.to_string(), token_type.name());
    }
    let standard = TokenType::ALL
        .iter()
        .filter(|token_type| !token_type.is_extension())
        .map(|token_type| token_type.name())
        .collect::<Vec<_>>();
    assert_eq!(standard, DTCG_NAMES);
    assert_eq!(TokenType::Blur.name(), BLUR_TYPE);
    assert!(TokenType::Blur.is_extension());
}

#[wasm_bindgen_test(unsupported = test)]
fn unknown_and_misspelled_names_name_no_type() {
    for name in [
        "",
        "colour",
        "fontweight",
        "Color",
        "string",
        "dxn.dtcg.other",
        "com.example.elevation",
    ] {
        assert_eq!(TokenType::from_name(name), None, "{name}");
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn validation_accepts_exactly_the_enabled_types() {
    for token_type in TokenType::ALL {
        let strict = validate_value(token_type.name(), &sample(*token_type));
        if token_type.is_extension() {
            assert_eq!(
                strict,
                Err(ValueValidationError::UnsupportedType(
                    token_type.name().into()
                ))
            );
        } else {
            assert_eq!(strict, Ok(()), "{token_type}");
        }
        validate_value_with(
            token_type.name(),
            &sample(*token_type),
            &ValueExtensions::all(),
        )
        .unwrap_or_else(|error| panic!("{token_type}: {error}"));
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn resolved_tokens_and_typed_values_report_their_type() {
    let mut document = serde_json::Map::new();
    for token_type in TokenType::ALL {
        document.insert(
            token_type.name().replace('.', "-"),
            json!({"$type": token_type.name(), "$value": sample(*token_type)}),
        );
    }
    // An alias takes its target's type, and a group's `$type` is inherited.
    document.insert("alias".into(), json!({"$value": "{dimension}"}));
    document.insert(
        "group".into(),
        json!({"$type": "duration", "fast": {"$value": ms(50.0)}}),
    );
    let tokens = DtcgRuntime::from_json_with_options(
        "tokens.json",
        Value::Object(document).to_string(),
        RuntimeOptions::default().with_extensions(ValueExtensions::all()),
    )
    .unwrap()
    .resolve(&[])
    .unwrap()
    .ready()
    .unwrap();

    for token_type in TokenType::ALL {
        let token = tokens.get(&token_type.name().replace('.', "-")).unwrap();
        assert_eq!(token.dtcg_type(), Some(*token_type));
        let typed = token.typed(&ValueExtensions::all()).unwrap();
        assert_eq!(typed.token_type(), *token_type);
    }
    assert_eq!(
        tokens.get("alias").unwrap().dtcg_type(),
        Some(TokenType::Dimension)
    );
    assert_eq!(
        tokens.get("group.fast").unwrap().dtcg_type(),
        Some(TokenType::Duration)
    );
}
