use dxn_dtcg::{DtcgError, DtcgRuntime};
use serde_json::{Value, json};
use wasm_bindgen_test::wasm_bindgen_test;

fn resolve(document: Value) -> Result<dxn_dtcg::ResolvedTokens, DtcgError> {
    DtcgRuntime::from_json("tokens.json", document.to_string())
        .unwrap()
        .resolve(&[])
        .map(|resolution| resolution.ready().expect("in-memory document"))
}

fn assert_token_error(document: Value, expected_path: &str, expected_message: &str) {
    match resolve(document.clone()) {
        Err(DtcgError::Token {
            resource,
            path,
            message,
        }) => {
            assert_eq!(resource.as_str(), "tokens.json", "{document}");
            assert_eq!(path, expected_path, "{document}");
            assert_eq!(message, expected_message, "{document}");
        }
        other => panic!("{document} must fail with a token error, got {other:?}"),
    }
}

fn number(value: i64) -> Value {
    json!({"$type": "number", "$value": value})
}

#[wasm_bindgen_test(unsupported = test)]
fn tokens_must_not_have_children_or_a_root() {
    assert_token_error(
        json!({"g": {"$type": "number", "$value": 1, "child": number(2)}}),
        "g",
        "token `g` must not have children",
    );
    assert_token_error(
        json!({"g": {"inner": {"$type": "number", "$value": 1, "$root": number(2)}}}),
        "g.inner",
        "token `g.inner` must not have children",
    );
    assert_token_error(
        json!({
            "$extensions": {"x": 1},
            "g": {"$type": "number", "$ref": "#/$extensions/x", "child": number(2)}
        }),
        "g",
        "token `g` must not have children",
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn a_token_must_not_have_both_value_and_ref() {
    assert_token_error(
        json!({
            "base": number(1),
            "both": {"$type": "number", "$value": 2, "$ref": "#/base/$value"}
        }),
        "both",
        "a token must not have both `$value` and `$ref`",
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn root_members_must_be_tokens() {
    assert_token_error(
        json!({"g": {"$type": "number", "$root": {"x": number(1)}}}),
        "g.$root",
        "`$root` must be a token",
    );
    assert_token_error(
        json!({"g": {"$root": {}}}),
        "g.$root",
        "`$root` must be a token",
    );
    assert_token_error(
        json!({"g": {"$root": 1}}),
        "g.$root",
        "`$root` must be a token",
    );

    let tokens =
        resolve(json!({"g": {"$type": "number", "$root": {"$value": 1}, "x": {"$value": 2}}}))
            .unwrap();
    assert_eq!(
        tokens
            .iter()
            .map(|token| token.path.as_ref())
            .collect::<Vec<_>>(),
        ["g.$root", "g.x"]
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn a_source_root_is_never_a_token() {
    assert_token_error(
        json!({"$type": "number", "$ref": "#/$extensions/x", "$extensions": {"x": 1}}),
        "",
        "a source root must not be a token",
    );
    assert_token_error(
        json!({"$type": "number", "$value": 1}),
        "",
        "a source root must not be a token",
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn names_must_not_be_empty() {
    assert_token_error(
        json!({"": number(1)}),
        "",
        "token and group names must not be empty",
    );
    assert_token_error(
        json!({"g": {"": number(1)}}),
        "g.",
        "token and group names must not be empty",
    );
    assert_token_error(
        json!({"g": {"": {"inner": number(1)}}}),
        "g.",
        "token and group names must not be empty",
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn metadata_members_must_have_their_types_on_tokens_groups_and_roots() {
    for (member, invalid, message) in [
        ("$description", json!(1), "`$description` must be a string"),
        (
            "$description",
            json!({"text": "x"}),
            "`$description` must be a string",
        ),
        (
            "$deprecated",
            json!(1),
            "`$deprecated` must be a boolean or a string",
        ),
        (
            "$deprecated",
            json!({"reason": "x"}),
            "`$deprecated` must be a boolean or a string",
        ),
        (
            "$deprecated",
            json!(null),
            "`$deprecated` must be a boolean or a string",
        ),
        ("$extensions", json!("x"), "`$extensions` must be an object"),
        ("$extensions", json!([1]), "`$extensions` must be an object"),
    ] {
        let mut token = number(1);
        token[member] = invalid.clone();
        assert_token_error(json!({"g": {"t": token}}), "g.t", message);

        let mut group = json!({"t": number(1)});
        group[member] = invalid.clone();
        assert_token_error(json!({"g": group}), "g", message);

        let mut root = json!({"t": number(1)});
        root[member] = invalid;
        assert_token_error(root, "", message);
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn well_typed_metadata_is_accepted_everywhere() {
    let tokens = resolve(json!({
        "$description": "root",
        "$extensions": {"vendor": {"x": 1}},
        "g": {
            "$description": "group",
            "$deprecated": "use h",
            "$extensions": {},
            "a": {"$type": "number", "$value": 1, "$description": "a", "$deprecated": true, "$extensions": {"k": [1]}},
            "b": {"$type": "number", "$value": 2, "$deprecated": false}
        }
    }))
    .unwrap();
    assert_eq!(tokens.len(), 2);
}

#[wasm_bindgen_test(unsupported = test)]
fn extends_belongs_to_groups_and_targets_groups() {
    assert_token_error(
        json!({"t": {"$type": "number", "$value": 1, "$extends": "{missing}"}}),
        "t",
        "`$extends` is only allowed on groups",
    );
    for target in [
        json!({"$type": "number", "$value": 1}),
        json!({"$type": "number", "$ref": "#/$extensions/x"}),
    ] {
        assert_token_error(
            json!({"$extensions": {"x": 1}, "target": target, "g": {"$extends": "{target}"}}),
            "g",
            "`$extends` target `{target}` must be a group",
        );
    }
}
