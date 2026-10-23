use dxn_dtcg::{DtcgError, DtcgRuntime, ResolvedTokens};
use serde_json::{Value, json};

fn resolve(document: Value) -> Result<ResolvedTokens, DtcgError> {
    DtcgRuntime::from_json("tokens.json", document.to_string())
        .unwrap()
        .resolve(&[])
        .map(|resolution| resolution.ready().expect("in-memory document"))
}

fn token_type(tokens: &ResolvedTokens, path: &str) -> String {
    tokens
        .get(path)
        .unwrap_or_else(|| panic!("missing {path}"))
        .token_type
        .as_deref()
        .unwrap()
        .to_owned()
}

fn dimensions() -> Value {
    json!({"$type": "dimension", "small": {"$value": {"value": 4, "unit": "px"}}})
}

#[test]
fn a_whole_value_reference_takes_its_targets_type_before_the_group_type() {
    let tokens = resolve(json!({
        "dims": dimensions(),
        "colors": {
            "$type": "color",
            "red": {"$value": {"colorSpace": "srgb", "components": [1, 0, 0]}},
            "alias": {"$value": "{dims.small}"},
            "pointer": {"$value": {"$ref": "#/dims/small/$value"}},
            "token-ref": {"$ref": "#/dims/small/$value"}
        }
    }))
    .unwrap();
    assert_eq!(token_type(&tokens, "colors.red"), "color");
    for path in ["colors.alias", "colors.pointer", "colors.token-ref"] {
        assert_eq!(token_type(&tokens, path), "dimension", "{path}");
        assert_eq!(
            tokens.get(path).unwrap().value,
            json!({"value": 4, "unit": "px"})
        );
    }
}

#[test]
fn an_own_type_comes_first_and_the_group_type_applies_otherwise() {
    let tokens = resolve(json!({
        "g": {
            "$type": "color",
            "count": {"$type": "number", "$value": 3},
            "literal": {"$value": {"colorSpace": "srgb", "components": [0, 0, 1]}}
        }
    }))
    .unwrap();
    assert_eq!(token_type(&tokens, "g.count"), "number");
    assert_eq!(token_type(&tokens, "g.literal"), "color");

    let untyped = resolve(json!({"g": {"x": {"$value": 1}}})).unwrap_err();
    assert!(
        matches!(&untyped, DtcgError::TokenType { path, message, .. }
            if path == "g.x" && message == "token requires a DTCG type"),
        "{untyped:?}"
    );
}

#[test]
fn the_group_type_after_extends_applies_to_untyped_tokens() {
    let tokens = resolve(json!({
        "base": {"$type": "number", "one": {"$value": 1}},
        "derived": {"$extends": "{base}", "two": {"$value": 2}, "alias": {"$value": "{dims.small}"}},
        "dims": dimensions()
    }))
    .unwrap();
    assert_eq!(token_type(&tokens, "derived.two"), "number");
    assert_eq!(token_type(&tokens, "derived.one"), "number");
    assert_eq!(token_type(&tokens, "derived.alias"), "dimension");
}

#[test]
fn a_token_level_ref_outside_a_token_value_inherits_the_group_type() {
    let tokens = resolve(json!({
        "$extensions": {"x": 7},
        "numbers": {"$type": "number", "from-extension": {"$ref": "#/$extensions/x"}}
    }))
    .unwrap();
    assert_eq!(token_type(&tokens, "numbers.from-extension"), "number");
    assert_eq!(
        tokens.get("numbers.from-extension").unwrap().value,
        json!(7)
    );
}

#[test]
fn a_value_pointer_whose_prefix_is_not_a_token_reads_the_document() {
    let tokens = resolve(json!({
        "$extensions": {"palette": {"$value": 3}},
        "numbers": {
            "$type": "number",
            "pointer": {"$value": {"$ref": "#/$extensions/palette/$value"}},
            "token-ref": {"$ref": "#/$extensions/palette/$value"}
        }
    }))
    .unwrap();
    for path in ["numbers.pointer", "numbers.token-ref"] {
        assert_eq!(tokens.get(path).unwrap().value, json!(3), "{path}");
        assert_eq!(token_type(&tokens, path), "number", "{path}");
    }
}

#[test]
fn an_own_type_that_differs_from_the_reference_target_type_is_a_mismatch() {
    for value in [
        json!({"$type": "fontWeight", "$value": "{weights.regular}"}),
        json!({"$type": "fontWeight", "$value": {"$ref": "#/weights/regular/$value"}}),
        json!({"$type": "fontWeight", "$ref": "#/weights/regular/$value"}),
    ] {
        let error = resolve(json!({
            "weights": {"regular": {"$type": "number", "$value": 400}},
            "heading": value
        }))
        .unwrap_err();
        assert!(
            matches!(&error, DtcgError::TokenType { path, message, .. }
                if path == "heading"
                    && message == "reference type mismatch: expected fontWeight, found number"),
            "{error:?}"
        );
    }

    let matching = resolve(json!({
        "weights": {"regular": {"$type": "fontWeight", "$value": 400}},
        "heading": {"$type": "fontWeight", "$value": "{weights.regular}"}
    }))
    .unwrap();
    assert_eq!(matching.get("heading").unwrap().value, json!(400));
}

#[test]
fn reference_types_follow_chains() {
    let tokens = resolve(json!({
        "a": {"$type": "duration", "$value": {"value": 100, "unit": "ms"}},
        "group": {"$type": "number", "b": {"$value": "{a}"}, "c": {"$value": "{group.b}"}}
    }))
    .unwrap();
    assert_eq!(token_type(&tokens, "group.b"), "duration");
    assert_eq!(token_type(&tokens, "group.c"), "duration");
}
