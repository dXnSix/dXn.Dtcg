use dtcg_runtime::{DtcgRuntime, Resolution};

#[test]
fn aliases_observe_the_final_whole_token_overlay() {
    let root = r##"{
      "resolutionOrder":[{"$ref":"#/sets/base"},{"$ref":"#/modifiers/theme"}],
      "sets":{"base":{"sources":["base.json"]}},
      "modifiers":{"theme":{"default":"dark","contexts":{"dark":["dark.json"]}}}
    }"##;
    let mut runtime = DtcgRuntime::from_json("root.json", root).unwrap();
    runtime
        .provide_json(
            "base.json",
            r#"{
              "palette":{"primary":{"$value":"red"}},
              "button":{"background":{"$value":"{palette.primary}"}},
              "border":{"$value":{"color":"red","width":1}}
            }"#,
        )
        .unwrap();
    runtime
        .provide_json(
            "dark.json",
            r#"{
              "palette":{"primary":{"$value":"blue"}},
              "border":{"$value":{"color":"blue"}}
            }"#,
        )
        .unwrap();

    let Resolution::Ready(tokens) = runtime.resolve(&[]).unwrap() else {
        panic!("provided active graph must resolve")
    };
    assert_eq!(
        tokens.get("button.background").unwrap().value.as_str(),
        Some("blue")
    );
    assert_eq!(tokens.get("border").unwrap().value["color"], "blue");
    assert!(tokens.get("border").unwrap().value.get("width").is_none());
}
