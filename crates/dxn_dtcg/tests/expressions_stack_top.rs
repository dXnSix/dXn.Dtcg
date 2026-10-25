//! The stack limit measures each evaluation from where it starts, however
//! deep in resolution that is and whichever token started the session, and
//! a stack overflow is never catchable.
#![cfg(feature = "expressions")]

use dxn_dtcg::{
    DtcgError, DtcgRuntime, EXPRESSION_EXTENSION, RuntimeOptions, expressions::ExpressionOptions,
};
use serde_json::{Map, Value, json};
use wasm_bindgen_test::wasm_bindgen_test;

fn expression(js: &str, inputs: Value) -> Value {
    json!({ "$type": "number", "$value": 0, "$extensions": { EXPRESSION_EXTENSION: { "inputs": inputs, "js": js } } })
}

/// Recursion that catches its own stack overflow and returns its depth.
const CAUGHT_RECURSION: &str = "(() => { let depth = 0; const f = () => { depth++; f(); }; try { f(); } catch (e) {} return depth; })()";

/// Resolves `deep`, reached through `hops` aliases, optionally after an
/// earlier expression token that starts the session.
fn through_aliases(js: &str, hops: usize, early: bool) -> Result<Value, DtcgError> {
    let mut tokens = Map::new();
    if early {
        tokens.insert("early".into(), expression("1", json!({})));
    }
    for hop in 0..hops {
        let next = if hop + 1 == hops {
            "deep".to_owned()
        } else {
            format!("c{}", hop + 1)
        };
        tokens.insert(
            format!("c{hop}"),
            json!({ "$type": "number", "$value": format!("{{{next}}}") }),
        );
    }
    tokens.insert("deep".into(), expression(js, json!({})));
    resolve(&Value::Object(tokens), "deep")
}

fn resolve(document: &Value, path: &str) -> Result<Value, DtcgError> {
    DtcgRuntime::from_json_with_options(
        "tokens.json",
        document.to_string(),
        RuntimeOptions::default().with_expressions(ExpressionOptions::default()),
    )
    .unwrap()
    .resolve(&[])
    .map(|resolution| resolution.ready().unwrap().get(path).unwrap().value.clone())
}

fn assert_stack_overflow(result: Result<Value, DtcgError>, case: &str) {
    match result {
        Err(DtcgError::Expression { message, .. }) if message.contains("stack overflow") => {}
        other => panic!("{case}: expected an uncatchable stack overflow, got {other:?}"),
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn ordinary_code_runs_however_deep_the_evaluation_starts() {
    for early in [false, true] {
        for hops in [0, 8, 32, 63] {
            assert_eq!(
                through_aliases(
                    "[1, 2, 3].map((x) => x * 2).reduce((a, b) => a + b)",
                    hops,
                    early
                )
                .unwrap(),
                json!(12),
                "early={early} hops={hops}"
            );
        }
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn caught_recursion_fails_however_deep_the_evaluation_starts() {
    for early in [false, true] {
        for hops in [0, 8, 32, 63] {
            assert_stack_overflow(
                through_aliases(CAUGHT_RECURSION, hops, early),
                &format!("early={early} hops={hops}"),
            );
        }
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn evaluations_nested_in_input_resolution_get_the_whole_stack() {
    // Each token's input is the previous expression token, so every
    // evaluation starts while the next one resolves its inputs.
    let mut tokens = Map::new();
    tokens.insert("e0".into(), expression("1", json!({})));
    for index in 1..40 {
        tokens.insert(
            format!("e{index}"),
            expression(
                "previous + 1",
                json!({ "previous": format!("{{e{}}}", index - 1) }),
            ),
        );
    }
    let document = Value::Object(tokens.clone());
    assert_eq!(resolve(&document, "e39").unwrap(), json!(40));
    tokens.insert("e0".into(), expression(CAUGHT_RECURSION, json!({})));
    assert_stack_overflow(resolve(&Value::Object(tokens), "e39"), "nested inputs");
}

#[wasm_bindgen_test(unsupported = test)]
fn engine_recursion_through_nested_arrays_is_not_catchable() {
    for conversion in ["String(a)", "a.join()", "`${a}`"] {
        let js = format!(
            "(() => {{ let a = [1]; for (let i = 0; i < 100000; i++) a = [a]; try {{ {conversion}; }} catch (e) {{}} return 1; }})()"
        );
        for early in [false, true] {
            assert_stack_overflow(
                through_aliases(&js, 8, early),
                &format!("{conversion} early={early}"),
            );
        }
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn no_catch_or_finally_form_handles_a_stack_overflow() {
    let recursion = "const f = () => f() + 1;";
    for body in [
        "try { return f(); } catch (e) { return 1; }",
        "try { return f(); } catch { return 1; }",
        "try { return f(); } catch ({ message }) { return message.length; }",
        "try { return f(); } finally { return 1; }",
        "try { return f(); } catch (e) { return f(); } finally { return 1; }",
        "for (;;) { try { return f(); } catch (e) { break; } } return 1;",
        "try { try { return f(); } finally { /* finally */ } } catch (e) { return 1; }",
    ] {
        let js = format!("(() => {{ {recursion} {body} }})()");
        for early in [false, true] {
            assert_stack_overflow(
                through_aliases(&js, 8, early),
                &format!("{body} early={early}"),
            );
        }
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn a_module_cannot_handle_a_stack_overflow_either() {
    let module = dxn_dtcg::expressions::ExpressionModule::javascript(
        "const f = () => f() + 1;\nlet depth = 0;\ntry { f(); } catch (e) { depth = 1; }\nexport const caught = depth;",
    );
    let document = json!({
        "early": expression("1", json!({})),
        "deep": { "$type": "number", "$value": 0, "$extensions": { EXPRESSION_EXTENSION: {
            "imports": { "lib/deep": ["caught"] }, "js": "caught"
        } } },
    });
    let result = DtcgRuntime::from_json_with_options(
        "tokens.json",
        document.to_string(),
        RuntimeOptions::default()
            .with_expressions(ExpressionOptions::default().with_module("lib/deep", module)),
    )
    .unwrap()
    .resolve(&[])
    .map(|resolution| {
        resolution
            .ready()
            .unwrap()
            .get("deep")
            .unwrap()
            .value
            .clone()
    });
    assert_stack_overflow(result, "module top level");
}

#[wasm_bindgen_test(unsupported = test)]
fn errors_keep_their_authored_positions_around_try_statements() {
    let column = |js: &str| match through_aliases(js, 0, false) {
        Err(DtcgError::Expression { message, .. }) => message
            .rsplit_once("at line 1, column ")
            .and_then(|(_, column)| column.parse::<u32>().ok())
            .unwrap_or_else(|| panic!("{js}: {message}")),
        other => panic!("{js}: {other:?}"),
    };
    // The columns QuickJS reports for the same code without the check:
    // it records positions only at some operations, so the column of an
    // error after a `try` can be one inside it.
    assert_eq!(
        column("(() => { try { return 1; } catch (e) { return 2; } finally { null.a; } })()"),
        62
    );
    assert_eq!(
        column("(() => { let x = 0; try { x = 1; } catch { x = 2; } return null.c; })()"),
        44
    );
    let inside = column("(() => { try { null.a; } catch (e) { return null.b; } })()");
    assert!((16..=45).contains(&inside), "{inside}");
}

#[wasm_bindgen_test(unsupported = test)]
fn deeply_nested_patterns_fail_the_same_way_however_deep_the_evaluation_starts() {
    // A pattern built at run time nests no deeper than a literal may, so
    // the regular expression compiler never runs out of stack.
    for compile in [
        "new RegExp('(?:'.repeat(5000) + ')'.repeat(5000))",
        "'a'.match('(?:'.repeat(5000) + 'a' + ')'.repeat(5000))",
    ] {
        let js = format!(
            "(() => {{ try {{ {compile}; return 1; }} catch (e) {{ return e.message.includes('nests too deeply') ? 2 : 3; }} }})()"
        );
        for early in [false, true] {
            for hops in [0, 8, 63] {
                assert_eq!(
                    through_aliases(&js, hops, early).unwrap(),
                    json!(2),
                    "{compile} early={early} hops={hops}"
                );
            }
        }
    }
    // A pattern that fits compiles as usual.
    assert_eq!(
        through_aliases(
            "new RegExp('(?:'.repeat(20) + 'a' + ')'.repeat(20)).test('a') ? 1 : 0",
            0,
            false
        )
        .unwrap(),
        json!(1)
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn columns_count_characters_after_non_ascii_text() {
    let js = "(() => { const s = 'éééééé'; null.a; })()";
    let expected = js[..js.find("null").unwrap()].chars().count() + 1;
    match through_aliases(js, 0, false) {
        Err(DtcgError::Expression { message, .. }) => assert!(
            message.ends_with(&format!("at line 1, column {expected}")),
            "{message}"
        ),
        other => panic!("{other:?}"),
    }
}
