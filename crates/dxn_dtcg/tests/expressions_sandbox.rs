//! Expressions run without ambient capabilities, within deterministic
//! limits no expression can catch, and repeat their results.
#![cfg(feature = "expressions")]

use dxn_dtcg::{
    DtcgError, DtcgRuntime, EXPRESSION_EXTENSION, ResolvedTokens, RuntimeOptions,
    expressions::ExpressionOptions,
};
use serde_json::{Value, json};

fn options_with(expressions: ExpressionOptions) -> RuntimeOptions {
    RuntimeOptions::default().with_expressions(expressions)
}

fn computed(token_type: &str, value: Value, js: &str) -> Value {
    json!({
        "$type": token_type,
        "$value": value,
        "$extensions": { EXPRESSION_EXTENSION: { "inputs": { "base": "{base}" }, "js": js } }
    })
}

fn runtime(document: &Value, options: RuntimeOptions) -> DtcgRuntime {
    DtcgRuntime::from_json_with_options("tokens.json", document.to_string(), options).unwrap()
}

fn resolve(runtime: &DtcgRuntime) -> Result<ResolvedTokens, DtcgError> {
    runtime
        .resolve(&[])
        .map(|resolution| resolution.ready().unwrap())
}

/// Evaluates `js` as a `fontFamily` (string) token with input `base`
/// (`{ value: 4, unit: "px" }`).
fn text(js: &str, expressions: ExpressionOptions) -> Result<Value, DtcgError> {
    let document = json!({
        "base": { "$type": "dimension", "$value": { "value": 4, "unit": "px" } },
        "computed": computed("fontFamily", json!("x"), js)
    });
    resolve(&runtime(&document, options_with(expressions)))
        .map(|tokens| tokens.get("computed").unwrap().value.clone())
}

fn caught(js: &str) -> String {
    let js = format!(
        "(() => {{ try {{ {js}; return 'no error'; }} catch (e) {{ return String(e); }} }})()"
    );
    match text(&js, ExpressionOptions::default()).unwrap() {
        Value::String(message) => message,
        other => panic!("{other:?}"),
    }
}

fn uncaught(js: &str) -> String {
    match text(js, ExpressionOptions::default()) {
        Err(DtcgError::Expression { path, message, .. }) => {
            assert_eq!(path, "computed");
            message
        }
        other => panic!("{js}: expected an expression error, got {other:?}"),
    }
}

#[test]
fn there_is_no_clock_and_no_randomness() {
    assert!(uncaught("Date.now()").contains("ReferenceError: Date is not defined"));
    assert_eq!(
        text("typeof Date", ExpressionOptions::default()).unwrap(),
        json!("undefined")
    );
    assert!(uncaught("String(Math.random())").contains("Math.random is not available"));
    assert!(caught("Math.random()").contains("Math.random is not available"));
}

#[test]
fn there_is_no_code_from_strings() {
    assert!(uncaught("eval('1')").contains("eval is not defined"));
    for constructor in [
        "Function",
        "(function () {}).constructor",
        "(async function () {}).constructor",
        "(function* () {}).constructor",
        "(async function* () {}).constructor",
    ] {
        let message = caught(&format!("{constructor}('return 1')"));
        assert!(
            message.starts_with("TypeError: the ")
                && message.contains("constructor is not available"),
            "{constructor}: {message}"
        );
    }
}

#[test]
fn promises_proxies_weak_references_timers_and_dynamic_imports_are_unavailable() {
    assert_eq!(
        text(
            "[typeof Promise, typeof Proxy, typeof WeakRef, typeof FinalizationRegistry, typeof setTimeout, typeof setInterval, typeof queueMicrotask].join()",
            ExpressionOptions::default()
        )
        .unwrap(),
        json!("undefined,undefined,undefined,undefined,undefined,undefined,undefined")
    );
    assert!(uncaught("import('anything')").contains("dynamic `import()` is not available"));
    // An async function's promise is not a JSON value, and never settles.
    assert!(uncaught("(async () => 'x')()").contains("not a JSON value"));
}

#[test]
fn globals_and_built_in_prototypes_are_frozen() {
    for write in [
        "globalThis.leak = 1",
        "Object.prototype.leak = 1",
        "Array.prototype.map = null",
        "Math.leak = 1",
        "JSON.parse = null",
        "String.prototype.trim = null",
        "Object.defineProperty(globalThis, 'Math', { value: 1 })",
        "delete globalThis.Math",
    ] {
        let message = caught(write);
        assert!(message.starts_with("TypeError"), "{write}: {message}");
    }
    // Own members that shadow frozen prototype members still work.
    assert_eq!(
        text(
            "(() => { class Tagged extends Error { constructor() { super('m'); this.name = 'Tagged'; } } class P { constructor() { this.toString = () => 'own'; } } return new Tagged().name + String(new P()); })()",
            ExpressionOptions::default()
        )
        .unwrap(),
        json!("Taggedown")
    );
}

#[test]
fn inputs_are_frozen() {
    assert!(caught("base.value = 9").starts_with("TypeError"));
    assert!(uncaught("(base.unit = 'rem', 'x')").contains("TypeError"));
}

#[test]
fn every_failure_is_an_expression_error_or_a_caught_value() {
    assert_eq!(caught("Date.now()"), "ReferenceError: Date is not defined");
    assert!(uncaught("null.x").starts_with("TypeError"));
    assert!(uncaught("(() => { throw 'plain'; })()").contains("uncaught exception: plain"));
}

#[test]
fn an_infinite_loop_exceeds_the_step_budget_uncatchably() {
    for js in [
        "(() => { for (;;) {} })()",
        "(() => { try { for (;;) {} } catch { return 'caught'; } })()",
        "(() => { try { for (;;) {} } finally { return 'finally'; } })()",
    ] {
        assert!(
            matches!(
                text(js, ExpressionOptions::default()),
                Err(DtcgError::LimitExceeded {
                    kind: "expression steps",
                    limit: 1_000
                })
            ),
            "{js}"
        );
    }
}

#[test]
fn the_step_budget_is_configurable() {
    let work =
        "(() => { let n = 0; for (let i = 0; i < 2000000; i++) n += i; return String(n > 0); })()";
    assert_eq!(
        text(work, ExpressionOptions::default()).unwrap(),
        json!("true")
    );
    assert!(matches!(
        text(work, ExpressionOptions::default().with_step_budget(5)),
        Err(DtcgError::LimitExceeded {
            kind: "expression steps",
            limit: 5
        })
    ));
}

#[test]
fn exhausting_memory_exceeds_the_memory_limit_uncatchably() {
    for js in [
        "(() => { const all = []; for (;;) all.push(new Array(100000).fill(1)); })()",
        "(() => { try { const all = []; for (;;) all.push(new Array(100000).fill(1)); } catch { return 'caught'; } })()",
        "(() => { try { return 'x'.repeat(2 ** 28); } catch { return 'caught'; } })()",
    ] {
        assert!(
            matches!(
                text(js, ExpressionOptions::default()),
                Err(DtcgError::LimitExceeded { kind: "expression memory", limit }) if limit == 64 * 1024 * 1024
            ),
            "{js}"
        );
    }
    let allocation = "(() => String(new Array(1000000).fill(0).length))()";
    assert_eq!(
        text(allocation, ExpressionOptions::default()).unwrap(),
        json!("1000000")
    );
    assert!(matches!(
        text(
            allocation,
            ExpressionOptions::default().with_memory_limit(4 * 1024 * 1024)
        ),
        Err(DtcgError::LimitExceeded {
            kind: "expression memory",
            ..
        })
    ));
}

#[test]
fn runaway_recursion_is_a_stack_overflow_uncatchably() {
    for js in [
        "(() => { const f = (n) => f(n + 1) + 1; return f(0); })()",
        "(() => { const f = (n) => { try { return f(n + 1) + 1; } catch { return 0; } }; return String(f(0)); })()",
    ] {
        let message = uncaught(js);
        assert!(message.contains("stack overflow"), "{js}: {message}");
    }
    // Ordinary recursion fits the default limit on every platform (frames
    // are larger on x86_64 than on aarch64).
    let deep = "(() => { const f = (n) => n === 0 ? 0 : f(n - 1) + 1; return String(f(100)); })()";
    assert_eq!(
        text(deep, ExpressionOptions::default()).unwrap(),
        json!("100")
    );
    let message = match text(
        deep,
        ExpressionOptions::default().with_stack_limit(16 * 1024),
    ) {
        Err(DtcgError::Expression { message, .. }) => message,
        other => panic!("{other:?}"),
    };
    assert!(message.contains("stack overflow"), "{message}");
}

#[test]
fn the_runtime_resolves_again_after_every_limit() {
    let base = json!({ "$type": "dimension", "$value": { "value": 4, "unit": "px" } });
    for js in [
        "(() => { for (;;) {} })()",
        "(() => { const all = []; for (;;) all.push(new Array(100000).fill(1)); })()",
        "(() => { const f = (n) => f(n + 1) + 1; return f(0); })()",
    ] {
        let failing = json!({ "base": base, "computed": computed("fontFamily", json!("x"), js) });
        let mut runtime = runtime(&failing, options_with(ExpressionOptions::default()));
        assert!(resolve(&runtime).is_err(), "{js}");
        assert!(resolve(&runtime).is_err(), "{js}");
        let working =
            json!({ "base": base, "computed": computed("fontFamily", json!("x"), "'ok'") });
        runtime
            .replace_json("tokens.json", working.to_string())
            .unwrap();
        assert_eq!(
            resolve(&runtime).unwrap().get("computed").unwrap().value,
            json!("ok"),
            "{js}"
        );
    }
}

fn many(order: &[usize]) -> Value {
    let mut document = serde_json::Map::new();
    document.insert(
        "base".into(),
        json!({ "$type": "dimension", "$value": { "value": 4, "unit": "px" } }),
    );
    for index in order {
        document.insert(
            format!("token{index}"),
            computed(
                "number",
                json!(0),
                &format!(
                    "(globalThis.counter = (globalThis.counter ?? 0) + 1, base.value * {index})"
                ),
            ),
        );
        // Try to leave state for later expressions; every attempt fails.
        document.insert(
            format!("probe{index}"),
            computed(
                "number",
                json!(0),
                "(() => { try { globalThis.shared = 1; } catch {} try { Object.prototype.shared = 1; } catch {} return typeof globalThis.shared === 'undefined' && ({}).shared === undefined ? 1 : 0; })()",
            ),
        );
    }
    Value::Object(document)
}

#[test]
fn expressions_cannot_share_state() {
    // `globalThis.counter = …` throws: every expression sees a pristine
    // global object, whatever ran before it.
    let tokens = resolve(&runtime(
        &many(&[1, 2, 3]),
        options_with(ExpressionOptions::default()),
    ));
    assert!(matches!(tokens, Err(DtcgError::Expression { .. })));

    let document = json!({
        "base": { "$type": "dimension", "$value": { "value": 4, "unit": "px" } },
        "first": computed("number", json!(0), "(() => { try { globalThis.shared = 7; } catch {} return 1; })()"),
        "second": computed("number", json!(0), "typeof globalThis.shared === 'undefined' ? 2 : 0"),
    });
    let tokens = resolve(&runtime(
        &document,
        options_with(ExpressionOptions::default()),
    ))
    .unwrap();
    assert_eq!(tokens.get("second").unwrap().value, json!(2));
}

#[test]
fn resolution_repeats_and_is_independent_of_evaluation_order() {
    let probes = |order: &[usize]| {
        let mut document = many(order);
        for index in order {
            document[format!("token{index}")] =
                computed("number", json!(0), &format!("base.value * {index} / 3"));
        }
        document
    };
    let options = options_with(ExpressionOptions::default());
    let forward = runtime(&probes(&[1, 2, 3, 4]), options.clone());
    let first = resolve(&forward).unwrap();
    let second = resolve(&forward).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.fingerprint(), second.fingerprint());
    assert!(
        first
            .iter()
            .filter(|token| token.path.starts_with("probe"))
            .all(|token| token.value == json!(1))
    );

    let backward = resolve(&runtime(&probes(&[4, 3, 2, 1]), options.clone())).unwrap();
    for token in first.iter() {
        assert_eq!(backward.get(&token.path).unwrap().value, token.value);
    }

    // Aliases make `token4` evaluate first, before its authored position.
    let mut aliased = probes(&[1, 2, 3, 4]);
    aliased["alias"] = json!({ "$type": "number", "$value": "{token4}" });
    let aliased_object = aliased.as_object_mut().unwrap();
    let alias = aliased_object.shift_remove("alias").unwrap();
    let mut reordered = serde_json::Map::new();
    reordered.insert("alias".into(), alias);
    reordered.extend(aliased_object.clone());
    let reordered = resolve(&runtime(&Value::Object(reordered), options)).unwrap();
    for token in first.iter() {
        assert_eq!(reordered.get(&token.path).unwrap().value, token.value);
    }
    assert_eq!(
        reordered.get("alias").unwrap().value,
        first.get("token4").unwrap().value
    );
}

#[test]
fn a_resolution_without_expression_tokens_never_starts_the_engine() {
    let tiny = options_with(ExpressionOptions::default().with_memory_limit(1));
    let plain = json!({
        "base": { "$type": "dimension", "$value": { "value": 4, "unit": "px" } },
        "other": { "$type": "number", "$value": 1, "$extensions": { "vendor.data": {} } }
    });
    assert_eq!(resolve(&runtime(&plain, tiny.clone())).unwrap().len(), 2);

    let mut with_expression = plain.clone();
    with_expression["computed"] = computed("fontFamily", json!("x"), "'y'");
    assert!(matches!(
        resolve(&runtime(&with_expression, tiny)),
        Err(DtcgError::LimitExceeded {
            kind: "expression memory",
            limit: 1
        })
    ));
}

/// The value of `language` source `source` as a `number` token, with
/// `expressions`.
fn number(
    language: &str,
    source: &str,
    expressions: ExpressionOptions,
) -> Result<Value, DtcgError> {
    let mut expression = serde_json::Map::new();
    expression.insert(language.into(), json!(source));
    let document = json!({
        "computed": { "$type": "number", "$value": 0, "$extensions": { EXPRESSION_EXTENSION: expression } }
    });
    resolve(&runtime(&document, options_with(expressions)))
        .map(|tokens| tokens.get("computed").unwrap().value.clone())
}

#[test]
fn deeply_nested_source_text_fails_without_overflowing_the_stack() {
    let deep = 100_000;
    let cases = [
        format!("{}1{}", "(".repeat(deep), ")".repeat(deep)),
        format!("{}1{}", "[".repeat(deep), "]".repeat(deep)),
        format!("{}1", "-".repeat(deep)),
        format!("{}1", "!".repeat(deep)),
        format!("{}1", "typeof ".repeat(deep)),
        format!("{}1", "new ".repeat(deep)),
        format!("{}1", "x ? 1 : ".repeat(deep)),
        format!("{}1", "2 ** ".repeat(deep)),
        format!("{}1", "x => ".repeat(deep)),
        format!("{}1{}", "`${".repeat(deep), "}`".repeat(deep)),
        format!("{}1{}", "{ a: ".repeat(deep), " }".repeat(deep)),
        format!("(() => {{ {}return 1; }})()", "if (1) ".repeat(deep)),
        format!(
            "(() => {{ if (0) {{}} {}return 1; }})()",
            "else if (0) {} else ".repeat(deep)
        ),
        format!("(() => {{ {}1{} }})()", "{".repeat(deep), "}".repeat(deep)),
        format!("(() => {{ let x; {}1; }})()", "x = ".repeat(deep)),
        format!("(class {})", "extends class ".repeat(deep)),
        // A division the regular expression reading would take for a
        // string start, hiding the parentheses after it.
        format!(
            "(() => {{ if (1)/'/; {}1{}; '' }})()",
            "(".repeat(deep),
            ")".repeat(deep)
        ),
        format!(
            "(() => {{ return (1)/'/ + {}1{} + ''; }})()",
            "(".repeat(deep),
            ")".repeat(deep)
        ),
        // Left-associative chains deepen the syntax tree oxc walks.
        format!("1{}", " + 1".repeat(deep)),
        format!("0{}", " || 1".repeat(deep)),
        format!("'x'{}", ".length".repeat(deep)),
        format!("(() => () => 1){}", "()".repeat(deep)),
        format!("[[1]]{}", "[0]".repeat(deep)),
        // Groups nest inside a regular expression literal.
        format!(
            "/{}x{}/.source.length",
            "(?:".repeat(30_000),
            ")".repeat(30_000)
        ),
        // No reading accepts a control character; the check fails closed.
        format!("1 \u{1} {}1{}", "(".repeat(deep), ")".repeat(deep)),
        // TypeScript's postfix `!` ends an operand: the `/` divides.
        format!("(0)! / 2 + {}1{}", "(".repeat(10_000), ")".repeat(10_000)),
    ];
    for source in &cases {
        for language in ["js", "ts"] {
            if source.contains(")! /") && language == "js" {
                continue;
            }
            match number(language, source, ExpressionOptions::default()) {
                Err(DtcgError::Expression { message, .. }) => assert!(
                    message.contains("nests too deeply"),
                    "{language} {}: {message}",
                    &source[..40]
                ),
                other => panic!("{language} {}: {other:?}", &source[..40]),
            }
        }
    }
    let typescript = format!(
        "(() => {{ let x: {}number{} = 1; return x; }})()",
        "Array<".repeat(deep),
        ">".repeat(deep)
    );
    assert!(matches!(
        number("ts", &typescript, ExpressionOptions::default()),
        Err(DtcgError::Expression { ref message, .. }) if message.contains("nests too deeply")
    ));
}

#[test]
fn ordinary_nesting_evaluates() {
    let parens = format!("{}1{}", "(".repeat(100), ")".repeat(100));
    assert_eq!(
        number("js", &parens, ExpressionOptions::default()).unwrap(),
        json!(1)
    );
    let chain = format!("{}1", "0 ? 0 : ".repeat(100));
    assert_eq!(
        number("ts", &chain, ExpressionOptions::default()).unwrap(),
        json!(1)
    );
    // Statements without semicolons reset at every line.
    let statements = (0..600).map(|i| format!("x = {i}\n")).collect::<String>();
    let source = format!("(() => {{\nlet x\n{statements}return x\n}})()");
    assert_eq!(
        number("js", &source, ExpressionOptions::default()).unwrap(),
        json!(599)
    );
}

#[test]
fn tamed_members_promises_iterators_and_stack_hooks_are_frozen() {
    for write in [
        "({}).toString.leak = 1",
        "Object.prototype.hasOwnProperty.leak = 1",
        "Object.getPrototypeOf((async () => {})()).leak = 1",
        "Object.getPrototypeOf((async () => {})()).then.leak = 1",
        "Object.getPrototypeOf([].values().map((x) => x)).leak = 1",
        "Object.getPrototypeOf(Iterator.from({ next() { return { done: true }; } })).leak = 1",
        "Error.prepareStackTrace = () => 'leak'",
        "Error.stackTraceLimit = 0",
    ] {
        let message = caught(write);
        assert!(message.starts_with("TypeError"), "{write}: {message}");
    }
    assert!(
        caught("new ((async () => {})().constructor)(() => {})")
            .contains("Promise is not available")
    );

    // Every object reachable from the globals and from what syntax creates
    // is frozen.
    let walk = "(() => {
      const generator = function* () {};
      const asyncGenerator = async function* () {};
      // Each generator function has its own, unshared prototype object.
      const own = [generator.prototype, asyncGenerator.prototype];
      const roots = [globalThis, (async () => {})(), generator, generator(), async function () {},
        asyncGenerator, asyncGenerator(), [][Symbol.iterator](), ''[Symbol.iterator](),
        new Map().entries(), new Set().values(), 'a'.matchAll(/a/g), [].values().map((x) => x),
        Iterator.from({ next() { return { done: true }; } })];
      const seen = new Set();
      const pending = roots.flatMap((root) => [root, Object.getPrototypeOf(root)]);
      const open = [];
      while (pending.length > 0) {
        const value = pending.pop();
        if (value === null || (typeof value !== 'object' && typeof value !== 'function') || seen.has(value)) continue;
        seen.add(value);
        if (!Object.isFrozen(value) && !roots.includes(value) && !own.includes(value)) open.push(String(value.name ?? typeof value));
        for (const key of Reflect.ownKeys(value)) {
          const d = Object.getOwnPropertyDescriptor(value, key);
          if ('value' in d) pending.push(d.value); else pending.push(d.get, d.set);
        }
        pending.push(Object.getPrototypeOf(value));
      }
      return open.join(',') || 'all frozen';
    })()";
    assert_eq!(
        text(walk, ExpressionOptions::default()).unwrap(),
        json!("all frozen")
    );
}

#[test]
fn a_write_through_shared_built_ins_never_reaches_another_expression() {
    let document = json!({
        "base": { "$type": "dimension", "$value": { "value": 4, "unit": "px" } },
        "writer": computed("number", json!(0), "(() => { for (const write of [() => { ({}).toString.leak = 'x'; }, () => { Object.getPrototypeOf((async () => {})()).leak = 'x'; }, () => { Error.prepareStackTrace = () => 'x'; }]) { try { write(); } catch {} } return 1; })()"),
        "reader": computed("fontFamily", json!("x"), "[typeof ({}).toString.leak, typeof Object.getPrototypeOf((async () => {})()).leak, typeof Error.prepareStackTrace].join()"),
    });
    let tokens = resolve(&runtime(
        &document,
        options_with(ExpressionOptions::default()),
    ))
    .unwrap();
    assert_eq!(
        tokens.get("reader").unwrap().value,
        json!("undefined,undefined,undefined")
    );
}

#[test]
fn the_step_budget_does_not_depend_on_what_ran_before() {
    let options = || options_with(ExpressionOptions::default().with_step_budget(1));
    let looping = |iterations: u32| {
        computed(
            "number",
            json!(0),
            &format!(
                "(() => {{ let n = 0; for (let i = 0; i < {iterations}; i++) n++; return n; }})()"
            ),
        )
    };
    let base = json!({ "$type": "dimension", "$value": { "value": 4, "unit": "px" } });
    // Up to 6,500 iterations (about two interrupt polls each) need one
    // interrupt check: within a budget of 1, alone and after any other
    // evaluation.
    let mut fits = serde_json::Map::new();
    fits.insert("base".into(), base.clone());
    for index in 0..12 {
        fits.insert(format!("token{index}"), looping(1_000 + index * 500));
    }
    let forward = resolve(&runtime(&Value::Object(fits.clone()), options())).unwrap();
    let mut reversed = serde_json::Map::new();
    reversed.insert("base".into(), base.clone());
    for (key, value) in fits.iter().rev().filter(|(key, _)| *key != "base") {
        reversed.insert(key.clone(), value.clone());
    }
    let backward = resolve(&runtime(&Value::Object(reversed), options())).unwrap();
    for token in forward.iter() {
        assert_eq!(backward.get(&token.path).unwrap().value, token.value);
    }
    assert_eq!(forward.get("token11").unwrap().value, json!(6_500));

    // 15,000 iterations need more checks: over the budget wherever they run.
    for position in [0, 6, 12] {
        let mut document = serde_json::Map::new();
        document.insert("base".into(), base.clone());
        for (index, (key, value)) in fits.iter().filter(|(key, _)| *key != "base").enumerate() {
            if index == position {
                document.insert("over".into(), looping(15_000));
            }
            document.insert(key.clone(), value.clone());
        }
        if position == 12 {
            document.insert("over".into(), looping(15_000));
        }
        assert!(
            matches!(
                resolve(&runtime(&Value::Object(document), options())),
                Err(DtcgError::LimitExceeded {
                    kind: "expression steps",
                    limit: 1
                })
            ),
            "{position}"
        );
    }

    // Affected resolution evaluates a subset and agrees with a full one.
    let mut runtime = runtime(&Value::Object(fits.clone()), options());
    let mut tokens = resolve(&runtime).unwrap();
    let mut edited = fits.clone();
    edited["token11"] = looping(6_400);
    let affected = runtime
        .replace_json_and_resolve_affected(
            "tokens.json",
            Value::Object(edited).to_string(),
            &[],
            &[],
        )
        .unwrap()
        .ready()
        .unwrap();
    tokens.apply_affected(&affected);
    assert_eq!(tokens, resolve(&runtime).unwrap());
    assert_eq!(tokens.get("token11").unwrap().value, json!(6_400));
}

#[test]
fn a_step_budget_below_one_counts_as_one() {
    let document = |order: &[&str]| {
        let mut document = serde_json::Map::new();
        document.insert(
            "base".into(),
            json!({ "$type": "dimension", "$value": { "value": 4, "unit": "px" } }),
        );
        for name in order {
            let js = if *name == "a" {
                "(() => { let n = 0; for (let i = 0; i < 4996; i++) n++; return n; })()"
            } else {
                "1"
            };
            document.insert((*name).into(), computed("number", json!(0), js));
        }
        Value::Object(document)
    };
    let options = || options_with(ExpressionOptions::default().with_step_budget(0));
    for order in [&["a"][..], &["b"], &["a", "b"], &["b", "a"]] {
        assert!(
            resolve(&runtime(&document(order), options())).is_ok(),
            "{order:?}"
        );
    }
}

#[test]
fn a_stack_limit_below_the_minimum_counts_as_the_minimum() {
    assert_eq!(
        number(
            "js",
            "1 + 1",
            ExpressionOptions::default().with_stack_limit(0)
        )
        .unwrap(),
        json!(2)
    );
    let recursion = "(() => { const f = (n) => n === 0 ? 0 : f(n - 1) + 1; return f(100000); })()";
    match number(
        "js",
        recursion,
        ExpressionOptions::default().with_stack_limit(0),
    ) {
        Err(DtcgError::Expression { message, .. }) => assert!(message.contains("stack overflow")),
        other => panic!("{other:?}"),
    }
}

/// The deepest input of each construct that passes the nesting check
/// compiles without overflowing a 1 MiB stack, the default stack of
/// `wasm32-unknown-unknown`, even unoptimized.
#[test]
fn the_deepest_accepted_sources_compile_on_a_one_mebibyte_stack() {
    type Construct = fn(usize) -> String;
    let constructs: Vec<(&str, Construct)> = vec![
        ("js", |n| format!("{}1{}", "(".repeat(n), ")".repeat(n))),
        ("js", |n| format!("{}1{}", "[".repeat(n), "]".repeat(n))),
        ("js", |n| {
            format!("{}1{}", "{ a: ".repeat(n), " }".repeat(n))
        }),
        ("js", |n| format!("{}1{}", "`${".repeat(n), "}`".repeat(n))),
        ("js", |n| format!("1{}", " + 1".repeat(n))),
        ("js", |n| format!("{}1", "-".repeat(n))),
        ("js", |n| format!("{}1", "x ? 1 : ".repeat(n))),
        ("js", |n| format!("{}1", "x => ".repeat(n))),
        ("js", |n| format!("{}1", "2 ** ".repeat(n))),
        ("js", |n| format!("{}1", "new ".repeat(n))),
        ("js", |n| {
            format!("(() => {{ let x; {}1; }})()", "x = ".repeat(n))
        }),
        ("js", |n| {
            format!("(() => {{ {}return 1; }})()", "if (1) ".repeat(n))
        }),
        ("js", |n| {
            format!("(() => {{ {}1{} }})()", "{".repeat(n), "}".repeat(n))
        }),
        ("js", |n| {
            format!(
                "{}1{}",
                "(function () { return ".repeat(n),
                "; })()".repeat(n)
            )
        }),
        ("js", |n| {
            format!("/{}x{}/.source.length", "(?:".repeat(n), ")".repeat(n))
        }),
        ("ts", |n| format!("{}1{}", "(".repeat(n), ")".repeat(n))),
        ("ts", |n| {
            format!(
                "(() => {{ let x: {}number{} = 1; return x; }})()",
                "Array<".repeat(n),
                ">".repeat(n)
            )
        }),
        ("ts", |n| {
            format!(
                "(() => {{ let x: {}number{} = 1; return x; }})()",
                "{ a: ".repeat(n),
                " }".repeat(n)
            )
        }),
    ];
    let worker = std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(move || {
            for (language, construct) in constructs {
                let rejected = |n: usize| {
                    matches!(
                        number(language, &construct(n), ExpressionOptions::default()),
                        Err(DtcgError::Expression { ref message, .. }) if message.contains("nests too deeply")
                    )
                };
                let (mut accepted, mut refused) = (1, 4_096);
                assert!(rejected(refused), "{}", construct(4));
                while refused - accepted > 1 {
                    let middle = (accepted + refused) / 2;
                    if rejected(middle) {
                        refused = middle;
                    } else {
                        accepted = middle;
                    }
                }
                // Compiling (and evaluating) the deepest accepted source
                // returns, whatever its result.
                let _ = number(language, &construct(accepted), ExpressionOptions::default());
            }
        })
        .unwrap();
    worker.join().unwrap();
}
