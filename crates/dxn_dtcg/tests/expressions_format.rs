//! The token-level expression object is validated before evaluation when
//! expressions are enabled.
#![cfg(feature = "expressions")]

use dxn_dtcg::{
    DtcgError, DtcgRuntime, EXPRESSION_EXTENSION, ResolvedTokens, RuntimeOptions,
    expressions::ExpressionOptions,
};
use serde_json::{Value, json};

fn options() -> RuntimeOptions {
    RuntimeOptions::default().with_expressions(ExpressionOptions::default())
}

fn token(expression: Value) -> Value {
    json!({
        "base": { "$type": "number", "$value": 4 },
        "group": {
            "computed": {
                "$type": "number",
                "$value": 0,
                "$extensions": { EXPRESSION_EXTENSION: expression }
            }
        }
    })
}

fn resolve(document: &Value) -> Result<ResolvedTokens, DtcgError> {
    DtcgRuntime::from_json_with_options("tokens.json", document.to_string(), options())
        .unwrap()
        .resolve(&[])
        .map(|resolution| resolution.ready().unwrap())
}

/// Fails with `DtcgError::Expression` naming the token and a message that
/// contains `needle`.
fn assert_invalid(expression: Value, needle: &str) {
    match resolve(&token(expression.clone())) {
        Err(DtcgError::Expression {
            resource,
            path,
            message,
        }) => {
            assert_eq!(resource.as_str(), "tokens.json");
            assert_eq!(path, "group.computed");
            assert!(
                message.contains(needle),
                "{expression}: `{message}` lacks `{needle}`"
            );
        }
        other => panic!("{expression}: expected an expression error, got {other:?}"),
    }
}

#[test]
fn accepts_js_and_ts_with_inputs() {
    let tokens = resolve(&token(
        json!({ "inputs": { "base": "{base}" }, "js": "base + 1" }),
    ))
    .unwrap();
    assert_eq!(tokens.get("group.computed").unwrap().value, json!(5));
    let tokens = resolve(&token(json!({
        "inputs": { "base": "{base}", "$step": 2, "_ratio": 1.5 },
        "ts": "(base as number) * $step * _ratio"
    })))
    .unwrap();
    assert_eq!(tokens.get("group.computed").unwrap().value, json!(12));
}

#[test]
fn the_object_and_its_members_have_their_types() {
    assert_invalid(json!("base * 2"), "must be an object");
    assert_invalid(json!({ "js": 1 }), "`js` must be a string");
    assert_invalid(json!({ "ts": ["x"] }), "`ts` must be a string");
    assert_invalid(
        json!({ "js": "1", "inputs": ["base"] }),
        "`inputs` must be an object",
    );
    assert_invalid(
        json!({ "js": "1", "imports": "lib" }),
        "`imports` must be an object",
    );
    assert_invalid(
        json!({ "js": "1", "imports": { "lib": 3 } }),
        "must be an array of export names or a namespace name",
    );
    assert_invalid(
        json!({ "js": "1", "imports": { "lib": ["ok", 2] } }),
        "imported names must be strings",
    );
}

#[test]
fn exactly_one_of_js_or_ts() {
    assert_invalid(
        json!({ "js": "1", "ts": "1" }),
        "either `js` or `ts`, not both",
    );
    assert_invalid(json!({ "inputs": {} }), "needs `js` or `ts`");
}

#[test]
fn unknown_members_are_errors() {
    assert_invalid(
        json!({ "js": "1", "input": {} }),
        "unknown expression member `input`",
    );
    assert_invalid(
        json!({ "js": "1", "$value": 1 }),
        "unknown expression member `$value`",
    );
}

#[test]
fn local_names_are_identifiers_that_are_not_reserved_words() {
    assert_invalid(
        json!({ "js": "1", "inputs": { "2x": 1 } }),
        "`2x` is not a JavaScript identifier",
    );
    assert_invalid(
        json!({ "js": "1", "inputs": { "a-b": 1 } }),
        "`a-b` is not a JavaScript identifier",
    );
    assert_invalid(
        json!({ "js": "1", "inputs": { "": 1 } }),
        "is not a JavaScript identifier",
    );
    for reserved in [
        "class",
        "default",
        "await",
        "yield",
        "let",
        "eval",
        "arguments",
        "enum",
    ] {
        let mut inputs = serde_json::Map::new();
        inputs.insert(reserved.into(), json!(1));
        assert_invalid(
            json!({ "js": "1", "inputs": inputs }),
            &format!("`{reserved}` is a reserved word"),
        );
    }
    assert_invalid(
        json!({ "js": "1", "imports": { "lib": ["new"] } }),
        "`new` is a reserved word",
    );
    assert_invalid(
        json!({ "js": "1", "imports": { "lib": "1ns" } }),
        "`1ns` is not a JavaScript identifier",
    );
}

#[test]
fn local_names_are_unique_across_inputs_and_imports() {
    assert_invalid(
        json!({ "js": "1", "inputs": { "base": 1 }, "imports": { "lib": ["base"] } }),
        "duplicate local name `base`",
    );
    assert_invalid(
        json!({ "js": "1", "imports": { "a": ["mix"], "b": ["mix"] } }),
        "duplicate local name `mix`",
    );
    assert_invalid(
        json!({ "js": "1", "imports": { "a": "lib", "b": ["lib"] } }),
        "duplicate local name `lib`",
    );
}

#[test]
fn imports_name_modules_by_bare_specifier() {
    assert_invalid(
        json!({ "js": "1", "imports": { "": ["a"] } }),
        "must not be empty",
    );
    for relative in ["./lib.js", "../lib.js", "/lib.js"] {
        let mut imports = serde_json::Map::new();
        imports.insert(relative.into(), json!(["a"]));
        assert_invalid(json!({ "js": "1", "imports": imports }), "is relative");
    }
}

#[test]
fn a_valid_import_declaration_reaches_module_resolution() {
    // The object is valid; the module is simply unknown.
    assert_invalid(
        json!({ "js": "mix(1)", "imports": { "color-lib": ["mix"] } }),
        "unknown module `color-lib`",
    );
}

#[test]
fn value_stays_required() {
    // Without `$value` (or a token-level `$ref`) the object is a group with
    // `$extensions`, not a token, as for any other extension data.
    let mut document = token(json!({ "js": "1" }));
    document["group"]["computed"]
        .as_object_mut()
        .unwrap()
        .remove("$value");
    let tokens = resolve(&document).unwrap();
    assert!(tokens.get("group.computed").is_none());

    // A token-level `$ref` is a value like `$value`.
    let mut document = token(json!({ "inputs": { "base": "{base}" }, "js": "base * 3" }));
    let computed = document["group"]["computed"].as_object_mut().unwrap();
    computed.remove("$value");
    computed.insert("$ref".into(), json!("#/base/$value"));
    let tokens = resolve(&document).unwrap();
    let computed = tokens.get("group.computed").unwrap();
    assert_eq!(computed.value, json!(12));
    assert_eq!(computed.snapshot, Some(json!(4)));
}

#[test]
fn the_text_is_exactly_one_expression() {
    for source in [
        "1), (2",
        "1); } await 0; export function g() { return (1",
        "1;\nexport const leak = 2",
        "base }\nexport default function () { return 1",
    ] {
        assert_invalid(
            json!({ "js": source, "inputs": { "base": "{base}" } }),
            "SyntaxError",
        );
        assert_invalid(
            json!({ "ts": source, "inputs": { "base": "{base}" } }),
            "SyntaxError",
        );
    }
    // A sequence expression is one expression.
    let tokens = resolve(&token(
        json!({ "inputs": { "base": "{base}" }, "js": "(base, base * 2)" }),
    ))
    .unwrap();
    assert_eq!(tokens.get("group.computed").unwrap().value, json!(8));
}

#[test]
fn local_names_follow_the_javascript_identifier_grammar() {
    for invalid in ["a\u{b2}", "\u{2028}", "a-", "a.b", "\u{200d}a"] {
        let mut inputs = serde_json::Map::new();
        inputs.insert(invalid.into(), json!(1));
        assert_invalid(
            json!({ "js": "1", "inputs": inputs }),
            "is not a JavaScript identifier",
        );
    }
    // ID_Continue includes combining marks and the zero-width joiners.
    for valid in [
        "e\u{301}",
        "a\u{200d}b",
        "a\u{200c}",
        "\u{3c0}",
        "$\u{2118}",
        "_0",
    ] {
        let mut inputs = serde_json::Map::new();
        inputs.insert(valid.into(), json!(2));
        let tokens = resolve(&token(json!({ "js": valid, "inputs": inputs }))).unwrap();
        assert_eq!(
            tokens.get("group.computed").unwrap().value,
            json!(2),
            "{valid:?}"
        );
    }
}
