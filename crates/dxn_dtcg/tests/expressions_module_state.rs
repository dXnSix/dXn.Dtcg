//! A module's values are frozen once its top-level code ran: tokens share
//! a module within a resolution, so none can change what another sees, and
//! affected resolution equals a full one.
#![cfg(feature = "expressions")]

use dxn_dtcg::{
    DocumentKind, DtcgError, DtcgRuntime, EXPRESSION_EXTENSION, Resolution, RuntimeOptions,
    expressions::{ExpressionModule, ExpressionOptions},
};
use serde_json::{Value, json};

fn expression(js: &str, imports: Value, inputs: Value) -> Value {
    json!({ "$type": "number", "$value": 0, "$extensions": { EXPRESSION_EXTENSION: {
        "imports": imports, "inputs": inputs, "js": js
    } } })
}

fn options(module: ExpressionModule) -> RuntimeOptions {
    RuntimeOptions::default()
        .with_expressions(ExpressionOptions::default().with_module("lib/shared", module))
}

fn resolve(document: &Value, module: ExpressionModule) -> Result<Vec<Value>, DtcgError> {
    let tokens =
        DtcgRuntime::from_json_with_options("tokens.json", document.to_string(), options(module))
            .unwrap()
            .resolve(&[])?
            .ready()
            .unwrap();
    Ok(["a", "b"]
        .iter()
        .map(|path| tokens.get(path).unwrap().value.clone())
        .collect())
}

/// What `js` sees after trying to change the module's state with `attempt`,
/// for two tokens in one resolution.
fn after_attempt(module: &str, imports: Value, attempt: &str, read: &str) -> Vec<Value> {
    let js = format!("(() => {{ try {{ {attempt}; }} catch (e) {{}} return {read}; }})()");
    let document = json!({
        "a": expression(&js, imports.clone(), json!({})),
        "b": expression(&js, imports, json!({})),
    });
    resolve(&document, ExpressionModule::javascript(module)).unwrap()
}

#[test]
fn exported_values_cannot_change() {
    let imports = json!({ "lib/shared": ["box", "list", "Point", "scale"] });
    let module = "export const box = { n: 0, inner: { n: 0 } };\n\
        export const list = [1];\n\
        export class Point { static count = 0; norm() { return 1; } }\n\
        export function scale(x) { return x * 2; }";
    for (attempt, read) in [
        ("box.n += 1", "box.n"),
        ("box.inner.n += 1", "box.inner.n"),
        ("box.extra = 1", "box.extra ?? 0"),
        ("list.push(2)", "list.length"),
        ("Point.count += 1", "Point.count"),
        ("Point.prototype.norm = () => 5", "new Point().norm()"),
        ("scale.factor = 3", "scale.factor ?? 0"),
    ] {
        let seen = after_attempt(module, imports.clone(), attempt, read);
        assert_eq!(seen[0], seen[1], "{attempt}");
        assert!(
            seen[0] == json!(0) || seen[0] == json!(1),
            "{attempt}: {seen:?}"
        );
    }
}

#[test]
fn values_a_module_keeps_to_itself_cannot_change() {
    let module = "const cache = { hits: 0 };\n\
        export default { n: 0 };\n\
        export function hit() { cache.hits += 1; return cache.hits; }\n\
        export function hits() { return cache.hits; }";
    let seen = after_attempt(
        module,
        json!({ "lib/shared": ["hit", "hits"] }),
        "hit()",
        "hits()",
    );
    assert_eq!(seen, [json!(0), json!(0)]);
    let seen = after_attempt(
        module,
        json!({ "lib/shared": "lib" }),
        "lib.default.n += 1",
        "lib.default.n",
    );
    assert_eq!(seen, [json!(0), json!(0)]);
}

#[test]
fn changing_a_frozen_value_throws_a_type_error() {
    let document = json!({
        "a": expression("(box.n += 1, box.n)", json!({ "lib/shared": ["box"] }), json!({})),
        "b": expression("1", json!({}), json!({})),
    });
    let error = resolve(
        &document,
        ExpressionModule::javascript("export const box = { n: 0 };"),
    )
    .unwrap_err();
    assert!(
        matches!(&error, DtcgError::Expression { path, message, .. } if path == "a" && message.starts_with("TypeError")),
        "{error:?}"
    );
}

#[test]
fn affected_resolution_equals_a_full_resolution() {
    let module = ExpressionModule::javascript("export const box = { n: 0 };");
    let js = "(() => { try { box.n += 1; } catch (e) {} return box.n + source; })()";
    let document = |source: u32| {
        json!({
            "src": { "$type": "number", "$value": source },
            "a": expression(js, json!({ "lib/shared": ["box"] }), json!({ "source": "{src}" })),
            "b": expression(js, json!({ "lib/shared": ["box"] }), json!({ "source": "{src}" })),
        })
        .to_string()
    };
    let mut runtime =
        DtcgRuntime::from_json_with_options("tokens.json", document(1), options(module)).unwrap();
    let mut tokens = runtime.resolve(&[]).unwrap().ready().unwrap();
    let Resolution::Ready(affected) = runtime
        .replace_json_and_resolve_affected("tokens.json", document(2), &[], &["src"])
        .unwrap()
    else {
        panic!("everything is loaded");
    };
    tokens.apply_affected(&affected);
    let full = runtime.resolve(&[]).unwrap().ready().unwrap();
    assert_eq!(tokens, full);
    assert_eq!(full.get("a").unwrap().value, json!(2));
    assert_eq!(full.get("b").unwrap().value, json!(2));
}

#[test]
fn modules_still_work_normally() {
    // Top-level code may build its values before they freeze, TypeScript
    // types leave no bindings behind, and names the freezing code would use
    // stay the module's own.
    let module = ExpressionModule::typescript(
        "interface Scale { steps: number[] }\n\
         type Step = number;\n\
         const __dxn_dtcg_self = 2;\n\
         const __dxn_dtcg_freeze = 3;\n\
         export const scale: Scale = { steps: [] };\n\
         for (let i = 1; i <= 4; i++) scale.steps.push(i * __dxn_dtcg_self);\n\
         export const sum = (): Step => scale.steps.reduce((a, b) => a + b, 0) + __dxn_dtcg_freeze;",
    );
    let document = json!({
        "a": expression("sum()", json!({ "lib/shared": ["sum"] }), json!({})),
        "b": expression("scale.steps.length", json!({ "lib/shared": ["scale"] }), json!({})),
    });
    assert_eq!(resolve(&document, module).unwrap(), [json!(23), json!(4)]);
}

#[test]
fn document_modules_are_frozen_too() {
    let document = json!({
        "$extensions": { EXPRESSION_EXTENSION: { "modules": { "brand": "brand.js" } } },
        "a": expression("(() => { try { box.n += 1; } catch (e) {} return box.n; })()", json!({ "brand": ["box"] }), json!({})),
        "b": expression("(() => { try { box.n += 1; } catch (e) {} return box.n; })()", json!({ "brand": ["box"] }), json!({})),
    });
    let mut runtime = DtcgRuntime::from_json_with_options(
        "tokens.json",
        document.to_string(),
        RuntimeOptions::default().with_expressions(ExpressionOptions::default()),
    )
    .unwrap();
    let Resolution::Missing(requests) = runtime.resolve(&[]).unwrap() else {
        panic!("the module is requested");
    };
    assert_eq!(requests[0].kind, DocumentKind::Module);
    runtime
        .provide_requested_module(&requests[0], "export const box = { n: 0 };")
        .unwrap();
    let tokens = runtime.resolve(&[]).unwrap().ready().unwrap();
    assert_eq!(tokens.get("a").unwrap().value, json!(0));
    assert_eq!(tokens.get("b").unwrap().value, json!(0));
}

#[test]
fn objects_others_inherit_from_still_let_inheritors_assign() {
    let module = ExpressionModule::javascript(
        "export class Counter {\n\
           constructor() { this.next = this.next.bind(this); this.step = 2; }\n\
           next() { return this.step; }\n\
         }\n\
         Counter.prototype.step = 1;\n\
         const base = { size: 1 };\n\
         export const sized = Object.create(base);\n\
         export function make() { const counter = new Counter(); counter.step = 3; return counter.next(); }",
    );
    let document = json!({
        "a": expression("make()", json!({ "lib/shared": ["make"] }), json!({})),
        "b": expression("(() => { const o = Object.create(Object.getPrototypeOf(sized)); o.size = 5; return o.size; })()",
            json!({ "lib/shared": ["sized"] }), json!({})),
    });
    assert_eq!(resolve(&document, module).unwrap(), [json!(3), json!(5)]);
}

#[test]
fn an_object_nothing_inherits_from_yet_stays_read_only_for_inheritors() {
    // Freezing cannot know that tokens will inherit from `defaults`: an
    // inherited property cannot be assigned, only defined.
    let module = ExpressionModule::javascript("export const defaults = { size: 1 };");
    let js = |assign: &str| {
        format!("(() => {{ const o = Object.create(defaults); {assign}; return o.size; }})()")
    };
    let document = json!({
        "a": expression(&js("try { o.size = 5; } catch (e) {}"), json!({ "lib/shared": ["defaults"] }), json!({})),
        "b": expression(&js("Object.defineProperty(o, 'size', { value: 5 })"), json!({ "lib/shared": ["defaults"] }), json!({})),
    });
    assert_eq!(resolve(&document, module).unwrap(), [json!(1), json!(5)]);
}

#[test]
fn the_internal_module_cannot_be_imported() {
    let document = json!({
        "a": expression("1", json!({ "dxn.dtcg/freeze": ["check"] }), json!({})),
        "b": expression("1", json!({}), json!({})),
    });
    let error = resolve(
        &document,
        ExpressionModule::javascript("export const x = 1;"),
    )
    .unwrap_err();
    assert!(
        matches!(&error, DtcgError::Expression { message, .. } if message.contains("unknown module `dxn.dtcg/freeze`")),
        "{error:?}"
    );
    let document = json!({
        "a": expression("x", json!({ "lib/shared": ["x"] }), json!({})),
        "b": expression("1", json!({}), json!({})),
    });
    let error = resolve(
        &document,
        ExpressionModule::javascript(
            "import { check } from 'dxn.dtcg/freeze';\nexport const x = 1;",
        ),
    )
    .unwrap_err();
    assert!(
        matches!(&error, DtcgError::Expression { message, .. } if message.contains("unknown module `dxn.dtcg/freeze`")),
        "{error:?}"
    );
}

#[test]
fn freezing_is_charged_to_the_module_that_is_frozen() {
    // About five steps per thousand objects a module's values reach.
    let table = |rows: u32| {
        ExpressionModule::javascript(format!(
            "export const table = [];\nfor (let i = 0; i < {rows}; i++) table.push({{ name: 'r' + i, value: i }});"
        ))
    };
    let document = json!({
        "a": expression("table.length", json!({ "lib/shared": ["table"] }), json!({})),
        "b": expression("1", json!({}), json!({})),
    });
    assert_eq!(
        resolve(&document, table(20_000)).unwrap(),
        [json!(20_000), json!(1)]
    );
    let tight = DtcgRuntime::from_json_with_options(
        "tokens.json",
        document.to_string(),
        RuntimeOptions::default().with_expressions(
            ExpressionOptions::default()
                .with_step_budget(50)
                .with_module("lib/shared", table(20_000)),
        ),
    )
    .unwrap()
    .resolve(&[])
    .unwrap_err();
    assert!(
        matches!(
            tight,
            DtcgError::LimitExceeded {
                kind: "expression steps",
                ..
            }
        ),
        "{tight:?}"
    );
}

#[test]
fn collections_a_module_holds_are_shared_within_a_resolution() {
    // The owner's decision: modules may hold and export collections, and
    // writes into them (from module functions or from expressions) are
    // shared by the tokens of one resolution, never across resolutions.
    let module = ExpressionModule::javascript(
        "export const seen = new Map();\n\
         export const bytes = new Uint8Array(1);\n\
         let calls = 0;\n\
         export function call() { calls += 1; return calls; }",
    );
    let document = json!({
        "a": expression("(seen.set('a', 1), bytes[0] += 1, call(), seen.size)", json!({ "lib/shared": ["seen", "bytes", "call"] }), json!({})),
        "b": expression("seen.size * 100 + bytes[0] * 10 + call()", json!({ "lib/shared": ["seen", "bytes", "call"] }), json!({})),
    });
    let runtime =
        DtcgRuntime::from_json_with_options("tokens.json", document.to_string(), options(module))
            .unwrap();
    for _ in 0..2 {
        let tokens = runtime.resolve(&[]).unwrap().ready().unwrap();
        assert_eq!(tokens.get("a").unwrap().value, json!(1));
        assert_eq!(tokens.get("b").unwrap().value, json!(112));
    }
}
