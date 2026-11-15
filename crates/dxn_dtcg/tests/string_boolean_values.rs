// The opt-in `dxn.dtcg.string` and `dxn.dtcg.boolean` extension types: DTCG 2025.10 has neither,
// but design tools commonly export string and boolean variables. Both are off by default, follow
// the alias rules of every other type, and read into `String`, `bool` and `TypedValue`.
use dxn_dtcg::{
    BOOLEAN_TYPE, DtcgError, DtcgRuntime, DtcgTokens, ResolvedTokens, RuntimeOptions, STRING_TYPE,
    TokenType, ValueExtensions, ValueValidationError, validate_value, validate_value_with,
    values::TypedValue,
};
use serde_json::{Value, json};
use wasm_bindgen_test::wasm_bindgen_test;

fn options(extensions: ValueExtensions) -> RuntimeOptions {
    RuntimeOptions::default().with_extensions(extensions)
}

fn both() -> ValueExtensions {
    ValueExtensions::default()
        .with_string(true)
        .with_boolean(true)
}

fn resolve_with(
    document: &Value,
    extensions: ValueExtensions,
) -> Result<ResolvedTokens, DtcgError> {
    DtcgRuntime::from_json_with_options("tokens.json", document.to_string(), options(extensions))
        .unwrap()
        .resolve(&[])
        .map(|resolution| resolution.ready().unwrap())
}

fn document() -> Value {
    json!({
        "label": {
            "$type": STRING_TYPE,
            "submit": {"$value": "Sign in"},
            "empty": {"$value": ""},
            "braces": {"$value": "Hello {name}"},
            "cancel": {"$value": "{label.submit}"}
        },
        "visible": {"$type": BOOLEAN_TYPE, "$value": true},
        "hidden": {"$value": "{visible}"},
        "flags": {"$type": BOOLEAN_TYPE, "compact": {"$value": false}}
    })
}

#[wasm_bindgen_test(unsupported = test)]
fn both_types_are_off_by_default() {
    assert!(!ValueExtensions::default().string && !ValueExtensions::default().boolean);
    assert!(ValueExtensions::all().string && ValueExtensions::all().boolean);
    for (token_type, value) in [(STRING_TYPE, json!("Sign in")), (BOOLEAN_TYPE, json!(true))] {
        assert_eq!(
            validate_value(token_type, &value),
            Err(ValueValidationError::UnsupportedType(token_type.into()))
        );
        let document = json!({"x": {"$type": token_type, "$value": value}});
        let error = resolve_with(&document, ValueExtensions::default()).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!(
                "token `x` in `tokens.json`: unsupported DTCG type `{token_type}` for final 2025.10"
            )
        );
    }
    // The pre-2025 `string` type stays unsupported, with the extensions too.
    assert!(validate_value_with("string", &json!("x"), &ValueExtensions::all()).is_err());
    assert!(validate_value_with("boolean", &json!(true), &ValueExtensions::all()).is_err());
}

#[wasm_bindgen_test(unsupported = test)]
fn the_types_are_named_like_the_other_extension_types() {
    assert_eq!(STRING_TYPE, "dxn.dtcg.string");
    assert_eq!(BOOLEAN_TYPE, "dxn.dtcg.boolean");
    assert_eq!(TokenType::from_name(STRING_TYPE), Some(TokenType::String));
    assert_eq!(TokenType::from_name(BOOLEAN_TYPE), Some(TokenType::Boolean));
    assert!(TokenType::String.is_extension() && TokenType::Boolean.is_extension());
}

#[wasm_bindgen_test(unsupported = test)]
fn each_flag_accepts_only_its_own_shape() {
    let string = ValueExtensions::default().with_string(true);
    let boolean = ValueExtensions::default().with_boolean(true);
    validate_value_with(STRING_TYPE, &json!("x"), &string).unwrap();
    validate_value_with(STRING_TYPE, &json!(""), &string).unwrap();
    validate_value_with(BOOLEAN_TYPE, &json!(false), &boolean).unwrap();
    assert!(validate_value_with(BOOLEAN_TYPE, &json!(true), &string).is_err());
    assert!(validate_value_with(STRING_TYPE, &json!("x"), &boolean).is_err());

    for value in [
        json!(1),
        json!(true),
        json!(null),
        json!(["x"]),
        json!({"value": "x"}),
    ] {
        assert_eq!(
            validate_value_with(STRING_TYPE, &value, &string),
            Err(ValueValidationError::InvalidValue(STRING_TYPE.into())),
            "{value}"
        );
    }
    for value in [
        json!("true"),
        json!(1),
        json!(0),
        json!(null),
        json!([true]),
    ] {
        assert_eq!(
            validate_value_with(BOOLEAN_TYPE, &value, &boolean),
            Err(ValueValidationError::InvalidValue(BOOLEAN_TYPE.into())),
            "{value}"
        );
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn values_resolve_with_group_types_and_aliases() {
    let tokens = resolve_with(&document(), both()).unwrap();
    let value = |path: &str| tokens.get(path).unwrap().value.clone();
    assert_eq!(value("label.submit"), "Sign in");
    assert_eq!(value("label.empty"), "");
    // Only a whole `{path}` string is an alias.
    assert_eq!(value("label.braces"), "Hello {name}");
    assert_eq!(value("label.cancel"), "Sign in");
    assert_eq!(value("hidden"), true);
    assert_eq!(value("flags.compact"), false);

    for (path, token_type) in [
        ("label.submit", TokenType::String),
        ("label.cancel", TokenType::String),
        ("visible", TokenType::Boolean),
        ("hidden", TokenType::Boolean), // the alias target's type
        ("flags.compact", TokenType::Boolean),
    ] {
        assert_eq!(
            tokens.get(path).unwrap().dtcg_type(),
            Some(token_type),
            "{path}"
        );
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn aliases_must_target_their_own_type() {
    for (document, message) in [
        (
            json!({"n": {"$type": "number", "$value": 1},
                   "s": {"$type": STRING_TYPE, "$value": "{n}"}}),
            format!("reference type mismatch: expected {STRING_TYPE}, found number"),
        ),
        (
            json!({"s": {"$type": STRING_TYPE, "$value": "on"},
                   "b": {"$type": BOOLEAN_TYPE, "$value": "{s}"}}),
            format!("reference type mismatch: expected {BOOLEAN_TYPE}, found {STRING_TYPE}"),
        ),
        (
            json!({"s": {"$type": STRING_TYPE, "$value": "{missing}"}}),
            String::new(),
        ),
    ] {
        let error = resolve_with(&document, both()).unwrap_err();
        match &error {
            DtcgError::TokenType { message: found, .. } => assert_eq!(found, &message),
            DtcgError::UnresolvedReference { target, .. } => assert_eq!(target, "missing"),
            other => panic!("{other:?}"),
        }
    }
    // A string token is not a font family: composite members keep their types.
    let typography = json!({
        "family": {"$type": STRING_TYPE, "$value": "Inter"},
        "body": {"$type": "typography", "$value": {
            "fontFamily": "{family}", "fontSize": {"value": 16, "unit": "px"},
            "fontWeight": 400, "letterSpacing": {"value": 0, "unit": "px"}, "lineHeight": 1.5
        }}
    });
    assert!(matches!(
        resolve_with(&typography, both()).unwrap_err(),
        DtcgError::TokenType { path, .. } if path == "body"
    ));
}

#[wasm_bindgen_test(unsupported = test)]
fn typed_values_and_projection_read_them() {
    let tokens = resolve_with(&document(), both()).unwrap();
    let submit = tokens.get("label.submit").unwrap();
    let typed = submit.typed(&both()).unwrap();
    assert_eq!(typed, TypedValue::String("Sign in".into()));
    assert_eq!(typed.token_type(), TokenType::String);
    let visible = tokens.get("visible").unwrap().typed(&both()).unwrap();
    assert_eq!(visible, TypedValue::Boolean(true));
    assert_eq!(visible.token_type(), TokenType::Boolean);
    // Converting with extensions that lack the type fails, as for any extension type.
    assert!(matches!(
        submit.typed(&ValueExtensions::default()),
        Err(DtcgError::Projection { path, .. }) if path == "label.submit"
    ));
    assert_eq!(
        tokens.get_as::<TypedValue>("hidden").unwrap(),
        TypedValue::Boolean(true)
    );

    assert_eq!(tokens.get_as::<String>("label.cancel").unwrap(), "Sign in");
    assert!(!tokens.get_as::<bool>("flags.compact").unwrap());
    assert!(matches!(
        tokens.get_as::<bool>("label.submit"),
        Err(DtcgError::Projection { message, .. }) if message == "expected boolean value"
    ));

    #[derive(Debug, DtcgTokens)]
    struct Form {
        #[dtcg(path = "label.submit")]
        submit: String,
        visible: bool,
        #[dtcg(path = "flags.compact")]
        compact: Option<bool>,
    }
    let form = Form::project(&tokens, &dxn_dtcg::TypeRegistry).unwrap();
    assert_eq!(
        (form.submit.as_str(), form.visible, form.compact),
        ("Sign in", true, Some(false))
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn exported_documents_resolve_again_with_the_extensions() {
    let tokens = resolve_with(&document(), both()).unwrap();
    let exported = tokens.to_document();
    assert_eq!(exported["label"]["cancel"]["$type"], STRING_TYPE);
    assert_eq!(exported["hidden"]["$type"], BOOLEAN_TYPE);
    let again = resolve_with(&exported, both()).unwrap();
    for token in tokens.iter() {
        let other = again.get(&token.path).unwrap();
        assert_eq!(
            (&other.token_type, &other.value),
            (&token.token_type, &token.value)
        );
    }
    assert!(resolve_with(&exported, ValueExtensions::default()).is_err());
}
