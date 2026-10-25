use dxn_dtcg::{DtcgError, DtcgRuntime, RuntimeLimits};
use serde_json::{Map, Value, json};
use wasm_bindgen_test::wasm_bindgen_test;

fn is_limit(result: &Result<impl std::fmt::Debug, DtcgError>, expected: &str) -> bool {
    matches!(result, Err(DtcgError::LimitExceeded { kind, .. }) if *kind == expected)
}

/// `g0` holds four number tokens; every `g<i>` has two members extending
/// `g<i-1>`, so the flattened token count doubles at each level.
fn doubling_extends(levels: usize) -> Value {
    let mut document = Map::new();
    document.insert(
        "g0".into(),
        json!({"$type": "number", "a": {"$value": 1}, "b": {"$value": 2}, "c": {"$value": 3}, "d": {"$value": 4}}),
    );
    for level in 1..=levels {
        let base = format!("{{g{}}}", level - 1);
        document.insert(
            format!("g{level}"),
            json!({"x": {"$extends": base}, "y": {"$extends": base}}),
        );
    }
    Value::Object(document)
}

#[wasm_bindgen_test(unsupported = test)]
fn doubling_extends_fan_out_hits_the_token_limit() {
    let document = doubling_extends(30);
    assert!(document.to_string().len() < 2_000);
    let runtime = DtcgRuntime::from_json("tokens.json", document.to_string()).unwrap();
    assert!(is_limit(&runtime.resolve(&[]), "tokens"));

    let preview = DtcgRuntime::from_json("tokens.json", "{}").unwrap();
    assert!(is_limit(&preview.preview_source(&document, &[]), "tokens"));
}

#[wasm_bindgen_test(unsupported = test)]
fn max_tokens_counts_every_flattened_token() {
    let document = json!({
        "a": {"$type": "number", "$value": 1},
        "b": {"$type": "number", "$value": 2},
        "c": {"$extends": "{d}"},
        "d": {"e": {"$type": "number", "$value": 3}}
    });
    let runtime = |limit: usize| {
        let mut runtime = DtcgRuntime::with_limits(RuntimeLimits::default().with_max_tokens(limit));
        runtime
            .provide_json("tokens.json", document.to_string())
            .unwrap();
        runtime.set_root("tokens.json").unwrap();
        runtime
    };
    // a, b, c.e (copied by `$extends`) and d.e.
    assert_eq!(runtime(4).resolve(&[]).unwrap().ready().unwrap().len(), 4);
    assert!(is_limit(&runtime(3).resolve(&[]), "tokens"));
    assert_eq!(RuntimeLimits::default().max_tokens, 100_000);
}

#[wasm_bindgen_test(unsupported = test)]
fn a_linear_chain_of_four_hundred_extends_groups_resolves() {
    let mut document = Map::new();
    document.insert("g0".into(), json!({"$type": "number", "t0": {"$value": 0}}));
    for index in 1..400 {
        document.insert(
            format!("g{index}"),
            json!({"$extends": format!("{{g{}}}", index - 1), format!("t{index}"): {"$value": index}}),
        );
    }
    let runtime =
        DtcgRuntime::from_json("tokens.json", Value::Object(document).to_string()).unwrap();
    let tokens = runtime.resolve(&[]).unwrap().ready().unwrap();
    assert_eq!(tokens.len(), 400 * 401 / 2);
    let deepest = tokens.get("g399.t0").unwrap();
    assert_eq!(deepest.value, json!(0));
    assert_eq!(deepest.token_type.as_deref(), Some("number"));
    assert_eq!(
        tokens.get("g399.t399").unwrap().token_type.as_deref(),
        Some("number")
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn nested_extends_copies_are_bounded_in_depth() {
    // Every `g<i>.a` extends `g<i-1>`, so `g<n>` flattens n levels deep.
    let mut document = Map::new();
    document.insert("g0".into(), json!({"t": {"$type": "number", "$value": 1}}));
    for index in 1..200 {
        document.insert(
            format!("g{index}"),
            json!({"a": {"$extends": format!("{{g{}}}", index - 1)}}),
        );
    }
    let runtime =
        DtcgRuntime::from_json("tokens.json", Value::Object(document).to_string()).unwrap();
    assert!(is_limit(&runtime.resolve(&[]), "path depth"));
}

/// Sets `s0`–`s11` each reference the next set four times; `s12` is empty.
fn set_fan_out() -> Value {
    let mut sets = Map::new();
    for level in 0..12 {
        let next = json!({"$ref": format!("#/sets/s{}", level + 1)});
        sets.insert(
            format!("s{level}"),
            json!({"sources": [next, next, next, next]}),
        );
    }
    sets.insert("s12".into(), json!({"sources": []}));
    json!({
        "version": "2025.10",
        "sets": sets,
        "resolutionOrder": [{"$ref": "#/sets/s0"}]
    })
}

#[wasm_bindgen_test(unsupported = test)]
fn empty_set_fan_out_counts_against_the_document_limit() {
    let document = set_fan_out();
    assert!(document.to_string().len() < 1_500);
    let runtime = DtcgRuntime::from_json("root.json", document.to_string()).unwrap();
    assert!(is_limit(&runtime.resolve(&[]), "documents"));
    assert!(is_limit(&runtime.validate_complete(), "documents"));
    assert!(is_limit(
        &runtime.inspect_source(&json!({"$ref": "#/sets/s0"}), &[]),
        "documents"
    ));
}

#[wasm_bindgen_test(unsupported = test)]
fn expansion_steps_below_the_document_limit_still_resolve() {
    let runtime = DtcgRuntime::from_json(
        "root.json",
        json!({
            "version": "2025.10",
            "sets": {
                "empty": {"sources": []},
                "base": {"sources": [{"$ref": "#/sets/empty"}, {"a": {"$type": "number", "$value": 1}}]}
            },
            "resolutionOrder": [{"$ref": "#/sets/base"}, {"$ref": "#/sets/empty"}]
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(runtime.resolve(&[]).unwrap().ready().unwrap().len(), 1);
    assert!(runtime.validate_complete().unwrap().is_ready());
}
