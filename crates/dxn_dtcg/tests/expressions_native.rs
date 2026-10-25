//! Hosts register modules of Rust functions and constants that expressions
//! and modules import; failures become JavaScript errors.
#![cfg(feature = "expressions")]

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use dxn_dtcg::{
    DtcgError, DtcgRuntime, EXPRESSION_EXTENSION, RuntimeOptions,
    expressions::{ExpressionModule, ExpressionOptions, Json, NativeModule},
    values::Dimension,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Deserialize, Serialize)]
struct Pair {
    a: f64,
    b: f64,
}

fn brand() -> NativeModule {
    NativeModule::new("brand/native")
        .function("double", |value: f64| value * 2.0)
        .function("greet", |name: String, excited: Option<bool>| {
            format!(
                "hello {name}{}",
                if excited == Some(true) { "!" } else { "" }
            )
        })
        .function("divide", |a: f64, b: f64| {
            if b == 0.0 {
                Err("division by zero")
            } else {
                Ok(a / b)
            }
        })
        .function("swap", |pair: Pair| {
            Ok::<_, std::convert::Infallible>(Pair {
                a: pair.b,
                b: pair.a,
            })
        })
        .function("swapJson", |pair: Pair| {
            Json(Pair {
                a: pair.b,
                b: pair.a,
            })
        })
        .function("scale", |dimension: Dimension, factor: f64| Dimension {
            value: dimension.value * factor,
            ..dimension
        })
        .function("explode", |_: f64| -> f64 { panic!("boom") })
        .function("none", || ())
        .constant("golden", 1.618)
        .constant("palette", json!({ "steps": [1, 2, 3] }))
}

fn options(native: NativeModule) -> RuntimeOptions {
    RuntimeOptions::default()
        .with_expressions(ExpressionOptions::default().with_native_module(native))
}

fn evaluate(
    token_type: &str,
    snapshot: Value,
    expression: Value,
    options: RuntimeOptions,
) -> Result<Value, DtcgError> {
    let document = json!({
        "computed": { "$type": token_type, "$value": snapshot, "$extensions": { EXPRESSION_EXTENSION: expression } }
    });
    DtcgRuntime::from_json_with_options("tokens.json", document.to_string(), options)
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

fn number(js: &str) -> Result<Value, DtcgError> {
    evaluate(
        "number",
        json!(0),
        json!({ "imports": { "brand/native": "native" }, "js": js }),
        options(brand()),
    )
}

fn text(js: &str) -> Value {
    evaluate(
        "fontFamily",
        json!("x"),
        json!({ "imports": { "brand/native": "native" }, "js": js }),
        options(brand()),
    )
    .unwrap()
}

#[test]
fn functions_take_and_return_serde_types() {
    assert_eq!(number("native.double(21)").unwrap(), json!(42));
    assert_eq!(text("native.greet('dtcg')"), json!("hello dtcg"));
    assert_eq!(text("native.greet('dtcg', true)"), json!("hello dtcg!"));
    assert_eq!(number("native.divide(9, 3)").unwrap(), json!(3));
    assert_eq!(number("native.swap({ a: 1, b: 2 }).a").unwrap(), json!(2));
    assert_eq!(
        evaluate(
            "dimension",
            json!({ "value": 0, "unit": "px" }),
            json!({ "imports": { "brand/native": ["scale"] }, "js": "scale({ value: 4, unit: 'rem' }, 1.5)" }),
            options(brand())
        )
        .unwrap(),
        json!({ "value": 6, "unit": "rem" })
    );
    assert_eq!(text("String(native.none())"), json!("null"));
    assert_eq!(number("native.golden * 1000").unwrap(), json!(1618));
    assert_eq!(number("native.palette.steps.length").unwrap(), json!(3));
}

#[test]
fn errors_and_panics_become_catchable_javascript_errors() {
    for (js, kind, message) in [
        (
            "native.divide(1, 0)",
            "Error",
            "brand/native.divide: division by zero",
        ),
        (
            "native.explode(1)",
            "Error",
            "brand/native.explode: panicked: boom",
        ),
        (
            "native.double('x')",
            "TypeError",
            "brand/native.double: argument 1",
        ),
        (
            "native.double()",
            "TypeError",
            "brand/native.double: argument 1",
        ),
        (
            "native.scale({ value: 1, unit: 'em' }, 2)",
            "TypeError",
            "brand/native.scale: argument 1",
        ),
    ] {
        let caught = text(&format!(
            "(() => {{ try {{ {js}; return 'no error'; }} catch (e) {{ return e.constructor.name + '|' + e.message; }} }})()"
        ));
        let caught = caught.as_str().unwrap().to_owned();
        assert!(
            caught.starts_with(&format!("{kind}|{message}")),
            "{js}: {caught}"
        );
        match number(js) {
            Err(DtcgError::Expression {
                path,
                message: error,
                ..
            }) => {
                assert_eq!(path, "computed");
                assert!(error.contains(message), "{js}: {error}");
            }
            other => panic!("{js}: {other:?}"),
        }
    }
}

#[test]
fn a_panic_leaves_the_runtime_usable() {
    let runtime = DtcgRuntime::from_json_with_options(
        "tokens.json",
        json!({ "computed": { "$type": "number", "$value": 0, "$extensions": { EXPRESSION_EXTENSION: { "imports": { "brand/native": ["explode", "double"] }, "js": "explode(1)" } } } }).to_string(),
        options(brand()),
    )
    .unwrap();
    assert!(runtime.resolve(&[]).is_err());
    assert!(runtime.resolve(&[]).is_err());
    assert_eq!(number("native.double(2)").unwrap(), json!(4));
}

#[test]
fn exports_are_frozen() {
    assert!(
        text(
            "(() => { try { native.double.leak = 1; return 'wrote'; } catch (e) { return e.constructor.name; } })()"
        ) == json!("TypeError")
    );
    assert!(
        text(
            "(() => { try { native.palette.steps.push(4); return 'wrote'; } catch (e) { return e.constructor.name; } })()"
        ) == json!("TypeError")
    );
    assert_eq!(text("typeof native.double.prototype"), json!("undefined"));
}

#[test]
fn the_bridge_is_internal() {
    match number("1") {
        Ok(_) => {}
        other => panic!("{other:?}"),
    }
    let error = evaluate(
        "number",
        json!(0),
        json!({ "imports": { "dxn.dtcg/native": ["call"] }, "js": "1" }),
        options(brand()),
    )
    .unwrap_err();
    assert!(
        matches!(error, DtcgError::Expression { ref message, .. } if message.contains("unknown module `dxn.dtcg/native`")),
        "{error:?}"
    );
}

#[test]
fn native_registrations_need_free_bare_specifiers() {
    for specifier in ["dxn.dtcg", "dxn.dtcg/color", "dxn.dtcg/math", "", "./brand"] {
        let error = evaluate(
            "number",
            json!(0),
            json!({ "js": "1" }),
            options(NativeModule::new(specifier)),
        )
        .unwrap_err();
        assert!(
            matches!(error, DtcgError::Expression { .. }),
            "{specifier}: {error:?}"
        );
    }
    let both = RuntimeOptions::default().with_expressions(
        ExpressionOptions::default()
            .with_native_module(NativeModule::new("brand"))
            .with_module("brand", ExpressionModule::javascript("export {}")),
    );
    let error = evaluate("number", json!(0), json!({ "js": "1" }), both).unwrap_err();
    assert!(
        matches!(error, DtcgError::Expression { ref message, .. } if message.contains("both")),
        "{error:?}"
    );
}

#[test]
fn options_stay_comparable_and_printable() {
    let module = brand();
    let first = ExpressionOptions::default().with_native_module(module.clone());
    let second = ExpressionOptions::default().with_native_module(module);
    assert_eq!(first, second);
    assert_ne!(
        first,
        ExpressionOptions::default().with_native_module(brand())
    );
    let options = RuntimeOptions::default().with_expressions(first);
    assert_eq!(options.clone(), options);
    let printed = format!("{options:?}");
    assert!(
        printed.contains("brand/native") && printed.contains("double"),
        "{printed}"
    );
    fn send_sync<T: Send + Sync>() {}
    send_sync::<DtcgRuntime>();
    send_sync::<NativeModule>();
}

#[test]
fn each_module_evaluates_once_per_resolution() {
    let evaluations = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&evaluations);
    let native = NativeModule::new("probe/count")
        .function("count", move || counter.fetch_add(1, Ordering::SeqCst) + 1);
    let options = RuntimeOptions::default().with_expressions(
        ExpressionOptions::default()
            .with_native_module(native)
            .with_module(
                "lib/tracked",
                ExpressionModule::javascript(
                    "import { count } from 'probe/count';\nexport const loads = count();",
                ),
            ),
    );
    let tokens = (0..5)
        .map(|index| {
            (
                format!("token{index}"),
                json!({ "$type": "number", "$value": 0, "$extensions": { EXPRESSION_EXTENSION: {
                    "imports": { "lib/tracked": ["loads"] },
                    "js": format!("loads * 10 + {index}")
                } } }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let runtime = DtcgRuntime::from_json_with_options(
        "tokens.json",
        Value::Object(tokens).to_string(),
        options,
    )
    .unwrap();
    let first = runtime.resolve(&[]).unwrap().ready().unwrap();
    assert_eq!(evaluations.load(Ordering::SeqCst), 1);
    assert_eq!(first.get("token4").unwrap().value, json!(14));
    // The next resolution starts a new session: the module loads again.
    let second = runtime.resolve(&[]).unwrap().ready().unwrap();
    assert_eq!(evaluations.load(Ordering::SeqCst), 2);
    assert_eq!(second.get("token0").unwrap().value, json!(20));
}

#[test]
fn json_wraps_any_serializable_result() {
    assert_eq!(
        text("JSON.stringify(native.swapJson({ a: 1, b: 2 }))"),
        json!("{\"a\":2,\"b\":1}")
    );
}

#[test]
fn export_names_must_be_javascript_identifiers() {
    for name in ["new", "default", "delete", "1st", "two words", "", "a-b"] {
        let problem =
            std::panic::catch_unwind(|| NativeModule::new("brand").function(name, || 1.0))
                .err()
                .unwrap_or_else(|| panic!("accepted `{name}`"));
        let message = problem.downcast_ref::<String>().unwrap();
        assert!(
            message.starts_with(&format!("native module `brand`: export name `{name}` is ")),
            "{message}"
        );
        assert!(
            std::panic::catch_unwind(|| NativeModule::new("brand").constant(name, 1)).is_err(),
            "{name}"
        );
    }
    NativeModule::new("brand")
        .function("type", || 1.0)
        .function("$scale", || 1.0)
        .constant("_private", 1);
}
