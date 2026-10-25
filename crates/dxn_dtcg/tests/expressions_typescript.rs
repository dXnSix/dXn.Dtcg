//! TypeScript expressions have their types removed, unchecked, and errors
//! of every expression report their position in the authored text.
#![cfg(feature = "expressions")]

use dxn_dtcg::{
    DtcgDiagnostic, DtcgDiagnosticCode, DtcgError, DtcgRuntime, EXPRESSION_EXTENSION,
    RuntimeOptions, expressions::ExpressionOptions,
};
use serde_json::{Value, json};
use wasm_bindgen_test::wasm_bindgen_test;

fn evaluate(language: &str, source: &str) -> Result<Value, DtcgError> {
    let mut expression = serde_json::Map::new();
    expression.insert("inputs".into(), json!({ "base": "{base}" }));
    expression.insert(language.into(), json!(source));
    let document = json!({
        "base": { "$type": "dimension", "$value": { "value": 4, "unit": "px" } },
        "computed": {
            "$type": "dimension",
            "$value": { "value": 0, "unit": "px" },
            "$extensions": { EXPRESSION_EXTENSION: expression }
        }
    });
    DtcgRuntime::from_json_with_options(
        "tokens.json",
        document.to_string(),
        RuntimeOptions::default().with_expressions(ExpressionOptions::default()),
    )
    .unwrap()
    .resolve(&[])
    .map(|resolution| {
        resolution
            .ready()
            .unwrap()
            .get("computed")
            .unwrap()
            .value
            .clone()
    })
}

fn message(language: &str, source: &str) -> String {
    match evaluate(language, source) {
        Err(DtcgError::Expression {
            resource,
            path,
            message,
        }) => {
            assert_eq!(resource.as_str(), "tokens.json");
            assert_eq!(path, "computed");
            message
        }
        other => panic!("{source}: expected an expression error, got {other:?}"),
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn type_annotations_assertions_and_satisfies_are_removed() {
    assert_eq!(
        evaluate(
            "ts",
            "({ value: (base.value as number) * 2, unit: base.unit } satisfies { value: number; unit: string })"
        )
        .unwrap(),
        json!({ "value": 8, "unit": "px" })
    );
    assert_eq!(
        evaluate("ts", "((d: { value: number; unit: string }, k: number): typeof d => ({ ...d, value: d.value * k }))(base, 3)")
            .unwrap(),
        json!({ "value": 12, "unit": "px" })
    );
    assert_eq!(
        evaluate("ts", "<{ value: number; unit: string }>base").unwrap(),
        json!({ "value": 4, "unit": "px" })
    );
    assert_eq!(
        evaluate("ts", "base!").unwrap(),
        json!({ "value": 4, "unit": "px" })
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn generics_interfaces_and_type_aliases_are_removed() {
    let source = "(() => {
  interface Dimension { value: number; unit: 'px' | 'rem' }
  type Scale = (d: Dimension, k: number) => Dimension;
  const scale: Scale = <T extends Dimension>(d: T, k: number): T => ({ ...d, value: d.value * k });
  enum Unit { Px = 'px' }
  return scale(base as Dimension, 1.5);
})()";
    // `enum` is not only a type; it is compiled like any other TypeScript.
    assert_eq!(
        evaluate("ts", source).unwrap(),
        json!({ "value": 6, "unit": "px" })
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn types_are_not_checked() {
    assert_eq!(
        evaluate(
            "ts",
            "({ value: ('4' as unknown as number) * 1, unit: base.unit as unknown as number })"
        )
        .unwrap(),
        json!({ "value": 4, "unit": "px" })
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn syntax_errors_name_their_authored_line_and_column() {
    let javascript = message("js", "({ value: base.value *, unit: 'px' })");
    assert!(javascript.contains("SyntaxError"), "{javascript}");
    assert!(javascript.contains("at line 1, column 23"), "{javascript}");

    let typescript = message("ts", "({\n  value: base.value as,\n  unit: 'px' })");
    assert!(typescript.contains("SyntaxError"), "{typescript}");
    assert!(typescript.contains("at line 2, column 23"), "{typescript}");

    // An unterminated expression fails at its end.
    let unterminated = message("js", "base.value +");
    assert!(
        unterminated.contains("at line 1, column 13"),
        "{unterminated}"
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn uncaught_exceptions_name_their_authored_line_and_column() {
    let javascript = message("js", "({\n  value: base.value,\n  unit: missing.unit })");
    assert!(
        javascript.contains("ReferenceError: missing is not defined"),
        "{javascript}"
    );
    assert!(javascript.contains("at line 3, column 9"), "{javascript}");

    let thrown = message("js", "(() => {\n  throw new RangeError('too large');\n})()");
    assert!(thrown.contains("RangeError: too large"), "{thrown}");
    // QuickJS records where the error object is created.
    assert!(thrown.contains("at line 2, column 13"), "{thrown}");

    // TypeScript positions map back through the removed types.
    let typescript = message(
        "ts",
        "((d: { value: number; unit: string }): number => {\n  const scale: number = 2;\n  return (d as any).value.nope.deeper * scale;\n})(base)",
    );
    assert!(typescript.contains("TypeError"), "{typescript}");
    assert!(typescript.contains("at line 3, column "), "{typescript}");
    let column: u32 = typescript
        .rsplit("column ")
        .next()
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!((10..=36).contains(&column), "{typescript}");
}

#[wasm_bindgen_test(unsupported = test)]
fn expression_errors_map_to_the_expression_diagnostic_code() {
    let error = evaluate("ts", "base.value as").unwrap_err();
    let diagnostic = DtcgDiagnostic::from_error(&error);
    assert_eq!(diagnostic.code(), DtcgDiagnosticCode::Expression);
    assert_eq!(
        diagnostic.resource().map(|id| id.as_str()),
        Some("tokens.json")
    );
    assert_eq!(diagnostic.path(), Some("computed"));
    assert!(diagnostic.message().contains("SyntaxError"));
}

#[wasm_bindgen_test(unsupported = test)]
fn early_errors_name_their_authored_line_and_column() {
    for (language, source, line) in [
        (
            "js",
            "(() => {\n  let a = 1;\n  let a = 2;\n  return base;\n})()",
            3,
        ),
        (
            "ts",
            "(() => {\n  const b: number = 1;\n  let b = 2;\n  return base;\n})()",
            3,
        ),
        (
            "js",
            "(() => {\n  return { value: 010, unit: 'px' };\n})()",
            2,
        ),
        (
            "js",
            "(() => {\n  /(?<a>x)(?<a>y)/;\n  return base;\n})()",
            2,
        ),
    ] {
        let message = message(language, source);
        assert!(message.contains("SyntaxError"), "{source}: {message}");
        assert!(
            message.contains(&format!("at line {line}, column ")),
            "{source}: {message}"
        );
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn diagnostics_keep_the_error_kind_and_position_but_no_message_text() {
    for (source, expected) in [
        (
            "(() => { throw { secret: 'document text' }; })()",
            "uncaught exception",
        ),
        (
            "(() => {\n  throw new TypeError('secret document text');\n})()",
            "TypeError at line 2, column 13",
        ),
        (
            "({ value: secret_identifier, unit: 'px' })",
            "ReferenceError at line 1, column 11",
        ),
        ("'secret' + ", "SyntaxError at line 1, column 12"),
    ] {
        let error = evaluate("js", source).unwrap_err();
        let diagnostic = DtcgDiagnostic::from_error(&error);
        assert_eq!(diagnostic.code(), DtcgDiagnosticCode::Expression);
        assert_eq!(diagnostic.message(), expected, "{error}");
        assert!(!diagnostic.message().contains("secret"));
        // The error itself keeps the full message for the caller.
        assert!(
            error
                .to_string()
                .contains(&expected[..expected.find(' ').unwrap_or(expected.len())])
        );
    }
}
