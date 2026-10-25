//! Expressions evaluate inside token resolution: the snapshot resolves as
//! before, inputs resolve like token values, and the result is converted,
//! rounded, bounded and validated against the token's type.
#![cfg(feature = "expressions")]

use dxn_dtcg::{
    DtcgError, DtcgRuntime, EXPRESSION_EXTENSION, ResolvedTokens, RuntimeLimits, RuntimeOptions,
    ValueExtensions, expressions::ExpressionOptions,
};
use serde_json::{Value, json};

fn options() -> RuntimeOptions {
    RuntimeOptions::default().with_expressions(ExpressionOptions::default())
}

fn resolve_with(document: &Value, options: RuntimeOptions) -> Result<ResolvedTokens, DtcgError> {
    DtcgRuntime::from_json_with_options("tokens.json", document.to_string(), options)
        .unwrap()
        .resolve(&[])
        .map(|resolution| resolution.ready().unwrap())
}

fn resolve(document: &Value) -> Result<ResolvedTokens, DtcgError> {
    resolve_with(document, options())
}

/// A token of `token_type` with snapshot `value` computed by `expression`.
fn computed(token_type: &str, value: Value, expression: Value) -> Value {
    json!({ "$type": token_type, "$value": value, "$extensions": { EXPRESSION_EXTENSION: expression } })
}

/// A document with `base` (4), `ratio` (1.5) and `computed`.
fn document(computed: Value) -> Value {
    json!({
        "base": { "$type": "number", "$value": 4 },
        "ratio": { "$type": "number", "$value": 1.5 },
        "computed": computed
    })
}

/// The value `js` computes for a token of `token_type` with snapshot
/// `snapshot`, inputs `base` and `ratio`.
fn evaluate(token_type: &str, snapshot: Value, js: &str) -> Result<Value, DtcgError> {
    let token = computed(
        token_type,
        snapshot,
        json!({ "inputs": { "base": "{base}", "ratio": "{ratio}" }, "js": js }),
    );
    resolve(&document(token)).map(|tokens| tokens.get("computed").unwrap().value.clone())
}

fn expression_message(result: Result<Value, DtcgError>) -> String {
    match result {
        Err(DtcgError::Expression { path, message, .. }) => {
            assert_eq!(path, "computed");
            message
        }
        other => panic!("expected an expression error, got {other:?}"),
    }
}

#[test]
fn the_result_replaces_the_value_and_the_resolved_value_becomes_the_snapshot() {
    let tokens = resolve(&document(computed(
        "number",
        json!("{base}"),
        json!({ "inputs": { "base": "{base}", "ratio": "{ratio}" }, "js": "base * ratio" }),
    )))
    .unwrap();
    let token = tokens.get("computed").unwrap();
    assert_eq!(token.value, json!(6));
    assert_eq!(token.snapshot, Some(json!(4)));
    assert_eq!(token.authored_value, json!("{base}"));
    assert_eq!(token.token_type.as_deref(), Some("number"));
    assert_eq!(tokens.get("base").unwrap().snapshot, None);
}

#[test]
fn the_snapshot_is_still_resolved_and_validated() {
    let error = evaluate("number", json!("not a number"), "base").unwrap_err();
    assert!(matches!(error, DtcgError::TokenType { ref path, .. } if path == "computed"));
    let error = evaluate("number", json!("{missing}"), "base").unwrap_err();
    assert!(
        matches!(error, DtcgError::UnresolvedReference { .. }),
        "{error:?}"
    );
}

#[test]
fn the_result_is_validated_against_the_effective_type() {
    let error = evaluate("dimension", json!({ "value": 4, "unit": "px" }), "base").unwrap_err();
    match error {
        DtcgError::TokenType { path, message, .. } => {
            assert_eq!(path, "computed");
            assert!(message.contains("expression result"), "{message}");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        evaluate(
            "dimension",
            json!({ "value": 4, "unit": "px" }),
            "({ value: base * 2, unit: 'rem' })"
        )
        .unwrap(),
        json!({ "value": 8, "unit": "rem" })
    );
}

#[test]
fn the_effective_type_comes_from_the_snapshot_as_without_expressions() {
    // No own type: the type of the token `$value` aliases.
    let document = json!({
        "space": { "$type": "dimension", "$value": { "value": 4, "unit": "px" } },
        "computed": {
            "$value": "{space}",
            "$extensions": { EXPRESSION_EXTENSION: {
                "inputs": { "space": "{space}" },
                "js": "({ ...space, value: space.value * 3 })"
            } }
        }
    });
    let tokens = resolve(&document).unwrap();
    let token = tokens.get("computed").unwrap();
    assert_eq!(token.token_type.as_deref(), Some("dimension"));
    assert_eq!(token.value, json!({ "value": 12, "unit": "px" }));
}

#[test]
fn results_validate_with_the_runtime_value_extensions() {
    let typography = |line_height: &str| {
        json!({
            "text": computed(
                "typography",
                json!({
                    "fontFamily": "Inter",
                    "fontSize": { "value": 16, "unit": "px" },
                    "fontWeight": 400,
                    "letterSpacing": { "value": 0, "unit": "px" },
                    "lineHeight": 1.5
                }),
                json!({ "js": format!(
                    "({{ fontFamily: 'Inter', fontSize: {{ value: 16, unit: 'px' }}, fontWeight: 400, letterSpacing: {{ value: 0, unit: 'px' }}, lineHeight: {line_height} }})"
                ) }),
            )
        })
    };
    let dimension = typography("{ value: 24, unit: 'px' }");
    assert!(matches!(
        resolve(&dimension),
        Err(DtcgError::TokenType { .. })
    ));
    let extended =
        options().with_extensions(ValueExtensions::default().with_line_height_dimension(true));
    let tokens = resolve_with(&dimension, extended).unwrap();
    assert_eq!(
        tokens.get("text").unwrap().value["lineHeight"],
        json!({ "value": 24, "unit": "px" })
    );
}

#[test]
fn inputs_resolve_aliases_pointers_and_nested_literals() {
    let document = json!({
        "$extensions": { "vendor.palette": { "scale": [1, 2, 3] } },
        "base": { "$type": "number", "$value": 4 },
        "space": { "$type": "dimension", "$value": { "value": 8, "unit": "px" } },
        "computed": computed("number", json!(0), json!({
            "inputs": {
                "alias": "{base}",
                "pointer": { "$ref": "#/space/$value/value" },
                "document": { "$ref": "#/$extensions/vendor.palette/scale" },
                "nested": { "list": ["{base}", { "$ref": "#/base/$value" }, 5], "literal": "text" }
            },
            "js": "alias + pointer + document[2] + nested.list[0] + nested.list[1] + nested.list[2] + nested.literal.length"
        }))
    });
    let tokens = resolve(&document).unwrap();
    assert_eq!(tokens.get("computed").unwrap().value, json!(32));
}

#[test]
fn a_missing_input_target_is_an_unresolved_reference() {
    let error = resolve(&document(computed(
        "number",
        json!(0),
        json!({ "inputs": { "x": "{nowhere}" }, "js": "x" }),
    )))
    .unwrap_err();
    match error {
        DtcgError::UnresolvedReference { path, target, .. } => {
            assert_eq!(path, "computed");
            assert_eq!(target, "nowhere");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn input_references_count_toward_the_alias_depth() {
    let document = json!({
        "a": { "$type": "number", "$value": 1 },
        "b": { "$type": "number", "$value": "{a}" },
        "c": { "$type": "number", "$value": "{b}" },
        "computed": computed("number", json!(0), json!({ "inputs": { "c": "{c}" }, "js": "c" }))
    });
    let limited = options().with_limits(RuntimeLimits::default().with_max_alias_depth(2));
    assert!(matches!(
        resolve_with(&document, limited),
        Err(DtcgError::LimitExceeded {
            kind: "alias depth",
            ..
        })
    ));
    let enough = options().with_limits(RuntimeLimits::default().with_max_alias_depth(3));
    assert_eq!(
        resolve_with(&document, enough)
            .unwrap()
            .get("computed")
            .unwrap()
            .value,
        json!(1)
    );
}

#[test]
fn input_references_count_toward_value_nodes_and_depth() {
    let list = (0..50).collect::<Vec<_>>();
    let document = json!({
        "list": { "$type": "cubicBezier", "$value": [0, 0, 1, 1] },
        "big": { "$type": "fontFamily", "$value": list.iter().map(|n| format!("f{n}")).collect::<Vec<_>>() },
        "computed": computed("number", json!(0), json!({
            "inputs": { "a": "{big}", "b": "{big}" },
            "js": "a.length + b.length"
        }))
    });
    assert_eq!(
        resolve(&document).unwrap().get("computed").unwrap().value,
        json!(100)
    );
    let limited = options().with_limits(RuntimeLimits::default().with_max_value_nodes(100));
    assert!(matches!(
        resolve_with(&document, limited),
        Err(DtcgError::LimitExceeded {
            kind: "value nodes",
            ..
        })
    ));

    // A pointer to a 100-level value placed 40 levels deep in an input.
    let mut deep = json!(1);
    for _ in 0..99 {
        deep = json!([deep]);
    }
    let mut input = json!({ "$ref": "#/$extensions/vendor.deep" });
    for _ in 0..40 {
        input = json!([input]);
    }
    let document = json!({
        "$extensions": { "vendor.deep": deep },
        "computed": computed("number", json!(0), json!({ "inputs": { "x": input }, "js": "1" }))
    });
    assert!(matches!(
        resolve(&document),
        Err(DtcgError::LimitExceeded {
            kind: "value depth",
            ..
        })
    ));
}

#[test]
fn a_cycle_through_inputs_is_an_alias_cycle() {
    let own = resolve(&document(computed(
        "number",
        json!(0),
        json!({ "inputs": { "me": "{computed}" }, "js": "me + 1" }),
    )))
    .unwrap_err();
    assert!(
        matches!(own, DtcgError::Token { ref message, .. } if message.contains("alias cycle")),
        "{own:?}"
    );

    let document = json!({
        "a": computed("number", json!(0), json!({ "inputs": { "b": "{b}" }, "js": "b + 1" })),
        "b": computed("number", json!(0), json!({ "inputs": { "a": { "$ref": "#/a/$value" } }, "js": "a + 1" }))
    });
    let error = resolve(&document).unwrap_err();
    assert!(
        matches!(error, DtcgError::Token { ref message, .. } if message.contains("alias cycle")),
        "{error:?}"
    );
}

#[test]
fn references_to_an_expression_token_receive_its_evaluated_value() {
    let document = json!({
        "base": { "$type": "number", "$value": 4 },
        "space": computed(
            "dimension",
            json!({ "value": 4, "unit": "px" }),
            json!({ "inputs": { "base": "{base}" }, "js": "({ value: base * 2, unit: 'px' })" }),
        ),
        "alias": { "$type": "dimension", "$value": "{space}" },
        "pointer": { "$type": "number", "$value": { "$ref": "#/space/$value/value" } },
        "chained": computed("number", json!(0), json!({ "inputs": { "space": "{space}" }, "js": "space.value + 1" }))
    });
    let tokens = resolve(&document).unwrap();
    assert_eq!(
        tokens.get("alias").unwrap().value,
        json!({ "value": 8, "unit": "px" })
    );
    assert_eq!(tokens.get("alias").unwrap().snapshot, None);
    assert_eq!(tokens.get("pointer").unwrap().value, json!(8));
    assert_eq!(tokens.get("chained").unwrap().value, json!(9));
}

#[test]
fn inputs_are_bound_by_name_as_json_values() {
    assert_eq!(
        evaluate(
            "number",
            json!(0),
            "typeof base === 'number' && typeof ratio === 'number' ? base + ratio : -1"
        )
        .unwrap(),
        json!(5.5)
    );
    let document = json!({
        "accent": { "$type": "color", "$value": { "colorSpace": "srgb", "components": [0.1, 0.2, 0.3], "alpha": 0.5 } },
        "computed": computed("number", json!(0), json!({
            "inputs": { "accent": "{accent}" },
            "js": "Object.keys(accent).length * 10 + accent.components.length"
        }))
    });
    assert_eq!(
        resolve(&document).unwrap().get("computed").unwrap().value,
        json!(33)
    );
}

#[test]
fn results_convert_to_json() {
    // Arrays, plain objects, strings and numbers.
    assert_eq!(
        evaluate("cubicBezier", json!([0, 0, 1, 1]), "[0.25, 0.1, 0.25, 1.0]").unwrap(),
        json!([0.25, 0.1, 0.25, 1])
    );
    assert_eq!(
        evaluate("fontFamily", json!("Inter"), "['Inter', 'sans-' + 'serif']").unwrap(),
        json!(["Inter", "sans-serif"])
    );
    let shadow = evaluate(
        "shadow",
        json!({
            "color": { "colorSpace": "srgb", "components": [0, 0, 0] },
            "offsetX": { "value": 0, "unit": "px" },
            "offsetY": { "value": 0, "unit": "px" },
            "blur": { "value": 0, "unit": "px" },
            "spread": { "value": 0, "unit": "px" }
        }),
        "({ color: { colorSpace: 'srgb', components: [0, 0, 0], alpha: 0.25 }, offsetX: { value: 0, unit: 'px' }, offsetY: { value: base / 2, unit: 'px' }, blur: { value: base * ratio, unit: 'px' }, spread: { value: 0, unit: 'px' } })",
    )
    .unwrap();
    assert_eq!(shadow["offsetY"]["value"], json!(2));
    assert_eq!(shadow["blur"]["value"], json!(6));
    // `null`, booleans and objects convert; the type then rejects them.
    for js in ["null", "true", "({ value: 1 })", "Object.create(null)"] {
        assert!(
            matches!(
                evaluate("number", json!(0), js),
                Err(DtcgError::TokenType { .. })
            ),
            "{js}"
        );
    }
}

#[test]
fn integral_numbers_become_integers() {
    let value = evaluate("number", json!(0), "base * 0.5").unwrap();
    assert!(value.is_i64(), "{value:?}");
    assert_eq!(value, json!(2));
    let value = evaluate("number", json!(0), "2 ** 53").unwrap();
    assert_eq!(value.as_i64(), Some(9_007_199_254_740_992));
    let value = evaluate("number", json!(0), "-(2 ** 53)").unwrap();
    assert_eq!(value.as_i64(), Some(-9_007_199_254_740_992));
    let value = evaluate("number", json!(0), "2 ** 53 + 2").unwrap();
    assert!(value.is_f64(), "{value:?}");
    let value = evaluate("number", json!(0), "-0").unwrap();
    assert_eq!(value, json!(0));
}

#[test]
fn values_that_are_not_json_are_rejected() {
    for (js, kind) in [
        ("undefined", "undefined"),
        ("() => 1", "a function"),
        ("Symbol('x')", "a symbol"),
        ("10n", "a bigint"),
        ("0 / 0", "NaN"),
        ("1 / 0", "an infinity"),
        ("-1 / 0", "an infinity"),
        ("new Map()", "an object with a prototype"),
        ("[1, undefined]", "undefined"),
    ] {
        let message = expression_message(evaluate("number", json!(0), js));
        assert!(message.contains(kind), "{js}: {message}");
        assert!(message.contains("not a JSON value"), "{js}: {message}");
    }
    let message = expression_message(evaluate(
        "cubicBezier",
        json!([0, 0, 1, 1]),
        "[0, 0, 1, { a: () => 1 }]",
    ));
    assert!(
        message.contains("the result at `[3].a` is a function"),
        "{message}"
    );
}

#[test]
fn results_nest_at_most_128_levels_and_count_toward_value_nodes() {
    let nested = |levels: usize| {
        format!("(() => {{ let v = 1; for (let i = 1; i < {levels}; i++) v = [v]; return v; }})()")
    };
    // 128 levels convert (and the type then rejects the array).
    assert!(matches!(
        evaluate("number", json!(0), &nested(128)),
        Err(DtcgError::TokenType { .. })
    ));
    assert!(matches!(
        evaluate("number", json!(0), &nested(129)),
        Err(DtcgError::LimitExceeded {
            kind: "value depth",
            limit: 128
        })
    ));
    assert!(matches!(
        evaluate(
            "number",
            json!(0),
            "(() => { const v = {}; v.self = v; return v; })()"
        ),
        Err(DtcgError::LimitExceeded {
            kind: "value depth",
            ..
        })
    ));

    let document = document(computed(
        "fontFamily",
        json!("x"),
        json!({ "js": "Array.from({ length: 200 }, (_, i) => 'f' + i)" }),
    ));
    assert_eq!(
        resolve(&document)
            .unwrap()
            .get("computed")
            .unwrap()
            .value
            .as_array()
            .unwrap()
            .len(),
        200
    );
    let limited = options().with_limits(RuntimeLimits::default().with_max_value_nodes(100));
    assert!(matches!(
        resolve_with(&document, limited),
        Err(DtcgError::LimitExceeded {
            kind: "value nodes",
            limit: 100
        })
    ));
}

#[test]
fn non_integral_numbers_round_to_the_configured_precision() {
    assert_eq!(
        evaluate("number", json!(0), "1 / 3").unwrap(),
        json!(0.333333)
    );
    assert_eq!(
        evaluate("number", json!(0), "2 / 3").unwrap(),
        json!(0.666667)
    );
    assert_eq!(
        evaluate("number", json!(0), "0.1 + 0.2").unwrap(),
        json!(0.3)
    );
    assert_eq!(evaluate("number", json!(0), "1.0000001").unwrap(), json!(1));
    let token = computed("number", json!(0), json!({ "js": "1 / 3" }));
    let unrounded = RuntimeOptions::default()
        .with_expressions(ExpressionOptions::default().with_precision(None));
    assert_eq!(
        resolve_with(&document(token.clone()), unrounded)
            .unwrap()
            .get("computed")
            .unwrap()
            .value,
        json!(1.0 / 3.0)
    );
    let two = RuntimeOptions::default()
        .with_expressions(ExpressionOptions::default().with_precision(Some(2)));
    assert_eq!(
        resolve_with(&document(token), two)
            .unwrap()
            .get("computed")
            .unwrap()
            .value,
        json!(0.33)
    );
}

#[test]
fn strings_in_results_are_literals() {
    assert_eq!(
        evaluate("fontFamily", json!("x"), "'{base}'").unwrap(),
        json!("{base}")
    );
    assert_eq!(
        evaluate("fontFamily", json!(["x"]), "['{ratio}']").unwrap(),
        json!(["{ratio}"])
    );
}

#[test]
fn an_expression_replaces_the_whole_value_and_groups_are_not_evaluated() {
    let document = json!({
        "group": {
            "$extensions": { EXPRESSION_EXTENSION: { "js": "throw new Error('groups never evaluate')" } },
            "base": { "$type": "number", "$value": 4 }
        },
        "border": computed(
            "border",
            json!({ "color": { "colorSpace": "srgb", "components": [0, 0, 0] }, "width": { "value": 1, "unit": "px" }, "style": "solid" }),
            json!({ "inputs": { "base": "{group.base}" }, "js": "({ color: { colorSpace: 'srgb', components: [1, 0, 0] }, width: { value: base / 4, unit: 'px' }, style: 'dashed' })" }),
        )
    });
    let tokens = resolve(&document).unwrap();
    assert_eq!(
        tokens.get("border").unwrap().value,
        json!({ "color": { "colorSpace": "srgb", "components": [1, 0, 0] }, "width": { "value": 1, "unit": "px" }, "style": "dashed" })
    );
}
