//! Token documents declare modules at their root; resolution requests the
//! module documents expressions import through the catalog protocol.
#![cfg(feature = "expressions")]

use dxn_dtcg::{
    DocumentKind, DtcgError, DtcgRuntime, EXPRESSION_EXTENSION, Resolution, RuntimeOptions,
    expressions::{ExpressionModule, ExpressionOptions},
};
use serde_json::{Value, json};
use wasm_bindgen_test::wasm_bindgen_test;

const TINYCOLOR: &str = include_str!("fixtures/expressions/tinycolor.js");

fn options() -> RuntimeOptions {
    RuntimeOptions::default().with_expressions(ExpressionOptions::default())
}

fn computed(token_type: &str, value: Value, expression: Value) -> Value {
    json!({ "$type": token_type, "$value": value, "$extensions": { EXPRESSION_EXTENSION: expression } })
}

/// A token document declaring `modules`, with a token importing `imports`.
fn document(modules: Value, imports: Value, js: &str) -> Value {
    json!({
        "$extensions": { EXPRESSION_EXTENSION: { "modules": modules } },
        "computed": computed("number", json!(0), json!({ "imports": imports, "js": js }))
    })
}

fn runtime(id: &str, document: &Value, options: RuntimeOptions) -> DtcgRuntime {
    DtcgRuntime::from_json_with_options(id, document.to_string(), options).unwrap()
}

/// One request: the resource, who referenced it, how, and its kind.
type Request = (String, String, String, DocumentKind);

/// Resolves, providing every requested module from `modules` (by resource
/// id), until the resolution is ready; returns the requests of each round.
fn resolve_providing(
    runtime: &mut DtcgRuntime,
    modules: &[(&str, &str)],
) -> (Result<Value, DtcgError>, Vec<Vec<Request>>) {
    let mut rounds = Vec::new();
    loop {
        match runtime.resolve(&[]) {
            Ok(Resolution::Ready(tokens)) => {
                return (Ok(tokens.get("computed").unwrap().value.clone()), rounds);
            }
            Ok(Resolution::Missing(requests)) => {
                rounds.push(
                    requests
                        .iter()
                        .map(|request| {
                            (
                                request.resource_id.to_string(),
                                request.referenced_from.to_string(),
                                request.original_reference.to_string(),
                                request.kind,
                            )
                        })
                        .collect(),
                );
                for request in requests {
                    let (_, source) = modules
                        .iter()
                        .find(|(id, _)| *id == request.resource_id.as_str())
                        .expect("a known module");
                    runtime.provide_requested_module(&request, *source).unwrap();
                }
            }
            Err(error) => return (Err(error), rounds),
        }
        assert!(rounds.len() < 10, "the requests never end");
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn imported_document_modules_are_requested_until_the_import_graph_is_loaded() {
    let mut runtime = runtime(
        "themes/tokens.json",
        &document(
            json!({ "brand": "lib/brand.js" }),
            json!({ "brand": ["scaled"] }),
            "scaled(2)",
        ),
        options(),
    );
    let (value, rounds) = resolve_providing(
        &mut runtime,
        &[
            (
                "themes/lib/brand.js",
                "import { ratio } from './util/ratio.ts';\nexport const scaled = (n) => n * ratio;",
            ),
            (
                "themes/lib/util/ratio.ts",
                "export const ratio: number = 4;",
            ),
        ],
    );
    assert_eq!(value.unwrap(), json!(8));
    assert_eq!(
        rounds,
        vec![
            vec![(
                "themes/lib/brand.js".into(),
                "themes/tokens.json".into(),
                "lib/brand.js".into(),
                DocumentKind::Module
            )],
            vec![(
                "themes/lib/util/ratio.ts".into(),
                "themes/lib/brand.js".into(),
                "./util/ratio.ts".into(),
                DocumentKind::Module
            )],
        ]
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn json_documents_are_requested_as_json() {
    let resolver = json!({
        "version": "2025.10",
        "sets": { "base": { "sources": [{ "$ref": "tokens.json" }] } },
        "resolutionOrder": [{ "$ref": "#/sets/base" }]
    });
    let runtime = runtime("root.resolver.json", &resolver, options());
    let requests = runtime.resolve(&[]).unwrap().missing().unwrap().to_vec();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].kind, DocumentKind::Json);
}

#[wasm_bindgen_test(unsupported = test)]
fn a_resolution_that_imports_no_document_module_requests_none() {
    let mut document = document(
        json!({ "brand": "lib/brand.js", "unused": "lib/unused.ts" }),
        json!({}),
        "1",
    );
    document["other"] = json!({ "$type": "number", "$value": 2 });
    let runtime = runtime("tokens.json", &document, options());
    assert!(runtime.resolve(&[]).unwrap().is_ready());
    // Without expressions, declarations are not even read.
    let runtime = self::runtime("tokens.json", &document, RuntimeOptions::default());
    assert!(runtime.resolve(&[]).unwrap().is_ready());
}

#[wasm_bindgen_test(unsupported = test)]
fn module_catalog_mutations_are_atomic() {
    let mut runtime = runtime(
        "tokens.json",
        &document(
            json!({ "brand": "brand.js" }),
            json!({ "brand": ["one"] }),
            "one",
        ),
        options(),
    );
    let request = runtime.resolve(&[]).unwrap().missing().unwrap()[0].clone();
    let revision = runtime.catalog_revision();
    // A module request is not answered with JSON.
    assert!(runtime.provide_requested_json(&request, "{}").is_err());
    assert!(!runtime.contains_document("brand.js").unwrap());
    assert_eq!(runtime.catalog_revision(), revision);
    // A JSON document is not replaced by a module, nor a module by JSON.
    assert!(
        runtime
            .provide_module("tokens.json", "export const x = 1;")
            .is_err()
    );
    assert!(
        runtime
            .replace_module("brand.js", "export const one = 1;")
            .is_err()
    );
    assert!(runtime.provide_module("#bad", "").is_err());
    assert_eq!(runtime.catalog_revision(), revision);

    runtime
        .provide_requested_module(&request, "export const one = 1;")
        .unwrap();
    assert!(runtime.provide_json("brand.js", "{}").is_err());
    assert!(runtime.replace_json("brand.js", "{}").is_err());
    assert_eq!(
        runtime
            .resolve(&[])
            .unwrap()
            .ready()
            .unwrap()
            .get("computed")
            .unwrap()
            .value,
        json!(1)
    );

    runtime
        .replace_module("brand.js", "export const one = 11;")
        .unwrap();
    assert_eq!(
        runtime
            .resolve(&[])
            .unwrap()
            .ready()
            .unwrap()
            .get("computed")
            .unwrap()
            .value,
        json!(11)
    );

    runtime.remove_document("brand.js").unwrap();
    assert!(!runtime.contains_document("brand.js").unwrap());
    assert!(runtime.resolve(&[]).unwrap().is_missing());

    // A JSON request is not answered with a module.
    let resolver = json!({
        "version": "2025.10",
        "sets": { "base": { "sources": [{ "$ref": "tokens.json" }] } },
        "resolutionOrder": [{ "$ref": "#/sets/base" }]
    });
    let mut runtime = self::runtime("root.resolver.json", &resolver, options());
    let request = runtime.resolve(&[]).unwrap().missing().unwrap()[0].clone();
    assert!(
        runtime
            .provide_requested_module(&request, "export {}")
            .is_err()
    );
    assert!(!runtime.contains_document("tokens.json").unwrap());
}

#[wasm_bindgen_test(unsupported = test)]
fn a_module_where_json_is_expected_and_json_where_a_module_is_expected_are_errors() {
    let resolver = json!({
        "version": "2025.10",
        "sets": { "base": { "sources": [{ "$ref": "tokens.js" }] } },
        "resolutionOrder": [{ "$ref": "#/sets/base" }]
    });
    let mut runtime = runtime("root.resolver.json", &resolver, options());
    runtime.provide_module("tokens.js", "export {}").unwrap();
    assert!(matches!(
        runtime.resolve(&[]),
        Err(DtcgError::InvalidDocument { .. })
    ));

    let mut runtime = runtime_with_module_as_json();
    let error = runtime.resolve(&[]).unwrap_err();
    assert!(
        matches!(error, DtcgError::Expression { ref path, .. } if path == "brand"),
        "{error:?}"
    );
    runtime.remove_document("brand.js").unwrap();
}

fn runtime_with_module_as_json() -> DtcgRuntime {
    let mut runtime = runtime(
        "tokens.json",
        &document(
            json!({ "brand": "brand.js" }),
            json!({ "brand": ["one"] }),
            "one",
        ),
        options(),
    );
    runtime.provide_json("brand.js", "{}").unwrap();
    runtime
}

#[wasm_bindgen_test(unsupported = test)]
fn the_language_follows_the_extension() {
    for (id, source, value) in [
        ("brand.js", "export const one = 1;", 1),
        ("brand.mjs", "export const one = 2;", 2),
        ("brand.ts", "export const one: number = 3;", 3),
        ("brand.mts", "export const one = 4 as number;", 4),
    ] {
        let mut runtime = runtime(
            "tokens.json",
            &document(json!({ "brand": id }), json!({ "brand": ["one"] }), "one"),
            options(),
        );
        let (result, _) = resolve_providing(&mut runtime, &[(id, source)]);
        assert_eq!(result.unwrap(), json!(value), "{id}");
    }
    let cjs = runtime(
        "tokens.json",
        &document(
            json!({ "brand": "brand.cjs" }),
            json!({ "brand": ["one"] }),
            "one",
        ),
        options(),
    );
    let error = cjs.resolve(&[]).unwrap_err();
    assert!(
        matches!(error, DtcgError::Expression { ref message, .. } if message.contains("must end in `.js`, `.mjs`, `.ts` or `.mts`")),
        "{error:?}"
    );
    let mut json_import = runtime(
        "tokens.json",
        &document(
            json!({ "brand": "brand.js" }),
            json!({ "brand": ["one"] }),
            "one",
        ),
        options(),
    );
    json_import
        .provide_module("brand.js", "export { one } from './data.json';")
        .unwrap();
    let error = json_import.resolve(&[]).unwrap_err();
    assert!(
        matches!(error, DtcgError::Expression { ref message, .. } if message.contains(".json")),
        "{error:?}"
    );
}

/// The expression error the root declaration `modules` produces.
fn declaration_error(modules: Value) -> (String, String, String) {
    let document = json!({
        "$extensions": { EXPRESSION_EXTENSION: { "modules": modules } },
        "computed": computed("number", json!(0), json!({ "js": "1" }))
    });
    match runtime("themes/tokens.json", &document, options()).resolve(&[]) {
        Err(DtcgError::Expression {
            resource,
            path,
            message,
        }) => (resource.to_string(), path, message),
        other => panic!("{modules}: {other:?}"),
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn module_declarations_are_validated() {
    let (resource, _, message) = declaration_error(json!(["brand.js"]));
    assert_eq!(resource, "themes/tokens.json");
    assert!(message.contains("`modules` must be an object"), "{message}");
    for (specifier, reference, needle) in [
        ("", "brand.js", "must not be empty"),
        ("./brand", "brand.js", "bare, not relative"),
        ("../brand", "brand.js", "bare, not relative"),
        ("dxn.dtcg", "brand.js", "reserved"),
        ("dxn.dtcg/color", "brand.js", "reserved"),
        (
            "brand",
            "../../outside.js",
            "not a valid relative reference",
        ),
        ("brand", "brand.js#/fragment", "no fragment"),
        ("brand", "./brand.js", "not a valid relative reference"),
        ("brand", "", "not a valid relative reference"),
    ] {
        let mut modules = serde_json::Map::new();
        modules.insert(specifier.into(), json!(reference));
        let (resource, path, message) = declaration_error(Value::Object(modules));
        assert_eq!(
            (resource.as_str(), path.as_str()),
            ("themes/tokens.json", specifier)
        );
        assert!(
            message.contains(needle),
            "{specifier} {reference}: {message}"
        );
    }
    let (_, path, message) = declaration_error(json!({ "brand": 7 }));
    assert_eq!(path, "brand");
    assert!(message.contains("must be a string"), "{message}");
    let document = json!({
        "$extensions": { EXPRESSION_EXTENSION: { "modules": {}, "other": 1 } },
        "computed": computed("number", json!(0), json!({ "js": "1" }))
    });
    assert!(matches!(
        runtime("tokens.json", &document, options()).resolve(&[]),
        Err(DtcgError::Expression { .. })
    ));
}

#[wasm_bindgen_test(unsupported = test)]
fn references_canonicalize_like_other_document_references() {
    let mut runtime = runtime(
        "themes/dark/tokens.json",
        &document(
            json!({ "brand": "../shared/brand.js" }),
            json!({ "brand": ["one"] }),
            "one",
        ),
        options(),
    );
    let (result, rounds) = resolve_providing(
        &mut runtime,
        &[("themes/shared/brand.js", "export const one = 1;")],
    );
    assert_eq!(result.unwrap(), json!(1));
    assert_eq!(rounds[0][0].0, "themes/shared/brand.js");
}

#[wasm_bindgen_test(unsupported = test)]
fn later_sources_replace_earlier_declarations() {
    let resolver = json!({
        "version": "2025.10",
        "sets": { "base": { "sources": [{ "$ref": "first.json" }, { "$ref": "second.json" }] } },
        "resolutionOrder": [{ "$ref": "#/sets/base" }]
    });
    let mut runtime = runtime("root.resolver.json", &resolver, options());
    runtime
        .provide_json(
            "first.json",
            document(
                json!({ "brand": "first.js", "shared": "shared.js" }),
                json!({ "brand": ["one"], "shared": ["two"] }),
                "one * 10 + two",
            )
            .to_string(),
        )
        .unwrap();
    runtime
        .provide_json("second.json", json!({ "$extensions": { EXPRESSION_EXTENSION: { "modules": { "brand": "second.js" } } } }).to_string())
        .unwrap();
    let (result, rounds) = resolve_providing(
        &mut runtime,
        &[
            ("second.js", "export const one = 2;"),
            ("shared.js", "export const two = 3;"),
        ],
    );
    assert_eq!(result.unwrap(), json!(23));
    let requested = rounds
        .concat()
        .into_iter()
        .map(|request| request.0)
        .collect::<Vec<_>>();
    assert_eq!(requested, ["second.js", "shared.js"]);
}

#[wasm_bindgen_test(unsupported = test)]
fn a_specifier_registered_by_the_host_and_declared_by_a_document_is_an_error() {
    let options =
        RuntimeOptions::default().with_expressions(ExpressionOptions::default().with_module(
            "brand",
            ExpressionModule::javascript("export const one = 1;"),
        ));
    let error = runtime(
        "tokens.json",
        &document(
            json!({ "brand": "brand.js" }),
            json!({ "brand": ["one"] }),
            "one",
        ),
        options,
    )
    .resolve(&[])
    .unwrap_err();
    match error {
        DtcgError::Expression {
            resource,
            path,
            message,
        } => {
            assert_eq!((resource.as_str(), path.as_str()), ("tokens.json", "brand"));
            assert!(
                message.contains("both registered by the host and declared by a document"),
                "{message}"
            );
        }
        other => panic!("{other:?}"),
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn document_module_errors_name_the_module_and_the_specifier() {
    let mut runtime = runtime(
        "tokens.json",
        &document(
            json!({ "brand": "brand.js" }),
            json!({ "brand": ["one"] }),
            "one",
        ),
        options(),
    );
    runtime
        .provide_module("brand.js", "export const one = ;")
        .unwrap();
    match runtime.resolve(&[]).unwrap_err() {
        DtcgError::Expression {
            resource,
            path,
            message,
        } => {
            assert_eq!((resource.as_str(), path.as_str()), ("brand.js", "brand"));
            assert!(
                message.contains("SyntaxError") && message.contains("at line 1, column 20"),
                "{message}"
            );
        }
        other => panic!("{other:?}"),
    }
    runtime
        .replace_module(
            "brand.js",
            "export const one = 1;\nthrow new TypeError('broken');",
        )
        .unwrap();
    match runtime.resolve(&[]).unwrap_err() {
        DtcgError::Expression {
            resource,
            path,
            message,
        } => {
            assert_eq!((resource.as_str(), path.as_str()), ("brand.js", "brand"));
            assert!(message.contains("TypeError: broken at line 2"), "{message}");
        }
        other => panic!("{other:?}"),
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn host_modules_import_document_modules_and_relative_imports_stay_in_documents() {
    let options = RuntimeOptions::default().with_expressions(
        ExpressionOptions::default()
            .with_module(
                "host/scale",
                ExpressionModule::javascript(
                    "import { base } from 'brand';\nexport const scale = (n) => n * base;",
                ),
            )
            .with_module(
                "host/relative",
                ExpressionModule::javascript(
                    "import { base } from './brand.js';\nexport const x = base;",
                ),
            ),
    );
    let mut document = document(
        json!({ "brand": "brand.js" }),
        json!({ "host/scale": ["scale"] }),
        "scale(3)",
    );
    let mut runtime = runtime("tokens.json", &document, options.clone());
    let (result, _) = resolve_providing(&mut runtime, &[("brand.js", "export const base = 5;")]);
    assert_eq!(result.unwrap(), json!(15));

    document["computed"] = computed(
        "number",
        json!(0),
        json!({ "imports": { "host/relative": ["x"] }, "js": "x" }),
    );
    let mut runtime = self::runtime("tokens.json", &document, options);
    runtime
        .provide_module("brand.js", "export const base = 5;")
        .unwrap();
    match runtime.resolve(&[]).unwrap_err() {
        DtcgError::Expression { message, .. } => {
            assert!(
                message.contains("module `host/relative`") && message.contains("relative import"),
                "{message}"
            );
        }
        other => panic!("{other:?}"),
    }
}

fn context_resolver() -> Value {
    json!({
        "version": "2025.10",
        "sets": { "base": { "sources": [{ "$ref": "base.json" }] } },
        "modifiers": { "theme": { "contexts": {
            "light": [{ "$ref": "light.json" }],
            "dark": [{ "$ref": "dark.json" }]
        }, "default": "light" } },
        "resolutionOrder": [{ "$ref": "#/sets/base" }, { "$ref": "#/modifiers/theme" }]
    })
}

#[wasm_bindgen_test(unsupported = test)]
fn validate_complete_requests_the_modules_of_every_context() {
    let mut runtime = runtime("root.resolver.json", &context_resolver(), options());
    runtime
        .provide_json(
            "base.json",
            json!({ "base": { "$type": "number", "$value": 1 } }).to_string(),
        )
        .unwrap();
    for theme in ["light", "dark"] {
        runtime
            .provide_json(
                format!("{theme}.json"),
                document(
                    json!({ "palette": format!("{theme}.js") }),
                    json!({ "palette": ["value"] }),
                    "value",
                )
                .to_string(),
            )
            .unwrap();
    }
    let requests = runtime
        .validate_complete()
        .unwrap()
        .missing()
        .unwrap()
        .to_vec();
    let mut requested = requests
        .iter()
        .map(|request| request.resource_id.to_string())
        .collect::<Vec<_>>();
    requested.sort();
    assert_eq!(requested, ["dark.js", "light.js"]);
    assert!(
        requests
            .iter()
            .all(|request| request.kind == DocumentKind::Module)
    );
    // Resolving one context requests only its module.
    let missing = runtime
        .resolve(&[("theme", "dark")])
        .unwrap()
        .missing()
        .unwrap()
        .to_vec();
    assert_eq!(
        missing
            .iter()
            .map(|request| request.resource_id.as_str())
            .collect::<Vec<_>>(),
        ["dark.js"]
    );

    for theme in ["light", "dark"] {
        runtime
            .provide_module(
                format!("{theme}.js"),
                "import { tone } from './shared/tone.js';\nexport const value = tone;",
            )
            .unwrap();
    }
    let requests = runtime
        .validate_complete()
        .unwrap()
        .missing()
        .unwrap()
        .to_vec();
    assert_eq!(
        requests
            .iter()
            .map(|request| request.resource_id.as_str())
            .collect::<Vec<_>>(),
        ["shared/tone.js"]
    );
    runtime
        .provide_requested_module(&requests[0], "export const tone = 7;")
        .unwrap();
    assert!(runtime.validate_complete().unwrap().is_ready());
    // Without expressions, module declarations are opaque.
    let mut plain = self::runtime(
        "root.resolver.json",
        &context_resolver(),
        RuntimeOptions::default(),
    );
    plain.provide_json("base.json", "{}").unwrap();
    plain.provide_json("light.json", "{}").unwrap();
    plain
        .provide_json(
            "dark.json",
            document(json!({ "palette": "dark.js" }), json!({}), "1").to_string(),
        )
        .unwrap();
    assert!(plain.validate_complete().unwrap().is_ready());
}

#[wasm_bindgen_test(unsupported = test)]
fn previews_request_document_modules() {
    let mut runtime = runtime("root.resolver.json", &context_resolver(), options());
    runtime
        .provide_json(
            "base.json",
            json!({ "base": { "$type": "number", "$value": 1 } }).to_string(),
        )
        .unwrap();
    runtime.provide_json("light.json", "{}").unwrap();
    runtime.provide_json("dark.json", "{}").unwrap();
    let draft = json!({ "computed": computed("number", json!(0), json!({ "imports": { "palette": ["value"] }, "js": "value" })) });
    // The draft imports a specifier that the base set's document declares.
    runtime
        .replace_json("base.json", json!({ "$extensions": { EXPRESSION_EXTENSION: { "modules": { "palette": "palette.js" } } }, "base": { "$type": "number", "$value": 1 } }).to_string())
        .unwrap();
    let missing = runtime
        .preview_source(&draft, &[])
        .unwrap()
        .missing()
        .unwrap()
        .to_vec();
    assert_eq!(missing[0].resource_id.as_str(), "palette.js");
    assert_eq!(missing[0].kind, DocumentKind::Module);
    runtime
        .provide_requested_module(&missing[0], "export const value = 9;")
        .unwrap();
    let preview = runtime
        .preview_source(&draft, &[])
        .unwrap()
        .ready()
        .unwrap();
    assert_eq!(preview.get("computed").unwrap().value, json!(9));
    let occurrences = runtime
        .inspect_source(&draft, &[])
        .unwrap()
        .ready()
        .unwrap();
    let preview = runtime
        .preview_occurrence(&occurrences[0], &[])
        .unwrap()
        .ready()
        .unwrap();
    assert_eq!(preview.get("computed").unwrap().value, json!(9));
}

#[wasm_bindgen_test(unsupported = test)]
fn a_color_library_works_unmodified_as_a_document_module() {
    let accent = json!({ "colorSpace": "srgb", "components": [0.2, 0.4, 0.9] });
    let document = json!({
        "$extensions": { EXPRESSION_EXTENSION: { "modules": { "tinycolor": "vendor/tinycolor.js" } } },
        "accent": { "$type": "color", "$value": accent },
        "computed": computed("color", accent, json!({
            "imports": { "tinycolor": ["mix"] },
            "inputs": { "accent": "{accent}" },
            "js": "mix(accent, { colorSpace: 'srgb', components: [1, 1, 1] }, 0.5)"
        }))
    });
    let mut runtime = runtime("tokens.json", &document, options());
    let (result, rounds) = resolve_providing(&mut runtime, &[("vendor/tinycolor.js", TINYCOLOR)]);
    assert_eq!(
        result.unwrap(),
        json!({ "colorSpace": "srgb", "components": [0.6, 0.7, 0.95] })
    );
    assert_eq!(rounds.len(), 1);
}

#[wasm_bindgen_test(unsupported = test)]
fn a_host_module_nothing_imports_requests_no_document_module() {
    let options = RuntimeOptions::default().with_expressions(
        ExpressionOptions::default()
            .with_module(
                "host/unused",
                ExpressionModule::javascript(
                    "import { base } from 'brand';\nexport const x = base;",
                ),
            )
            .with_module(
                "host/used",
                ExpressionModule::javascript("export const y = 1;"),
            ),
    );
    let runtime = runtime(
        "tokens.json",
        &document(
            json!({ "brand": "brand.js" }),
            json!({ "host/used": ["y"] }),
            "y",
        ),
        options,
    );
    let tokens = runtime.resolve(&[]).unwrap().ready().unwrap();
    assert_eq!(tokens.get("computed").unwrap().value, json!(1));
}

#[wasm_bindgen_test(unsupported = test)]
fn document_cycles_name_the_module_that_threw() {
    let mut runtime = runtime(
        "tokens.json",
        &document(json!({ "first": "a.js" }), json!({ "first": ["a"] }), "a"),
        options(),
    );
    runtime
        .provide_module(
            "a.js",
            "import { b } from './b.js';\nexport const a = 1;\nthrow new RangeError('from a');",
        )
        .unwrap();
    runtime
        .provide_module(
            "b.js",
            "import { a } from './a.js';\nimport { c } from './c.js';\nexport const b = 2;",
        )
        .unwrap();
    runtime
        .provide_module("c.js", "export const c = 3;")
        .unwrap();
    match runtime.resolve(&[]).unwrap_err() {
        DtcgError::Expression {
            resource,
            path,
            message,
        } => {
            assert_eq!((resource.as_str(), path.as_str()), ("a.js", "first"));
            assert!(
                message.starts_with("RangeError: from a at line 3, column"),
                "{message}"
            );
        }
        other => panic!("{other:?}"),
    }
    runtime
        .replace_module("a.js", "import { b } from './b.js';\nexport const a = 1;")
        .unwrap();
    runtime
        .replace_module(
            "c.js",
            "export const c = 3;\nthrow new TypeError('from c');",
        )
        .unwrap();
    match runtime.resolve(&[]).unwrap_err() {
        DtcgError::Expression {
            resource,
            path,
            message,
        } => {
            assert_eq!((resource.as_str(), path.as_str()), ("c.js", "./c.js"));
            assert!(
                message.starts_with("TypeError: from c at line 2, column"),
                "{message}"
            );
        }
        other => panic!("{other:?}"),
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn module_documents_are_bounded_by_max_documents_as_their_own_count() {
    let modules = [
        (
            "a.js",
            "import { b } from './b.js'; export const a = b + 1;",
        ),
        (
            "b.js",
            "import { c } from './c.js'; export const b = c + 1;",
        ),
        ("c.js", "export const c = 1;"),
    ];
    let resolve = |max_documents: usize| {
        let options = RuntimeOptions::default()
            .with_limits(dxn_dtcg::RuntimeLimits::default().with_max_documents(max_documents))
            .with_expressions(ExpressionOptions::default());
        let mut runtime = runtime(
            "tokens.json",
            &document(json!({ "chain": "a.js" }), json!({ "chain": ["a"] }), "a"),
            options,
        );
        resolve_providing(&mut runtime, &modules).0
    };
    // One token document and three module documents: the module count is
    // separate from the source count, so a limit of 3 is enough.
    assert_eq!(resolve(3).unwrap(), json!(3));
    let error = resolve(2).unwrap_err();
    assert!(
        matches!(
            error,
            DtcgError::LimitExceeded {
                kind: "documents",
                limit: 2
            }
        ),
        "{error:?}"
    );
}
