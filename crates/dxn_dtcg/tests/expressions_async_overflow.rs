//! A stack overflow stays uncatchable where no `catch` is involved: async
//! functions (whose errors become promises nothing awaits), parameter
//! defaults, iterators closed by a throw, and errors that wrap others
//! (`SuppressedError`, `AggregateError`). Patterns built at run time nest
//! no deeper than literals, so the regular expression compiler cannot run
//! out of stack.
#![cfg(feature = "expressions")]

use dxn_dtcg::{
    DtcgError, DtcgRuntime, EXPRESSION_EXTENSION, RuntimeOptions, expressions::ExpressionOptions,
};
use serde_json::{Value, json};
use wasm_bindgen_test::wasm_bindgen_test;

fn evaluate(js: &str) -> Result<Value, DtcgError> {
    let document = json!({
        "early": { "$type": "number", "$value": 0, "$extensions": { EXPRESSION_EXTENSION: { "js": "1" } } },
        "computed": { "$type": "number", "$value": 0, "$extensions": { EXPRESSION_EXTENSION: { "js": js } } },
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

fn assert_stack_overflow(js: &str) {
    match evaluate(js) {
        Err(DtcgError::Expression { message, .. }) if message.contains("stack overflow") => {}
        other => panic!("{js}: expected an uncatchable stack overflow, got {other:?}"),
    }
}

const RECURSION: &str = "const f = () => f() + 1;";

#[wasm_bindgen_test(unsupported = test)]
fn an_async_function_cannot_swallow_a_stack_overflow() {
    for body in [
        "(async () => f())(); return 7;",
        "(async () => { f(); })(); return 7;",
        "(async function () { f(); })(); return 7;",
        "({ async m() { f(); } }).m(); return 7;",
        "(async function* () { f(); })().next(); return 7;",
        "let x = 0; (async () => { f(); x = 1; })(); return x;",
    ] {
        assert_stack_overflow(&format!("(() => {{ {RECURSION} {body} }})()"));
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn a_wrapped_stack_overflow_is_still_one() {
    for body in [
        "const stack = new DisposableStack(); stack.defer(() => { throw new Error('later'); }); stack.defer(() => f()); try { stack.dispose(); } catch (e) { return 7; }",
        "try { throw new SuppressedError(new Error('x'), (() => { try { f(); } catch (e) { return e; } })()); } catch (e) { return 7; }",
        "try { throw new AggregateError([(() => { try { f(); } catch (e) { return e; } })()]); } catch (e) { return 7; }",
    ] {
        assert_stack_overflow(&format!("(() => {{ {RECURSION} {body} }})()"));
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn async_code_still_runs_up_to_its_first_await() {
    assert_eq!(
        evaluate("(() => { let x = 0; (async () => { x = 1; })(); return x; })()").unwrap(),
        json!(1)
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn asynchronous_built_ins_are_removed() {
    assert_eq!(
        evaluate("typeof Array.fromAsync === 'undefined' && typeof AsyncDisposableStack === 'undefined' ? 1 : 0").unwrap(),
        json!(1)
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn parameter_defaults_and_closed_iterators_cannot_swallow_one_either() {
    for body in [
        "(async (x = f()) => {})(); return 7;",
        "(async ({ x = f() }) => {})({}); return 7;",
        "({ async m(x = f()) {} }).m(); return 7;",
        "const it = { [Symbol.iterator]() { return { next: () => ({ value: 1, done: false }), return() { f(); } }; } }; try { for (const x of it) throw 1; } catch (e) { return 7; }",
        "const it = { [Symbol.iterator]() { return { next: () => ({ value: 1, done: false }), return() { f(); } }; } }; try { Array.from(it, () => { throw 1; }); } catch (e) { return 7; }",
    ] {
        assert_stack_overflow(&format!("(() => {{ {RECURSION} {body} }})()"));
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn an_uncaught_wrapped_stack_overflow_is_reported_as_one() {
    assert_stack_overflow(&format!(
        "(() => {{ {RECURSION} const stack = new DisposableStack(); stack.defer(() => {{ throw new Error('later'); }}); stack.defer(() => f()); stack.dispose(); return 7; }})()"
    ));
}

#[wasm_bindgen_test(unsupported = test)]
fn async_functions_keep_their_meaning() {
    for js in [
        "(() => { (async function () { var g = 1; function g() {} })(); return 1; })()",
        "(() => { (async function () { function h() {} function h() {} })(); return 1; })()",
        "(() => { let x = 0; (async (y = (x = 2)) => { x += y; })(); return x; })()",
    ] {
        assert!(evaluate(js).is_ok(), "{js}: {:?}", evaluate(js));
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn patterns_built_at_run_time_nest_no_deeper_than_literals() {
    let deep = "'(?:'.repeat(30) + 'a' + ')'.repeat(30)";
    for compile in [
        format!("new RegExp({deep})"),
        format!("RegExp({deep}, 'g')"),
        format!("'a'.match({deep})"),
        format!("[...'a'.matchAll({deep} + '|b')]"),
        format!("'a'.search({deep})"),
        format!("new RegExp({{ [Symbol.match]: true, source: {deep}, flags: '' }})"),
        format!("'a'.match({{ toString: () => {deep} }})"),
        format!("(async () => new RegExp({deep}))()"),
    ] {
        let js = format!(
            "(() => {{ try {{ {compile}; return 1; }} catch (e) {{ return e instanceof SyntaxError && e.message.includes('nests too deeply') ? 2 : 3; }} }})()"
        );
        let expected = if compile.starts_with("(async") { 1 } else { 2 };
        assert_eq!(evaluate(&js).unwrap(), json!(expected), "{compile}");
    }
    // Patterns that fit, and regular expressions themselves, work as usual.
    for js in [
        "new RegExp('(?:'.repeat(20) + 'a' + ')'.repeat(20)).test('a') ? 1 : 0",
        "'xay'.match('a').index",
        "[...'aXa'.matchAll(/a/g)].length - 1",
        "'xay'.search(/a/)",
        "new RegExp(/a/g, 'i').flags === 'i' && /a/ instanceof RegExp && new RegExp('a').constructor === RegExp ? 1 : 0",
        "(() => { class Sub extends RegExp {} return new Sub('a') instanceof Sub && new Sub('a').test('a') ? 1 : 0; })()",
        "typeof RegExp.prototype.compile === 'undefined' ? 1 : 0",
        "(() => { const re = /a/; return RegExp(re) === re && RegExp(re, 'g') !== re ? 1 : 0; })()",
        "new RegExp('[[]'.repeat(25)).test('['.repeat(25)) ? 1 : 0",
        "(() => { try { new RegExp('['.repeat(25) + ']'.repeat(25), 'v'); return 0; } catch (e) { return e.message.includes('nests too deeply') ? 1 : 0; } })()",
    ] {
        assert_eq!(evaluate(js).unwrap(), json!(1), "{js}");
    }
}

/// `body` run after `depth` nested calls, with `a` a deeply nested array
/// whose conversion to a string overflows inside the engine.
fn at_depth(depth: u32, body: &str) -> String {
    format!(
        "(() => {{ let a = [1]; for (let i = 0; i < 20000; i++) a = [a]; \
         const run = (n) => n === 0 ? (() => {{ {body} }})() : run(n - 1); return run({depth}); }})()"
    )
}

#[wasm_bindgen_test(unsupported = test)]
fn engine_overflows_cannot_be_swallowed_at_any_depth() {
    let closing = "const it = { [Symbol.iterator]() { return { next: () => ({ value: [1, 1], done: false }), return() { String(a); return {}; } }; } };";
    for body in [
        "let x = 'swallowed'; (async () => { String(a); x = 'completed'; })(); return x;".to_owned(),
        "(async (y = String(a)) => {})(); return 7;".to_owned(),
        format!("{closing} try {{ for (const x of it) throw 1; }} catch (e) {{ return 7; }}"),
        format!("{closing} try {{ Array.from(it, () => {{ throw 1; }}); }} catch (e) {{ return 7; }}"),
        format!("{closing} try {{ new Map({{ [Symbol.iterator]: () => ({{ next: () => ({{ value: 1, done: false }}), return() {{ String(a); return {{}}; }} }}) }}); }} catch (e) {{ return 7; }}"),
        format!("{closing} try {{ Iterator.from(it[Symbol.iterator]()).map(() => {{ throw 1; }}).next(); }} catch (e) {{ return 7; }}"),
        "const it = { [Symbol.iterator]() { return { next: () => ({ value: 1, done: false }), return: () => String(a) }; } }; try { for (const x of it) throw 1; } catch (e) { return 7; }".to_owned(),
        "class It { [Symbol.iterator]() { return this; } next() { return { value: 1, done: false }; } return() { String(a); return {}; } } try { for (const x of new It()) throw 1; } catch (e) { return 7; }".to_owned(),
    ] {
        for depth in [0, 50, 100, 200] {
            assert_stack_overflow(&at_depth(depth, &body));
        }
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn iterator_return_methods_keep_their_meaning() {
    // The checked body keeps the method's `this`, `arguments`, hoisting and
    // return value.
    let js = "(() => { const log = []; \
        const it = { next: () => ({ value: 1, done: false }), \
          return(...rest) { var g = 1; function g() {} log.push(typeof g, this === it, arguments.length); return { done: true, value: 5 }; } }; \
        for (const x of { [Symbol.iterator]: () => it }) break; \
        const result = it.return(1, 2); \
        return log.join() === 'number,true,0,number,true,2' && result.value === 5 ? 1 : 0; })()";
    assert_eq!(evaluate(js).unwrap(), json!(1));
    // A parameter the body redeclares keeps its binding.
    for js in [
        "({ return(x) { var x; return x; } }).return(5)",
        "({ return(x = 1) { var x; return x; } }).return(5)",
        "({ return: (x) => { var x; return x; } }).return(5)",
        "({ return(x, { y }) { var x, y; return x + y; } }).return(2, { y: 3 })",
        "({ return(x) { function x() {} return typeof x === 'function' ? 5 : 0; } }).return(1)",
        "(class { static return(x) { var x; return x; } }).return(5)",
    ] {
        assert_eq!(evaluate(js).unwrap(), json!(5), "{js}");
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn literal_classes_nest_only_with_the_v_flag() {
    // Long enough that the nesting bound reads every token.
    let padding = format!(" + [{}].length * 0", "0, ".repeat(400));
    let shallow = format!(
        "(/{}/.test('{}') ? 1 : 0){padding}",
        "[[]".repeat(25),
        "[".repeat(25)
    );
    assert_eq!(evaluate(&shallow).unwrap(), json!(1));
    let nested = format!(
        "(/{}a{}/v.test('a') ? 1 : 0){padding}",
        "[".repeat(25),
        "]".repeat(25)
    );
    match evaluate(&nested) {
        Err(DtcgError::Expression { message, .. }) if message.contains("nests too deeply") => {}
        other => panic!("{other:?}"),
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn a_stack_overflow_wrapped_however_deeply_is_still_one() {
    // Each throwing callback of a `DisposableStack` wraps the error so far
    // in one more `SuppressedError`; the overflow converts a deeply nested
    // array, which QuickJS raises without the guard seeing it.
    let nested = "let a = [1]; for (let i = 0; i < 100000; i++) a = [a];";
    for callbacks in [100, 2_000, 12_000] {
        for overflow_first in [true, false] {
            let throwing =
                format!("for (let i = 0; i < {callbacks}; i++) s.defer(() => {{ throw 1; }});");
            let overflowing = "s.defer(() => String(a));";
            let defers = if overflow_first {
                format!("{overflowing} {throwing}")
            } else {
                format!("{throwing} {overflowing}")
            };
            assert_stack_overflow(&format!(
                "(() => {{ {nested} try {{ const s = new DisposableStack(); {defers} s.dispose(); }} catch (e) {{ return 7; }} return 0; }})()"
            ));
            assert_stack_overflow(&format!(
                "(() => {{ {nested} (async () => {{ const s = new DisposableStack(); {defers} s.dispose(); }})(); return 7; }})()"
            ));
        }
    }
}
