//! `ExpressionOptions::typescript_declarations` declares the DTCG value
//! types and every native module as valid TypeScript, matching what the
//! modules export at run time.
#![cfg(feature = "expressions")]

use std::collections::{BTreeMap, BTreeSet};

use dxn_dtcg::{
    DtcgRuntime, EXPRESSION_EXTENSION, RuntimeOptions,
    expressions::{ExpressionModule, ExpressionOptions, NativeModule, expression_module},
};
use oxc::{
    allocator::Allocator,
    ast::ast::{Declaration, ExportDeclaration, TSExternalModuleDeclaration},
    ast_visit::{Visit, walk},
    parser::Parser,
    span::SourceType,
};
use serde_json::{Value, json};

pub struct Spacing;

/// Spacing helpers.
#[expression_module(specifier = "brand/spacing")]
impl Spacing {
    /// The step of a spacing scale.
    pub fn step(index: u32) -> f64 {
        4.0 * f64::from(index)
    }
}

fn options() -> ExpressionOptions {
    ExpressionOptions::default()
        .with_native_module(Spacing::expression_module())
        .with_native_module(
            NativeModule::new("brand/raw")
                .function("untyped", |value: f64| value)
                .function("described", |value: f64| value)
                .describe(
                    "described",
                    "(value: number) => number",
                    "A described function.",
                )
                .constant("ratio", 1.5)
                .constant("labels", json!(["a", "b"]))
                .with_docs("Hand-registered."),
        )
        // JavaScript modules carry their own types and are not declared.
        .with_module(
            "brand/js",
            ExpressionModule::javascript("export const x = 1;"),
        )
}

/// The exports each `declare module` declares.
fn declared(source: &str) -> BTreeMap<String, BTreeSet<String>> {
    #[derive(Default)]
    struct Modules {
        current: Option<String>,
        modules: BTreeMap<String, BTreeSet<String>>,
    }
    impl<'a> Visit<'a> for Modules {
        fn visit_ts_external_module_declaration(&mut self, it: &TSExternalModuleDeclaration<'a>) {
            self.current = Some(it.id.value.to_string());
            self.modules.entry(it.id.value.to_string()).or_default();
            walk::walk_ts_external_module_declaration(self, it);
            self.current = None;
        }
        fn visit_export_declaration(&mut self, it: &ExportDeclaration<'a>) {
            if let (Some(module), Declaration::VariableDeclaration(variables)) =
                (&self.current, &it.declaration)
            {
                for declarator in &variables.declarations {
                    if let Some(identifier) = declarator.id.get_binding_identifier() {
                        self.modules
                            .get_mut(module)
                            .unwrap()
                            .insert(identifier.name.to_string());
                    }
                }
            }
        }
    }
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::d_ts()).parse();
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mut modules = Modules::default();
    modules.visit_program(&parsed.program);
    modules.modules
}

/// The exports of `specifier` at run time.
fn exported(specifier: &str) -> BTreeSet<String> {
    let document = json!({
        "computed": { "$type": "fontFamily", "$value": ["x"], "$extensions": { EXPRESSION_EXTENSION: {
            "imports": { specifier: "module" }, "js": "Object.keys(module).sort()"
        } } }
    });
    let tokens = DtcgRuntime::from_json_with_options(
        "tokens.json",
        document.to_string(),
        RuntimeOptions::default().with_expressions(options()),
    )
    .unwrap()
    .resolve(&[])
    .unwrap()
    .ready()
    .unwrap();
    match &tokens.get("computed").unwrap().value {
        Value::Array(names) => names
            .iter()
            .map(|name| name.as_str().unwrap().to_owned())
            .collect(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_declarations_parse_as_typescript() {
    let declarations = options().typescript_declarations();
    let modules = declared(&declarations);
    assert_eq!(
        modules.keys().map(String::as_str).collect::<Vec<_>>(),
        ["brand/raw", "brand/spacing", "dxn.dtcg", "dxn.dtcg/color"]
    );
    for value_type in [
        "interface Color",
        "interface Dimension",
        "interface Duration",
        "type CubicBezier",
        "type FontFamily",
        "type FontWeight",
        "type StrokeStyle",
        "interface Border",
        "interface Transition",
        "type Shadow",
        "type Gradient",
        "interface Typography",
        "type ColorSpace",
    ] {
        assert!(declarations.contains(value_type), "{value_type}");
    }
    // Defaults alone declare the built-in modules.
    let builtins = declared(&ExpressionOptions::default().typescript_declarations());
    assert_eq!(
        builtins.keys().map(String::as_str).collect::<Vec<_>>(),
        ["dxn.dtcg", "dxn.dtcg/color"]
    );
}

#[test]
fn every_declared_export_exists_at_run_time() {
    let declarations = declared(&options().typescript_declarations());
    for (specifier, exports) in &declarations {
        assert_eq!(&exported(specifier), exports, "{specifier}");
    }
}

#[test]
fn docs_and_signatures_come_from_the_registrations() {
    let declarations = options().typescript_declarations();
    for expected in [
        "/**\n * Spacing helpers.\n */\ndeclare module \"brand/spacing\" {",
        "  /**\n   * The step of a spacing scale.\n   */\n  export const step: (index: number) => number;",
        "  export const untyped: (...args: unknown[]) => unknown;",
        "  /**\n   * A described function.\n   */\n  export const described: (value: number) => number;",
        "  export const ratio: number;",
        "  export const labels: readonly unknown[];",
        "export const px: (value: number) => Dimension;",
        "export const convert: (color: Color, space: ColorSpace) => Color;",
    ] {
        assert!(declarations.contains(expected), "missing `{expected}`");
    }
    assert!(!declarations.contains("brand/js"));
}
