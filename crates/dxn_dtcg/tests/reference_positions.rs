use dxn_dtcg::{DtcgError, DtcgRuntime, ResolvedTokens};
use serde_json::{Value, json};
use wasm_bindgen_test::wasm_bindgen_test;

fn resolve(document: Value) -> Result<ResolvedTokens, DtcgError> {
    DtcgRuntime::from_json("tokens.json", document.to_string())
        .unwrap()
        .resolve(&[])
        .map(|resolution| resolution.ready().expect("in-memory document"))
}

/// One token of every type that composite members may reference.
fn library() -> Value {
    json!({
        "c": {"$type": "color", "$value": {"colorSpace": "srgb", "components": [0, 0, 0]}},
        "d": {"$type": "dimension", "$value": {"value": 2, "unit": "px"}},
        "t": {"$type": "duration", "$value": {"value": 100, "unit": "ms"}},
        "e": {"$type": "cubicBezier", "$value": [0, 0, 1, 1]},
        "n": {"$type": "number", "$value": 0.5},
        "ff": {"$type": "fontFamily", "$value": "Inter"},
        "fw": {"$type": "fontWeight", "$value": 400},
        "ss": {"$type": "strokeStyle", "$value": "dashed"},
        "sh": {"$type": "shadow", "$value": {
            "color": {"colorSpace": "srgb", "components": [0, 0, 0]},
            "offsetX": {"value": 0, "unit": "px"}, "offsetY": {"value": 1, "unit": "px"},
            "blur": {"value": 2, "unit": "px"}, "spread": {"value": 0, "unit": "px"}
        }},
        "shs": {"$type": "shadow", "$value": [{
            "color": {"colorSpace": "srgb", "components": [0, 0, 0]},
            "offsetX": {"value": 0, "unit": "px"}, "offsetY": {"value": 1, "unit": "px"},
            "blur": {"value": 2, "unit": "px"}, "spread": {"value": 0, "unit": "px"}
        }]},
        "gr": {"$type": "gradient", "$value": [
            {"color": {"colorSpace": "srgb", "components": [0, 0, 0]}, "position": 0}
        ]}
    })
}

fn with_token(token_type: &str, value: Value) -> Value {
    let mut document = library();
    document["subject"] = json!({"$type": token_type, "$value": value});
    document
}

#[wasm_bindgen_test(unsupported = test)]
fn aliases_are_accepted_at_every_composite_member_and_element() {
    for (token_type, value) in [
        (
            "border",
            json!({"color": "{c}", "width": "{d}", "style": "{ss}"}),
        ),
        (
            "border",
            json!({"color": "{c}", "width": "{d}", "style": {"dashArray": ["{d}", {"value": 1, "unit": "px"}], "lineCap": "round"}}),
        ),
        (
            "transition",
            json!({"duration": "{t}", "delay": "{t}", "timingFunction": "{e}"}),
        ),
        (
            "shadow",
            json!({"color": "{c}", "offsetX": "{d}", "offsetY": "{d}", "blur": "{d}", "spread": "{d}"}),
        ),
        (
            "shadow",
            json!([{"color": "{c}", "offsetX": "{d}", "offsetY": "{d}", "blur": "{d}", "spread": "{d}", "inset": true}, "{sh}"]),
        ),
        (
            "gradient",
            json!([{"color": "{c}", "position": "{n}"}, {"color": "{c}", "position": 1}]),
        ),
        (
            "typography",
            json!({"fontFamily": "{ff}", "fontSize": "{d}", "fontWeight": "{fw}", "letterSpacing": "{d}", "lineHeight": "{n}"}),
        ),
        (
            "strokeStyle",
            json!({"dashArray": ["{d}", "{d}"], "lineCap": "butt"}),
        ),
    ] {
        let tokens = resolve(with_token(token_type, value.clone()))
            .unwrap_or_else(|error| panic!("{token_type} {value}: {error}"));
        assert_eq!(
            tokens.get("subject").unwrap().token_type.as_deref(),
            Some(token_type)
        );
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn aliases_inside_non_composite_positions_are_rejected() {
    for (token_type, value) in [
        (
            "color",
            json!({"colorSpace": "srgb", "components": [0, 0, 0], "alpha": "{n}"}),
        ),
        (
            "color",
            json!({"colorSpace": "srgb", "components": ["{n}", 0, 0]}),
        ),
        ("color", json!({"colorSpace": "srgb", "components": "{n}"})),
        (
            "color",
            json!({"colorSpace": "{ff}", "components": [0, 0, 0]}),
        ),
        (
            "color",
            json!({"colorSpace": "srgb", "components": [0, 0, 0], "hex": "{ff}"}),
        ),
        ("dimension", json!({"value": "{n}", "unit": "px"})),
        ("duration", json!({"value": "{n}", "unit": "ms"})),
        ("cubicBezier", json!(["{n}", 0, 1, 1])),
        ("fontFamily", json!(["{ff}", "serif"])),
        (
            "strokeStyle",
            json!({"dashArray": [{"value": 1, "unit": "px"}], "lineCap": "{ff}"}),
        ),
        (
            "strokeStyle",
            json!({"dashArray": "{d}", "lineCap": "round"}),
        ),
        (
            "shadow",
            json!({"color": "{c}", "offsetX": "{d}", "offsetY": "{d}", "blur": "{d}", "spread": "{d}", "inset": "{n}"}),
        ),
        (
            "border",
            json!({"color": {"colorSpace": "srgb", "components": ["{n}", 0, 0]}, "width": "{d}", "style": "solid"}),
        ),
        (
            "typography",
            json!({"fontFamily": ["{ff}"], "fontSize": "{d}", "fontWeight": 400, "letterSpacing": "{d}", "lineHeight": 1}),
        ),
    ] {
        let error = resolve(with_token(token_type, value.clone())).unwrap_err();
        let expected = format!("curly-brace references are not allowed inside {token_type} values");
        assert!(
            matches!(&error, DtcgError::Token { path, message, .. }
                if path == "subject" && *message == expected),
            "{token_type} {value}: {error:?}"
        );
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn composite_member_aliases_must_target_the_member_type() {
    for (token_type, value, expected, found) in [
        (
            "border",
            json!({"color": "{d}", "width": "{d}", "style": "solid"}),
            "color",
            "dimension",
        ),
        (
            "border",
            json!({"color": "{c}", "width": "{n}", "style": "solid"}),
            "dimension",
            "number",
        ),
        (
            "border",
            json!({"color": "{c}", "width": "{d}", "style": "{ff}"}),
            "strokeStyle",
            "fontFamily",
        ),
        (
            "transition",
            json!({"duration": "{d}", "delay": "{t}", "timingFunction": "{e}"}),
            "duration",
            "dimension",
        ),
        (
            "transition",
            json!({"duration": "{t}", "delay": "{t}", "timingFunction": "{n}"}),
            "cubicBezier",
            "number",
        ),
        (
            "gradient",
            json!([{"color": "{c}", "position": "{d}"}]),
            "number",
            "dimension",
        ),
        ("gradient", json!(["{c}"]), "gradient", "color"),
        ("shadow", json!(["{c}"]), "shadow", "color"),
        (
            "strokeStyle",
            json!({"dashArray": ["{n}"], "lineCap": "round"}),
            "dimension",
            "number",
        ),
        (
            "typography",
            json!({"fontFamily": "{ff}", "fontSize": "{d}", "fontWeight": "{n}", "letterSpacing": "{d}", "lineHeight": 1}),
            "fontWeight",
            "number",
        ),
    ] {
        let error = resolve(with_token(token_type, value.clone())).unwrap_err();
        let message_expected =
            format!("reference type mismatch: expected {expected}, found {found}");
        assert!(
            matches!(&error, DtcgError::TokenType { path, message, .. }
                if path == "subject" && *message == message_expected),
            "{token_type} {value}: {error:?}"
        );
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn array_valued_targets_are_never_flattened_into_arrays() {
    for (token_type, value) in [("shadow", json!(["{shs}"])), ("gradient", json!(["{gr}"]))] {
        let error = resolve(with_token(token_type, value.clone())).unwrap_err();
        assert!(
            matches!(&error, DtcgError::TokenType { path, message, .. }
                if path == "subject" && message.contains("does not match")),
            "{token_type} {value}: {error:?}"
        );
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn json_pointer_objects_are_allowed_at_any_depth() {
    let tokens = resolve(
        with_token(
            "color",
            json!({
                "colorSpace": "srgb",
                "components": [{"$ref": "#/n/$value"}, 0, {"$ref": "#/$extensions/third"}],
                "alpha": {"$ref": "#/n/$value"}
            }),
        )
        .as_object()
        .cloned()
        .map(|mut document| {
            document.insert("$extensions".into(), json!({"third": 1}));
            Value::Object(document)
        })
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        tokens.get("subject").unwrap().value,
        json!({"colorSpace": "srgb", "components": [0.5, 0, 1], "alpha": 0.5})
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn a_whole_value_alias_is_always_allowed() {
    for (token_type, alias) in [
        ("color", "{c}"),
        ("cubicBezier", "{e}"),
        ("fontFamily", "{ff}"),
        ("shadow", "{shs}"),
    ] {
        let tokens = resolve(with_token(token_type, json!(alias))).unwrap();
        assert_eq!(
            tokens.get("subject").unwrap().token_type.as_deref(),
            Some(token_type)
        );
    }
}
