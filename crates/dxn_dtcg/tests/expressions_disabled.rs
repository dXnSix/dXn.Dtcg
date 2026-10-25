//! Without the `expressions` feature, or with it but without
//! `RuntimeOptions::with_expressions`, an expression extension is opaque
//! metadata: values come from `$value` and nothing validates the extension.

use dxn_dtcg::{DtcgRuntime, EXPRESSION_EXTENSION, ResolvedTokens, RuntimeOptions};
use serde_json::{Value, json};
use wasm_bindgen_test::wasm_bindgen_test;

fn document(with_extension: bool) -> Value {
    let mut document = json!({
        "base": { "$type": "number", "$value": 4 },
        "large": { "$type": "number", "$value": 16 },
        "accent": {
            "$type": "color",
            "$value": { "colorSpace": "srgb", "components": [0.2, 0.4, 0.9] }
        }
    });
    if with_extension {
        document["large"]["$extensions"] = json!({
            EXPRESSION_EXTENSION: { "inputs": { "base": "{base}" }, "js": "base * 5" }
        });
        document["accent"]["$extensions"] = json!({
            EXPRESSION_EXTENSION: { "js": 42, "unknown": true, "inputs": [] },
            "vendor.other": { "kept": true }
        });
        document["$extensions"] = json!({ EXPRESSION_EXTENSION: { "modules": 7 } });
    }
    document
}

fn resolve(document: &Value, options: RuntimeOptions) -> ResolvedTokens {
    DtcgRuntime::from_json_with_options("tokens.json", document.to_string(), options)
        .unwrap()
        .resolve(&[])
        .unwrap()
        .ready()
        .unwrap()
}

#[wasm_bindgen_test(unsupported = test)]
fn the_extension_key_is_in_the_library_namespace() {
    assert_eq!(EXPRESSION_EXTENSION, "dxn.dtcg.expressions");
}

#[wasm_bindgen_test(unsupported = test)]
fn expression_tokens_resolve_their_snapshot_and_keep_the_extension() {
    let document = document(true);
    let tokens = resolve(&document, RuntimeOptions::default());

    let large = tokens.get("large").unwrap();
    assert_eq!(large.value, json!(16));
    assert_eq!(large.authored_value, json!(16));
    assert_eq!(large.snapshot, None);
    assert_eq!(large.extensions, document["large"]["$extensions"]);
    assert_eq!(
        tokens.get("accent").unwrap().extensions,
        document["accent"]["$extensions"]
    );
    assert!(tokens.iter().all(|token| token.snapshot.is_none()));
}

#[wasm_bindgen_test(unsupported = test)]
fn malformed_expressions_and_module_declarations_are_not_errors() {
    // `accent` carries an invalid expression object and the root an invalid
    // `modules` declaration; both stay opaque.
    let tokens = resolve(&document(true), RuntimeOptions::default());
    assert_eq!(tokens.len(), 3);
    assert_eq!(
        tokens.group("").unwrap().extensions,
        json!({ EXPRESSION_EXTENSION: { "modules": 7 } })
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn fingerprints_equal_those_of_the_documents_without_the_extension() {
    let with = resolve(&document(true), RuntimeOptions::default());
    let without = resolve(&document(false), RuntimeOptions::default());
    assert_eq!(with.fingerprint(), without.fingerprint());
    for token in without.iter() {
        assert_eq!(with.get(&token.path).unwrap().value, token.value);
    }
}

#[cfg(feature = "expressions")]
#[wasm_bindgen_test(unsupported = test)]
fn the_compiled_feature_evaluates_nothing_until_options_enable_it() {
    let options = RuntimeOptions::default();
    assert_eq!(options.expressions, None);
    let tokens = resolve(&document(true), options);
    assert_eq!(tokens.get("large").unwrap().value, json!(16));

    // Enabled, the same document rejects its invalid module declaration
    // and, without it, evaluates `large` and rejects `accent`.
    let enabled = RuntimeOptions::default()
        .with_expressions(dxn_dtcg::expressions::ExpressionOptions::default());
    let error = DtcgRuntime::from_json_with_options(
        "tokens.json",
        document(true).to_string(),
        enabled.clone(),
    )
    .unwrap()
    .resolve(&[])
    .unwrap_err();
    assert!(matches!(error, dxn_dtcg::DtcgError::Expression { ref path, .. } if path.is_empty()));
    let mut without_declaration = document(true);
    without_declaration
        .as_object_mut()
        .unwrap()
        .remove("$extensions");
    let error = DtcgRuntime::from_json_with_options(
        "tokens.json",
        without_declaration.to_string(),
        enabled,
    )
    .unwrap()
    .resolve(&[])
    .unwrap_err();
    assert!(matches!(error, dxn_dtcg::DtcgError::Expression { ref path, .. } if path == "accent"));
}
