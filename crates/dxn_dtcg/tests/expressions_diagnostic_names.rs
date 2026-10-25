//! Redacted expression diagnostics keep only the standard JavaScript error
//! names: expression code sets an error's name, which could carry
//! document text.
#![cfg(feature = "expressions")]

use dxn_dtcg::{
    DtcgDiagnostic, DtcgRuntime, EXPRESSION_EXTENSION, RuntimeOptions,
    expressions::ExpressionOptions,
};
use serde_json::json;
use wasm_bindgen_test::wasm_bindgen_test;

fn diagnostic(js: &str) -> String {
    let document = json!({
        "secret": { "$type": "fontFamily", "$value": "Hunter2" },
        "computed": { "$type": "number", "$value": 0, "$extensions": { EXPRESSION_EXTENSION: {
            "inputs": { "secret": "{secret}" }, "js": js
        } } }
    });
    let error = DtcgRuntime::from_json_with_options(
        "tokens.json",
        document.to_string(),
        RuntimeOptions::default().with_expressions(ExpressionOptions::default()),
    )
    .unwrap()
    .resolve(&[])
    .unwrap_err();
    DtcgDiagnostic::from_error(&error).message().to_owned()
}

#[wasm_bindgen_test(unsupported = test)]
fn a_custom_error_name_is_reported_as_error() {
    let message =
        diagnostic("(() => { const e = new Error('x'); e.name = secret + 'Error'; throw e; })()");
    assert!(message.starts_with("Error at line 1, column "), "{message}");
    assert!(!message.contains("Hunter2"), "{message}");
    let message = diagnostic(
        "(() => { class Hunter2Error extends Error {} throw new Hunter2Error('x'); })()",
    );
    assert!(!message.contains("Hunter2"), "{message}");
}

#[wasm_bindgen_test(unsupported = test)]
fn standard_error_names_are_kept() {
    for (js, name) in [
        ("null.a", "TypeError"),
        ("missing", "ReferenceError"),
        ("(() => { throw new RangeError(secret); })()", "RangeError"),
        ("(() => { throw new URIError(secret); })()", "URIError"),
        ("(() => { throw new EvalError(secret); })()", "EvalError"),
        ("(() => { throw new Error(secret); })()", "Error"),
    ] {
        let message = diagnostic(js);
        assert!(
            message == name || message.starts_with(&format!("{name} at line ")),
            "{js}: {message}"
        );
        assert!(!message.contains("Hunter2"), "{js}: {message}");
    }
}
