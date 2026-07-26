use dtcg_runtime::{DtcgRuntime, Resolution};

#[test]
fn groups_root_extensions_and_effective_types_follow_dtcg_2025_10() {
    let tokens = r#"{
      "base": {
        "$type": "color",
        "$root": { "$value": "red" },
        "accent": { "$value": "green" }
      },
      "theme": {
        "$extends": "{base}",
        "accent": { "$value": "blue" }
      }
    }"#;
    let runtime = DtcgRuntime::from_json("tokens.json", tokens).unwrap();
    let Resolution::Ready(tokens) = runtime.resolve(&[]).unwrap() else {
        panic!("the complete in-memory document must resolve")
    };

    let root = tokens.get("base").unwrap();
    assert_eq!(root.token_type.as_deref(), Some("color"));
    assert_eq!(root.value.as_str(), Some("red"));
    assert_eq!(tokens.get("theme").unwrap().value.as_str(), Some("red"));
    assert_eq!(
        tokens.get("theme.accent").unwrap().value.as_str(),
        Some("blue")
    );
    assert_eq!(
        tokens.get("theme.accent").unwrap().token_type.as_deref(),
        Some("color")
    );
}

#[test]
fn external_fragment_sources_preserve_canonical_provenance() {
    let root = r##"{
      "resolutionOrder":[{"$ref":"#/sets/base"}],
      "sets":{"base":{"sources":["library.json#/palette"]}}
    }"##;
    let mut runtime = DtcgRuntime::from_json("root.json", root).unwrap();
    runtime
        .provide_json(
            "library.json",
            r#"{"palette":{"primary":{"$value":"red"}}}"#,
        )
        .unwrap();

    let Resolution::Ready(tokens) = runtime.resolve(&[]).unwrap() else {
        panic!("provided fragment source must resolve")
    };
    let token = tokens.get("primary").unwrap();
    assert_eq!(token.value.as_str(), Some("red"));
    assert_eq!(token.source.as_str(), "library.json");
}
