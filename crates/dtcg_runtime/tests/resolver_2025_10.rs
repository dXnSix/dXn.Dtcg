use dtcg_runtime::{DtcgRuntime, Resolution};

#[test]
fn resolver_requires_the_final_version() {
    for resolver in [
        r#"{"sets":{},"resolutionOrder":[]}"#,
        r#"{"version":"2026.01","sets":{},"resolutionOrder":[]}"#,
    ] {
        let runtime = DtcgRuntime::from_json("tokens.resolver.json", resolver).unwrap();
        assert!(runtime.resolve(&[]).is_err());
    }
}

#[test]
fn resolver_validates_modifier_inputs_and_accepts_inline_sources() {
    let resolver = r##"{
      "version":"2025.10",
      "modifiers":{
        "theme":{"contexts":{
          "light":[{"color":{"$type":"fontFamily","$value":"white"}}],
          "dark":[{"color":{"$type":"fontFamily","$value":"black"}}]
        }}
      },
      "resolutionOrder":[{"$ref":"#/modifiers/theme"}]
    }"##;
    let runtime = DtcgRuntime::from_json("tokens.resolver.json", resolver).unwrap();

    assert!(runtime.resolve(&[]).is_err());
    let error = runtime.resolve(&[("theme", "blue")]).unwrap_err();
    assert!(matches!(
        error,
        dtcg_runtime::DtcgError::Token { ref path, ref message, .. }
            if path == "#/modifiers/theme" && message == "selected modifier context is unknown"
    ));
    assert!(runtime.resolve(&[("unknown", "dark")]).is_err());
    assert!(
        runtime
            .resolve(&[("theme", "dark"), ("theme", "light")])
            .is_err()
    );

    let resolved = runtime
        .resolve(&[("theme", "dark")])
        .unwrap()
        .ready()
        .unwrap();
    assert_eq!(resolved.get("color").unwrap().value, "black");
}

#[test]
fn inline_sets_and_local_reference_overrides_respect_array_order() {
    let resolver = r##"{
      "version":"2025.10",
      "templates":{
        "tokens":{"value":{"$type":"number","$value":1}}
      },
      "resolutionOrder":[
        {
          "type":"set",
          "name":"foundation",
          "sources":[
            {"$ref":"#/templates/tokens"},
            {"$ref":"#/templates/tokens","value":{"$type":"number","$value":2}}
          ]
        },
        {
          "type":"set",
          "name":"final",
          "sources":[{"value":{"$type":"number","$value":3}}]
        }
      ]
    }"##;
    let runtime = DtcgRuntime::from_json("tokens.resolver.json", resolver).unwrap();
    let resolved = runtime.resolve(&[]).unwrap().ready().unwrap();

    assert_eq!(resolved.get("value").unwrap().value, 3);
}

#[test]
fn an_overriding_set_moves_the_token_to_its_last_definition_in_resolution_order() {
    let resolver = r##"{
      "version":"2025.10",
      "resolutionOrder":[
        {
          "type":"set",
          "name":"foundation",
          "sources":[{"a":{"$type":"number","$value":1},"b":{"$type":"number","$value":2}}]
        },
        {
          "type":"set",
          "name":"brand",
          "sources":[{"a":{"$type":"number","$value":3}}]
        }
      ]
    }"##;
    let runtime = DtcgRuntime::from_json("tokens.resolver.json", resolver).unwrap();
    let resolved = runtime.resolve(&[]).unwrap().ready().unwrap();

    let order = resolved
        .iter()
        .map(|token| (token.path.as_ref(), token.value.clone()))
        .collect::<Vec<_>>();
    assert_eq!(order, vec![("b", 2.into()), ("a", 3.into())]);
    assert_eq!(resolved.len(), 2);
}

#[test]
fn modifier_contexts_may_reference_sets() {
    let resolver = r##"{
      "version":"2025.10",
      "sets":{
        "base":{"sources":[{"value":{"$type":"number","$value":7}}]}
      },
      "modifiers":{
        "mode":{"default":"on","contexts":{"on":[{"$ref":"#/sets/base"}],"off":[]}}
      },
      "resolutionOrder":[{"$ref":"#/modifiers/mode"}]
    }"##;
    let runtime = DtcgRuntime::from_json("tokens.resolver.json", resolver).unwrap();
    let resolved = runtime.resolve(&[]).unwrap().ready().unwrap();

    assert_eq!(resolved.get("value").unwrap().value, 7);
}

#[test]
fn duplicate_inline_names_and_forbidden_references_are_rejected() {
    let duplicate = r#"{
      "version":"2025.10",
      "resolutionOrder":[
        {"type":"set","name":"same","sources":[]},
        {"type":"modifier","name":"same","default":"on","contexts":{"on":[],"off":[]}}
      ]
    }"#;
    let runtime = DtcgRuntime::from_json("tokens.resolver.json", duplicate).unwrap();
    assert!(runtime.resolve(&[]).is_err());

    let forbidden = r##"{
      "version":"2025.10",
      "sets":{"bad":{"sources":[{"$ref":"#/resolutionOrder/0"}]}},
      "resolutionOrder":[{"$ref":"#/sets/bad"}]
    }"##;
    let runtime = DtcgRuntime::from_json("tokens.resolver.json", forbidden).unwrap();
    assert!(runtime.resolve(&[]).is_err());
}

#[test]
fn external_reference_objects_apply_shallow_local_overrides() {
    let resolver = r##"{
      "version":"2025.10",
      "sets":{"base":{"sources":[{
        "$ref":"library.json#/tokens",
        "value":{"$type":"number","$value":2}
      }]}},
      "resolutionOrder":[{"$ref":"#/sets/base"}]
    }"##;
    let mut runtime = DtcgRuntime::from_json("tokens.resolver.json", resolver).unwrap();
    runtime
        .provide_json(
            "library.json",
            r#"{"tokens":{"value":{"$type":"number","$value":1},"kept":{"$type":"number","$value":4}}}"#,
        )
        .unwrap();

    let Resolution::Ready(resolved) = runtime.resolve(&[]).unwrap() else {
        panic!("the external source was provided")
    };
    assert_eq!(resolved.get("value").unwrap().value, 2);
    assert_eq!(resolved.get("kept").unwrap().value, 4);
}
