use dtcg_runtime::{DtcgRuntime, Resolution};
use serde_json::json;

#[test]
fn aliases_observe_the_final_whole_token_overlay() {
    let root = r##"{
      "version":"2025.10",
      "resolutionOrder":[{"$ref":"#/sets/base"},{"$ref":"#/modifiers/theme"}],
      "sets":{"base":{"sources":[{"$ref":"base.json"}]}},
      "modifiers":{"theme":{"default":"dark","contexts":{"dark":[{"$ref":"dark.json"}],"light":[]}}}
    }"##;
    let mut runtime = DtcgRuntime::from_json("root.json", root).unwrap();
    runtime
        .provide_json(
            "base.json",
            r##"{
              "palette":{"primary":{"$type":"color","$value":{"colorSpace":"srgb","components":[1,0,0]}}},
              "button":{"background":{"$value":"{palette.primary}"}},
              "border":{"$type":"color","$value":{"colorSpace":"srgb","components":[1,0,0],"hex":"#FF0000"}}
            }"##,
        )
        .unwrap();
    runtime
        .provide_json(
            "dark.json",
            r#"{
              "palette":{"primary":{"$type":"color","$value":{"colorSpace":"srgb","components":[0,0,1]}}},
              "border":{"$type":"color","$value":{"colorSpace":"srgb","components":[0,0,1]}}
            }"#,
        )
        .unwrap();

    let Resolution::Ready(tokens) = runtime.resolve(&[]).unwrap() else {
        panic!("provided active graph must resolve")
    };
    assert_eq!(
        tokens.get("button.background").unwrap().value["components"],
        json!([0, 0, 1])
    );
    assert_eq!(
        tokens.get("border").unwrap().value["components"],
        json!([0, 0, 1])
    );
    assert!(tokens.get("border").unwrap().value.get("hex").is_none());
}
