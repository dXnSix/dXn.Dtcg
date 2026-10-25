//! Expressions import ES modules the host registers, with standard module
//! semantics, and module failures name the module and the specifier.
#![cfg(feature = "expressions")]

use dxn_dtcg::{
    DtcgError, DtcgRuntime, EXPRESSION_EXTENSION, ResolvedTokens, RuntimeOptions,
    expressions::{ExpressionModule, ExpressionOptions},
};
use serde_json::{Value, json};
use wasm_bindgen_test::wasm_bindgen_test;

const TINYCOLOR: &str = include_str!("fixtures/expressions/tinycolor.js");

fn resolve(document: &Value, expressions: ExpressionOptions) -> Result<ResolvedTokens, DtcgError> {
    DtcgRuntime::from_json_with_options(
        "tokens.json",
        document.to_string(),
        RuntimeOptions::default().with_expressions(expressions),
    )
    .unwrap()
    .resolve(&[])
    .map(|resolution| resolution.ready().unwrap())
}

/// A token of `token_type` computed by `expression` (an object with
/// `imports`, `inputs` and `js` or `ts`).
fn computed(token_type: &str, value: Value, expression: Value) -> Value {
    json!({ "$type": token_type, "$value": value, "$extensions": { EXPRESSION_EXTENSION: expression } })
}

/// The value of a `number` token computed by `expression`.
fn number(expression: Value, expressions: ExpressionOptions) -> Result<Value, DtcgError> {
    resolve(
        &json!({ "computed": computed("number", json!(0), expression) }),
        expressions,
    )
    .map(|tokens| tokens.get("computed").unwrap().value.clone())
}

fn message(result: Result<Value, DtcgError>) -> (String, String, String) {
    match result {
        Err(DtcgError::Expression {
            resource,
            path,
            message,
        }) => (resource.to_string(), path, message),
        other => panic!("expected an expression error, got {other:?}"),
    }
}

fn math() -> ExpressionOptions {
    ExpressionOptions::default()
        .with_module(
            "brand/math",
            ExpressionModule::javascript(
                "export const ratio = 1.5;\nexport function step(base, n) { return base * ratio ** n; }\nexport default 42;",
            ),
        )
        .with_module(
            "brand/types",
            ExpressionModule::typescript(
                "export interface Pair { a: number; b: number }\nexport const sum = (pair: Pair): number => pair.a + pair.b;\nexport type Unused = string;",
            ),
        )
}

#[wasm_bindgen_test(unsupported = test)]
fn tokens_import_named_exports_and_namespaces() {
    assert_eq!(
        number(
            json!({ "imports": { "brand/math": ["step", "ratio"] }, "js": "step(4, 2) + ratio" }),
            math()
        )
        .unwrap(),
        json!(10.5)
    );
    assert_eq!(
        number(
            json!({ "imports": { "brand/math": "math" }, "js": "math.step(2, 1) + math.default" }),
            math()
        )
        .unwrap(),
        json!(45)
    );
    assert_eq!(
        number(
            json!({ "imports": { "brand/types": ["sum"] }, "ts": "sum({ a: 1, b: 2 } as { a: number; b: number })" }),
            math()
        )
        .unwrap(),
        json!(3)
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn modules_import_modules_with_standard_semantics() {
    let options = ExpressionOptions::default()
        .with_module("lib/base", ExpressionModule::javascript("export const one = 1;\nexport const two = 2;"))
        // Re-exports: named, renamed and star.
        .with_module(
            "lib/reexport",
            ExpressionModule::javascript(
                "export { one as uno } from 'lib/base';\nexport * from 'lib/base';\nexport * as base from 'lib/base';",
            ),
        )
        // A live binding: importers see the value after `bump` runs.
        .with_module(
            "lib/state",
            ExpressionModule::javascript("export let count = 0;\nexport function bump() { count += 1; return count; }"),
        )
        .with_module(
            "lib/live",
            ExpressionModule::javascript(
                "import { count, bump } from 'lib/state';\nconst before = count;\nbump();\nexport const seen = [before, count];",
            ),
        )
        // A cycle: each module calls the other's hoisted function.
        .with_module(
            "lib/even",
            ExpressionModule::javascript(
                "import { odd } from 'lib/odd';\nexport function even(n) { return n === 0 ? true : odd(n - 1); }",
            ),
        )
        .with_module(
            "lib/odd",
            ExpressionModule::javascript(
                "import { even } from 'lib/even';\nexport function odd(n) { return n === 0 ? false : even(n - 1); }",
            ),
        );
    assert_eq!(
        number(
            json!({ "imports": { "lib/reexport": ["uno", "two", "base"] }, "js": "uno + two * 10 + base.one * 100" }),
            options.clone()
        )
        .unwrap(),
        json!(121)
    );
    assert_eq!(
        number(
            json!({ "imports": { "lib/live": ["seen"] }, "js": "seen[0] * 10 + seen[1]" }),
            options.clone()
        )
        .unwrap(),
        json!(1)
    );
    assert_eq!(
        number(
            json!({ "imports": { "lib/even": ["even"], "lib/odd": ["odd"] }, "js": "(even(10) ? 1 : 0) + (odd(7) ? 10 : 0)" }),
            options
        )
        .unwrap(),
        json!(11)
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn only_static_imports_exist() {
    let options = ExpressionOptions::default().with_module(
        "lib/dynamic",
        ExpressionModule::javascript("export const load = () => import('lib/other');"),
    );
    let (_, path, message) = message(number(
        json!({ "imports": { "lib/dynamic": ["load"] }, "js": "1" }),
        options,
    ));
    assert_eq!(path, "computed");
    assert!(message.contains("module `lib/dynamic`"), "{message}");
    assert!(message.contains("dynamic `import()`"), "{message}");
}

#[wasm_bindgen_test(unsupported = test)]
fn an_unknown_specifier_or_export_names_the_token_and_the_specifier() {
    let (resource, path, text) = message(number(
        json!({ "imports": { "left-pad": ["pad"] }, "js": "pad" }),
        math(),
    ));
    assert_eq!(
        (resource.as_str(), path.as_str()),
        ("tokens.json", "computed")
    );
    assert!(text.contains("unknown module `left-pad`"), "{text}");

    let (_, path, text) = message(number(
        json!({ "imports": { "brand/math": ["missing"] }, "js": "missing" }),
        math(),
    ));
    assert_eq!(path, "computed");
    assert!(
        text.contains("missing") && text.contains("brand/math"),
        "{text}"
    );

    // A module importing what nothing provides.
    let options = math().with_module(
        "lib/broken",
        ExpressionModule::javascript("export { nothing } from 'nowhere';"),
    );
    let (_, _, text) = message(number(
        json!({ "imports": { "lib/broken": "broken" }, "js": "1" }),
        options,
    ));
    assert!(
        text.contains("module `lib/broken`") && text.contains("unknown module `nowhere`"),
        "{text}"
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn host_registrations_need_bare_specifiers_outside_the_reserved_namespace() {
    for specifier in ["dxn.dtcg", "dxn.dtcg/color", "dxn.dtcg/anything"] {
        let options = ExpressionOptions::default().with_module(
            specifier,
            ExpressionModule::javascript("export const x = 1;"),
        );
        let (_, _, text) = message(number(json!({ "js": "1" }), options));
        assert!(
            text.contains(specifier) && text.contains("reserved"),
            "{text}"
        );
    }
    let options =
        ExpressionOptions::default().with_module("./local.js", ExpressionModule::javascript(""));
    let (_, _, text) = message(number(json!({ "js": "1" }), options));
    assert!(text.contains("bare specifier"), "{text}");
}

#[wasm_bindgen_test(unsupported = test)]
fn module_syntax_and_evaluation_errors_name_the_module() {
    let options = ExpressionOptions::default()
        .with_module(
            "lib/syntax",
            ExpressionModule::javascript("export const a = 1;\nexport const b = (;\n"),
        )
        .with_module(
            "lib/types",
            ExpressionModule::typescript("export const a: number = 1;\nlet b: = 2;\n"),
        )
        .with_module(
            "lib/throws",
            ExpressionModule::javascript("export const a = 1;\nthrow new RangeError('at load');\n"),
        );
    let (_, path, text) = message(number(
        json!({ "imports": { "lib/syntax": ["a"] }, "js": "a" }),
        options.clone(),
    ));
    assert_eq!(path, "computed");
    assert!(
        text.starts_with("module `lib/syntax`: SyntaxError"),
        "{text}"
    );
    assert!(text.contains("at line 2, column 19"), "{text}");
    let (_, _, text) = message(number(
        json!({ "imports": { "lib/types": ["a"] }, "js": "a" }),
        options.clone(),
    ));
    assert!(
        text.starts_with("module `lib/types`: SyntaxError") && text.contains("at line 2, column "),
        "{text}"
    );
    let (_, _, text) = message(number(
        json!({ "imports": { "lib/throws": ["a"] }, "js": "a" }),
        options,
    ));
    assert!(
        text.starts_with("module `lib/throws`: RangeError: at load"),
        "{text}"
    );
    assert!(text.contains("at line 2, column "), "{text}");
}

#[wasm_bindgen_test(unsupported = test)]
fn module_top_level_code_has_its_own_step_budget() {
    let options = ExpressionOptions::default().with_module(
        "lib/spin",
        ExpressionModule::javascript("for (;;) {}\nexport const a = 1;"),
    );
    assert!(matches!(
        number(
            json!({ "imports": { "lib/spin": ["a"] }, "js": "a" }),
            options
        ),
        Err(DtcgError::LimitExceeded {
            kind: "expression steps",
            ..
        })
    ));
    let awaits = ExpressionOptions::default().with_module(
        "lib/await",
        ExpressionModule::javascript("await null;\nexport const a = 1;"),
    );
    let (_, _, text) = message(number(
        json!({ "imports": { "lib/await": ["a"] }, "js": "a" }),
        awaits,
    ));
    assert!(text.contains("top-level `await`"), "{text}");
}

#[wasm_bindgen_test(unsupported = test)]
fn a_color_library_works_unmodified_as_a_host_module() {
    let options = ExpressionOptions::default()
        .with_module("tinycolor", ExpressionModule::javascript(TINYCOLOR));
    let accent = json!({ "colorSpace": "srgb", "components": [0.2, 0.4, 0.9] });
    let document = json!({
        "accent": { "$type": "color", "$value": accent },
        "hover": computed("color", accent.clone(), json!({
            "imports": { "tinycolor": ["lighten"] },
            "inputs": { "accent": "{accent}" },
            "js": "lighten(accent, 0.1)"
        })),
        "muted": computed("color", accent.clone(), json!({
            "imports": { "tinycolor": ["mix"] },
            "inputs": { "accent": "{accent}" },
            "js": "mix(accent, { colorSpace: 'srgb', components: [1, 1, 1] }, 0.5)"
        })),
        "luminance": computed("number", json!(0), json!({
            "imports": { "tinycolor": "tc" },
            "inputs": { "accent": "{accent}" },
            "js": "tc.roundTo(tc.luminance(accent), 4) + tc.clamp(2) + (tc.default === tc.Color ? 10 : 0) + tc.whitePoint.length * 100"
        })),
        "oklch": computed("number", json!(0), json!({
            "imports": { "tinycolor": ["Color"] },
            "inputs": { "accent": "{accent}" },
            "ts": "((): number => { const { l } = Color.fromToken(accent).oklch(); return Math.round(l * 1000) / 1000; })()"
        }))
    });
    let tokens = resolve(&document, options).unwrap();
    assert_eq!(
        tokens.get("muted").unwrap().value,
        json!({ "colorSpace": "srgb", "components": [0.6, 0.7, 0.95] })
    );
    let hover = &tokens.get("hover").unwrap().value;
    let components = hover["components"].as_array().unwrap();
    assert!(
        components
            .iter()
            .zip([0.2, 0.4, 0.9])
            .all(|(after, before)| after.as_f64().unwrap() >= before)
    );
    assert_eq!(tokens.get("oklch").unwrap().value, json!(0.552));
    let luminance = tokens.get("luminance").unwrap().value.as_f64().unwrap();
    assert!((311.0..312.0).contains(&luminance), "{luminance}");
}

fn cycle(a_extra: &str, b_extra: &str) -> ExpressionOptions {
    ExpressionOptions::default()
        // Module values are frozen once their module ran, so the order is
        // observed through a counter rather than a shared array.
        .with_module(
            "lib/log",
            ExpressionModule::javascript("let next = 0;\nexport function tick(name) { next += 1; return name + next; }"),
        )
        .with_module(
            "lib/a",
            ExpressionModule::javascript(format!(
                "import {{ tick }} from 'lib/log';\nimport {{ b }} from 'lib/b';\nexport const ranA = tick('a');\n{a_extra}\nexport const a = 1;"
            )),
        )
        .with_module(
            "lib/b",
            ExpressionModule::javascript(format!(
                "import {{ tick }} from 'lib/log';\nimport {{ a }} from 'lib/a';\nexport const ranB = tick('b');\n{b_extra}\nexport const b = 2;"
            )),
        )
}

#[wasm_bindgen_test(unsupported = test)]
fn an_import_cycle_evaluates_in_es_module_order() {
    // Entered at `lib/a`, the cycle evaluates `lib/b` first, as ES modules do.
    let order = |options: ExpressionOptions| {
        let document = json!({ "computed": computed("fontFamily", json!("x"), json!({
            "imports": { "lib/a": ["ranA"], "lib/b": ["ranB"] },
            "js": "[ranB, ranA].join()"
        })) });
        resolve(&document, options).map(|tokens| tokens.get("computed").unwrap().value.clone())
    };
    assert_eq!(order(cycle("", "")).unwrap(), json!("b1,a2"));
    // `lib/b` reads `a` before `lib/a` ran: the error is `lib/b`'s.
    let (_, path, text) = message(order(cycle("", "a;")));
    assert_eq!(path, "computed");
    assert!(text.starts_with("module `lib/b`: ReferenceError"), "{text}");
    assert!(text.contains("at line 4, column"), "{text}");
}

#[wasm_bindgen_test(unsupported = test)]
fn module_errors_name_the_module_that_threw() {
    let throwing = |a: &str, b: &str| {
        number(
            json!({ "imports": { "lib/a": ["a"] }, "js": "a" }),
            cycle(a, b),
        )
    };
    let (_, _, text) = message(throwing("throw new Error('from a');", ""));
    assert!(
        text.starts_with("module `lib/a`: Error: from a at line 4, column"),
        "{text}"
    );
    let (_, _, text) = message(throwing("", "throw new Error('from b');"));
    assert!(
        text.starts_with("module `lib/b`: Error: from b at line 4, column"),
        "{text}"
    );
    // A dependency outside the cycle runs first and names itself.
    let options = cycle("", "")
        .with_module(
            "lib/c",
            ExpressionModule::javascript("export const c = 1;\nthrow new TypeError('from c');"),
        )
        .with_module(
            "lib/b",
            ExpressionModule::javascript(
                "import { a } from 'lib/a';\nimport { c } from 'lib/c';\nexport const b = 2;",
            ),
        );
    let (_, _, text) = message(number(
        json!({ "imports": { "lib/a": ["a"] }, "js": "a" }),
        options,
    ));
    assert!(
        text.starts_with("module `lib/c`: TypeError: from c at line 2, column"),
        "{text}"
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn modules_outside_a_cycle_have_their_own_step_budget() {
    let work = |iterations: u32| format!("for (let i = 0; i < {iterations}; i++) {{}}");
    // About two interrupt polls an iteration: a budget of 1 allows 20,000.
    let options = cycle(&work(3_000), &work(3_000))
        .with_step_budget(1)
        .with_module("lib/c", ExpressionModule::javascript(format!("{}\nexport const c = 1;", work(6_000))))
        .with_module(
            "lib/b",
            ExpressionModule::javascript(format!(
                "import {{ a }} from 'lib/a';\nimport {{ c }} from 'lib/c';\n{}\nexport const b = 2;",
                work(3_000)
            )),
        );
    assert_eq!(
        number(json!({ "imports": { "lib/a": ["a"] }, "js": "a" }), options).unwrap(),
        json!(1)
    );
}
