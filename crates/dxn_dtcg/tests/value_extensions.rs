use dxn_dtcg::{
    BLUR_TYPE, DtcgError, DtcgRuntime, ResolvedTokens, RuntimeLimits, RuntimeOptions,
    ValueExtensions, ValueValidationError, validate_value, validate_value_with,
};
use serde_json::{Value, json};

fn typography(line_height: Value) -> Value {
    json!({
        "fontFamily": "Inter",
        "fontSize": {"value": 32, "unit": "px"},
        "fontWeight": 700,
        "letterSpacing": {"value": 0, "unit": "px"},
        "lineHeight": line_height,
    })
}

fn blur() -> Value {
    json!({"kind": "layer", "radius": {"value": 4, "unit": "px"}})
}

/// A document using both RayX extensions: a typography token whose
/// `lineHeight` aliases a dimension token, a literal dimension `lineHeight`,
/// and a blur token.
fn rayx_document() -> Value {
    json!({
        "line": {"$type": "dimension", "$value": {"value": 40, "unit": "px"}},
        "heading": {"$type": "typography", "$value": typography(json!("{line}"))},
        "literal": {"$type": "typography", "$value": typography(json!({"value": 2, "unit": "rem"}))},
        "overlay": {"$type": BLUR_TYPE, "$value": blur()}
    })
}

fn resolve(runtime: &DtcgRuntime) -> Result<ResolvedTokens, DtcgError> {
    runtime
        .resolve(&[])
        .map(|resolution| resolution.ready().expect("in-memory document"))
}

#[test]
fn validation_is_strict_by_default() {
    assert_eq!(
        ValueExtensions::default(),
        ValueExtensions::rayx()
            .with_line_height_dimension(false)
            .with_rayx_blur(false)
    );
    validate_value("typography", &typography(json!(1.5))).expect("number line height");
    assert_eq!(
        validate_value(
            "typography",
            &typography(json!({"value": 40, "unit": "px"}))
        ),
        Err(ValueValidationError::InvalidValue("typography".into()))
    );
    assert_eq!(
        validate_value(BLUR_TYPE, &blur()),
        Err(ValueValidationError::UnsupportedType(BLUR_TYPE.into()))
    );
    assert_eq!(
        validate_value_with(BLUR_TYPE, &blur(), &ValueExtensions::default()),
        validate_value(BLUR_TYPE, &blur())
    );
}

#[test]
fn each_extension_flag_is_independent() {
    let line_height = ValueExtensions::default().with_line_height_dimension(true);
    validate_value_with(
        "typography",
        &typography(json!({"value": 40, "unit": "px"})),
        &line_height,
    )
    .expect("dimension line height");
    assert!(validate_value_with(BLUR_TYPE, &blur(), &line_height).is_err());

    let blur_only = ValueExtensions::default().with_rayx_blur(true);
    validate_value_with(BLUR_TYPE, &blur(), &blur_only).expect("blur");
    assert!(
        validate_value_with(
            "typography",
            &typography(json!({"value": 40, "unit": "px"})),
            &blur_only
        )
        .is_err()
    );
}

#[test]
fn a_default_runtime_rejects_the_rayx_shapes() {
    let runtime = DtcgRuntime::from_json("tokens.json", rayx_document().to_string()).unwrap();
    let error = resolve(&runtime).unwrap_err();
    assert!(
        matches!(&error, DtcgError::TokenType { path, message, .. }
            if path == "heading"
                && message == "reference type mismatch: expected number, found dimension"),
        "{error:?}"
    );

    for (path, document) in [
        (
            "literal",
            json!({"literal": {"$type": "typography", "$value": typography(json!({"value": 2, "unit": "rem"}))}}),
        ),
        (
            "overlay",
            json!({"overlay": {"$type": BLUR_TYPE, "$value": blur()}}),
        ),
    ] {
        let runtime = DtcgRuntime::from_json("tokens.json", document.to_string()).unwrap();
        let error = resolve(&runtime).unwrap_err();
        assert!(
            matches!(&error, DtcgError::TokenType { path: reported, .. } if reported == path),
            "{error:?}"
        );
    }
}

#[test]
fn the_rayx_profile_accepts_the_extensions_through_every_constructor() {
    let options = RuntimeOptions::default().with_extensions(ValueExtensions::rayx());
    let expect_ready = |runtime: &DtcgRuntime| {
        let tokens = resolve(runtime).unwrap();
        assert_eq!(
            tokens.get("heading").unwrap().value["lineHeight"],
            json!({"value": 40, "unit": "px"})
        );
        assert_eq!(
            tokens.get("overlay").unwrap().token_type.as_deref(),
            Some(BLUR_TYPE)
        );
    };

    let runtime = DtcgRuntime::from_json_with_options(
        "tokens.json",
        rayx_document().to_string(),
        options.clone(),
    )
    .unwrap();
    assert_eq!(runtime.options(), &options);
    expect_ready(&runtime);

    let mut runtime = DtcgRuntime::with_options(options.clone());
    runtime
        .provide_json("tokens.json", rayx_document().to_string())
        .unwrap();
    runtime.set_root("tokens.json").unwrap();
    expect_ready(&runtime);

    let envelope = json!({"data": {
        "version": "2025.10",
        "resolutionOrder": [{"type": "set", "name": "base", "sources": [rayx_document()]}]
    }});
    let (runtime, _) = DtcgRuntime::from_envelope_with_options(
        "project.tokeneditor",
        &envelope.to_string(),
        options.clone(),
    )
    .unwrap();
    expect_ready(&runtime);
    let (strict, _) =
        DtcgRuntime::from_envelope("project.tokeneditor", &envelope.to_string()).unwrap();
    assert!(resolve(&strict).is_err());

    let limited = DtcgRuntime::with_limits(RuntimeLimits::default().with_max_tokens(7));
    assert_eq!(limited.options().extensions, ValueExtensions::default());
    assert_eq!(limited.limits().max_tokens, 7);
}

#[test]
fn the_rayx_profile_keeps_every_other_rule_strict() {
    let options = RuntimeOptions::default().with_extensions(ValueExtensions::rayx());
    for (document, path) in [
        (
            json!({"line": {"$type": "duration", "$value": {"value": 1, "unit": "s"}},
                   "heading": {"$type": "typography", "$value": typography(json!("{line}"))}}),
            "heading",
        ),
        (
            json!({"heading": {"$type": "typography", "$value": typography(json!({"value": 40, "unit": "em"}))}}),
            "heading",
        ),
        (
            json!({"overlay": {"$type": BLUR_TYPE, "$value": {"kind": "motion", "radius": {"value": 4, "unit": "px"}}}}),
            "overlay",
        ),
        (
            json!({"other": {"$type": "dev.rayx.other", "$value": 1}}),
            "other",
        ),
        (
            json!({"c": {"$type": "color", "$value": {"colorSpace": "srgb", "components": [0, 0, 0], "alpha": "{n}"}},
                   "n": {"$type": "number", "$value": 1}}),
            "c",
        ),
    ] {
        let runtime = DtcgRuntime::from_json_with_options(
            "tokens.json",
            document.to_string(),
            options.clone(),
        )
        .unwrap();
        let error = resolve(&runtime).unwrap_err();
        let reported = match &error {
            DtcgError::TokenType { path, .. } | DtcgError::Token { path, .. } => path.as_str(),
            other => panic!("{document}: {other:?}"),
        };
        assert_eq!(reported, path, "{document}: {error:?}");
    }
}
