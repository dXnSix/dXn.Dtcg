use dtcg_runtime::{DocumentCatalog, DtcgRuntime, DtcgTokens, Resolution};

#[derive(Debug, DtcgTokens, PartialEq)]
struct AppTheme {
    #[dtcg(path = "color.button")]
    button: String,
}

const ROOT: &str = r##"{"sets":{"base":{"sources":[{"$ref":"base.json"}]}},"modifiers":{"theme":{"default":"light","contexts":{"light":[],"dark":[{"$ref":"dark.json"}]}}},"resolutionOrder":[{"$ref":"#/sets/base"},{"$ref":"#/modifiers/theme"}]}"##;
const BASE: &str = r#"{"spacing":{"sm":{"$type":"dimension","$value":4}},"color":{"button":{"$type":"color","$value":"blue"}},"component":{"fill":{"$value":"{color.button}"},"gap":{"$value":"{spacing.sm}"}}}"#;
const DARK: &str = r#"{"color":{"button":{"$type":"color","$value":"black"}}}"#;

#[test]
fn resolves_after_missing_documents_are_provided_and_overlays_whole_tokens() {
    let catalog = DocumentCatalog::new();
    assert!(catalog.is_empty());
    assert_eq!(catalog.len(), 0);
    let mut runtime = DtcgRuntime::from_json("resolver.json", ROOT).unwrap();
    assert!(
        matches!(runtime.resolve(&[]).unwrap(), Resolution::Missing(requests) if requests[0].resource_id.as_str() == "base.json")
    );
    runtime.provide_json("base.json", BASE).unwrap();
    let light = runtime.resolve(&[]).unwrap().ready().unwrap();
    assert_eq!(light.get("component.gap").unwrap().value, 4);
    assert_eq!(light.get("component.fill").unwrap().value, "blue");
    assert!(
        matches!(runtime.resolve(&[("theme", "dark")]).unwrap(), Resolution::Missing(requests) if requests[0].resource_id.as_str() == "dark.json")
    );
    runtime.provide_json("dark.json", DARK).unwrap();
    let dark = runtime
        .resolve(&[("theme", "dark")])
        .unwrap()
        .ready()
        .unwrap();
    assert_eq!(dark.get("component.fill").unwrap().value, "black");
    assert_eq!(
        runtime
            .project::<AppTheme>(&[("theme", "dark")])
            .unwrap()
            .ready(),
        Some(AppTheme {
            button: "black".into()
        })
    );
}
