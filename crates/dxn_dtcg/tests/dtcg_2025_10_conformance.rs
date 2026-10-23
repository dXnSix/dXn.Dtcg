use dxn_dtcg::{DtcgError, DtcgRuntime, Resolution, validate_value};
use serde_json::json;

#[test]
fn groups_root_extensions_and_effective_types_follow_dtcg_2025_10() {
    let tokens = r#"{
      "base": {
        "$type": "color",
        "$root": { "$value": {"colorSpace":"srgb","components":[1,0,0]} },
        "accent": { "$value": {"colorSpace":"srgb","components":[0,1,0]} }
      },
      "theme": {
        "$extends": "{base}",
        "accent": { "$value": {"colorSpace":"srgb","components":[0,0,1]} }
      }
    }"#;
    let runtime = DtcgRuntime::from_json("tokens.json", tokens).unwrap();
    let Resolution::Ready(tokens) = runtime.resolve(&[]).unwrap() else {
        panic!("the complete in-memory document must resolve")
    };

    let root = tokens.get("base.$root").unwrap();
    assert_eq!(root.token_type.as_deref(), Some("color"));
    assert_eq!(root.value["components"], json!([1, 0, 0]));
    assert_eq!(
        tokens.get("theme.$root").unwrap().value["components"],
        json!([1, 0, 0])
    );
    assert_eq!(
        tokens.get("theme.accent").unwrap().value["components"],
        json!([0, 0, 1])
    );
    assert_eq!(
        tokens.get("theme.accent").unwrap().token_type.as_deref(),
        Some("color")
    );
}

#[test]
fn external_fragment_sources_preserve_canonical_provenance() {
    let root = r##"{
      "version":"2025.10",
      "resolutionOrder":[{"$ref":"#/sets/base"}],
      "sets":{"base":{"sources":[{"$ref":"library.json#/palette"}]}}
    }"##;
    let mut runtime = DtcgRuntime::from_json("root.json", root).unwrap();
    runtime
        .provide_json(
            "library.json",
            r#"{"palette":{"primary":{"$type":"number","$value":1}}}"#,
        )
        .unwrap();

    let Resolution::Ready(tokens) = runtime.resolve(&[]).unwrap() else {
        panic!("provided fragment source must resolve")
    };
    let token = tokens.get("primary").unwrap();
    assert_eq!(token.value, 1);
    assert_eq!(token.source.as_str(), "library.json");
}

#[test]
fn validates_all_final_types_after_resolution() {
    let color = json!({"colorSpace":"srgb","components":[0.1,0.2,0.3],"alpha":0.5,"hex":"#1A334D"});
    let dimension = json!({"value":-2.5,"unit":"px"});
    let duration = json!({"value":-100,"unit":"ms"});
    let valid = [
        ("color", color.clone()),
        ("dimension", dimension.clone()),
        ("fontFamily", json!(["Inter", "sans-serif"])),
        ("fontWeight", json!("semi-bold")),
        ("duration", duration.clone()),
        ("cubicBezier", json!([0.2, -4, 0.8, 5])),
        ("number", json!(-1.5)),
        (
            "strokeStyle",
            json!({"dashArray":[dimension.clone()],"lineCap":"round"}),
        ),
        (
            "border",
            json!({"color":color.clone(),"width":dimension.clone(),"style":"solid"}),
        ),
        (
            "transition",
            json!({"duration":duration.clone(),"delay":{"value":0,"unit":"s"},"timingFunction":[0,0,1,1]}),
        ),
        (
            "shadow",
            json!({"color":color.clone(),"offsetX":dimension.clone(),"offsetY":dimension.clone(),"blur":dimension.clone(),"spread":dimension.clone(),"inset":true}),
        ),
        (
            "gradient",
            json!([{"color":color.clone(),"position":-99},{"color":color.clone(),"position":42}]),
        ),
        (
            "typography",
            json!({"fontFamily":"Inter","fontSize":dimension.clone(),"fontWeight":400,"letterSpacing":dimension,"lineHeight":1.5}),
        ),
    ];

    for (token_type, value) in valid {
        validate_value(token_type, &value)
            .unwrap_or_else(|error| panic!("{token_type} should be valid: {error}"));
    }

    let invalid = [
        ("color", json!("#fff")),
        ("dimension", json!({"value":1,"unit":"em"})),
        ("fontFamily", json!(["Inter", 1])),
        ("fontWeight", json!(0)),
        ("duration", json!({"value":1,"unit":"frames"})),
        ("cubicBezier", json!([2, 0, 1, 1])),
        ("number", json!("1")),
        ("strokeStyle", json!("wavy")),
        ("border", json!({"width":{"value":1,"unit":"px"}})),
        ("transition", json!({"duration":{"value":1,"unit":"ms"}})),
        ("shadow", json!([])),
        ("gradient", json!([])),
        ("typography", json!({"fontFamily":"Inter"})),
    ];
    for (token_type, value) in invalid {
        assert!(
            validate_value(token_type, &value).is_err(),
            "{token_type} must reject {value}"
        );
    }
    assert!(validate_value("string", &json!("legacy")).is_err());
    assert!(validate_value("fontFamily", &json!([])).is_err());
}

#[test]
fn aliases_to_explicit_root_paths_inherit_the_target_type() {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r#"{
          "spacing":{"$type":"dimension","$root":{"$value":{"value":8,"unit":"px"}}},
          "alias":{"$value":"{spacing.$root}"}
        }"#,
    )
    .unwrap();
    let tokens = runtime.resolve(&[]).unwrap().ready().unwrap();

    assert_eq!(
        tokens.get("spacing.$root").unwrap().path.as_ref(),
        "spacing.$root"
    );
    assert_eq!(
        tokens.get("alias").unwrap().token_type.as_deref(),
        Some("dimension")
    );
    assert_eq!(
        tokens.get("alias").unwrap().value,
        json!({"value":8,"unit":"px"})
    );

    let invalid = DtcgRuntime::from_json(
        "tokens.json",
        r#"{
          "spacing":{"$type":"dimension","$root":{"$value":{"value":8,"unit":"px"}}},
          "alias":{"$value":"{spacing}"}
        }"#,
    )
    .unwrap();
    assert!(invalid.resolve(&[]).is_err());
}

#[test]
fn rejects_invalid_names_and_fingerprints_include_the_resolved_type() {
    for invalid_name in ["bad.name", "bad{name", "bad}name", "$bad"] {
        let document = format!(r#"{{"{invalid_name}":{{"$type":"number","$value":1}}}}"#);
        assert!(
            DtcgRuntime::from_json("tokens.json", document)
                .unwrap()
                .resolve(&[])
                .is_err()
        );
    }

    let number = DtcgRuntime::from_json(
        "tokens.json",
        r#"{"value":{"$type":"number","$value":400}}"#,
    )
    .unwrap()
    .resolve(&[])
    .unwrap()
    .ready()
    .unwrap();
    let weight = DtcgRuntime::from_json(
        "tokens.json",
        r#"{"value":{"$type":"fontWeight","$value":400}}"#,
    )
    .unwrap()
    .resolve(&[])
    .unwrap()
    .ready()
    .unwrap();
    assert_ne!(number.fingerprint(), weight.fingerprint());
}

#[test]
fn resolves_same_document_json_pointer_values_and_properties() {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "baseNumber":{"$type":"number","$value":0.25},
          "numberAlias":{"$value":{"$ref":"#/baseNumber/$value"}},
          "baseColor":{"$type":"color","$value":{"colorSpace":"srgb","components":[0.25,0.5,0.75]}},
          "colorAlias":{"$type":"color","$value":{
            "colorSpace":"srgb",
            "components":[
              {"$ref":"#/baseColor/$value/components/0"},
              {"$ref":"#/baseColor/$value/components/1"},
              0
            ]
          }}
        }"##,
    )
    .unwrap();
    let tokens = runtime.resolve(&[]).unwrap().ready().unwrap();

    assert_eq!(
        tokens.get("numberAlias").unwrap().token_type.as_deref(),
        Some("number")
    );
    assert_eq!(tokens.get("numberAlias").unwrap().value, 0.25);
    assert_eq!(
        tokens.get("colorAlias").unwrap().value["components"],
        json!([0.25, 0.5, 0])
    );

    for reference in [
        "#/baseColor",
        "#/missing/$value",
        "#/baseColor/$value/missing",
    ] {
        let document = format!(
            r#"{{"baseColor":{{"$type":"color","$value":{{"colorSpace":"srgb","components":[0,0,0]}}}},"invalid":{{"$type":"color","$value":{{"$ref":"{reference}"}}}}}}"#
        );
        assert!(
            DtcgRuntime::from_json("tokens.json", document)
                .unwrap()
                .resolve(&[])
                .is_err(),
            "{reference} must be rejected"
        );
    }
}

#[test]
fn token_level_json_pointer_aliases_resolve_with_exact_failures_and_cycles() {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "base":{"$type":"number","$value":4},
          "alias":{"$ref":"#/base/$value"}
        }"##,
    )
    .unwrap();
    let tokens = runtime.resolve(&[]).unwrap().ready().unwrap();
    let alias = tokens.get("alias").unwrap();
    assert_eq!(alias.value, 4);
    assert_eq!(alias.token_type.as_deref(), Some("number"));
    assert_eq!(alias.authored_value, json!({"$ref":"#/base/$value"}));

    let shorthand = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "base":{"$type":"number","$value":4},
          "alias":{"$type":"number","$ref":"#/base"}
        }"##,
    )
    .unwrap();
    assert!(
        shorthand.resolve(&[]).is_err(),
        "a token-object pointer must not receive an implicit `/$value` suffix"
    );

    for target in ["#/missing", "#/group"] {
        let document = format!(
            r##"{{"group":{{"child":{{"$type":"number","$value":1}}}},"alias":{{"$type":"number","$ref":"{target}"}}}}"##
        );
        assert!(
            DtcgRuntime::from_json("tokens.json", document)
                .unwrap()
                .resolve(&[])
                .is_err(),
            "{target} must not receive an implicit `/$value` suffix"
        );
    }

    let cycle = DtcgRuntime::from_json(
        "tokens.json",
        r##"{"a":{"$ref":"#/b/$value"},"b":{"$ref":"#/a/$value"}}"##,
    )
    .unwrap()
    .resolve(&[])
    .unwrap_err();
    match cycle {
        DtcgError::Token {
            resource,
            path,
            message,
        } => {
            assert_eq!(resource.as_str(), "tokens.json");
            assert_eq!(path, "a");
            assert_eq!(message, "alias cycle: a -> b -> a");
        }
        other => panic!("expected an alias-cycle error, got {other:?}"),
    }
}

#[test]
fn json_pointers_reach_arbitrary_locations_and_unescape_segments() {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "$extensions":{"org.example":{"a/b~c":7}},
          "answer":{"$type":"number","$ref":"#/$extensions/org.example/a~1b~0c"}
        }"##,
    )
    .unwrap();
    let tokens = runtime.resolve(&[]).unwrap().ready().unwrap();

    let answer = tokens.get("answer").unwrap();
    assert_eq!(answer.value, 7);
    assert_eq!(
        answer.authored_value,
        json!({"$type":"number","$ref":"#/$extensions/org.example/a~1b~0c"})
    );

    let missing = DtcgRuntime::from_json(
        "tokens.json",
        r##"{"answer":{"$type":"number","$ref":"#/$extensions/missing"}}"##,
    )
    .unwrap()
    .resolve(&[])
    .unwrap_err();
    assert!(
        matches!(missing, DtcgError::InvalidPointer(ref pointer) if pointer == "#/$extensions/missing")
    );

    let cycle = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "$extensions":{"a":{"$ref":"#/$extensions/b"},"b":{"$ref":"#/$extensions/a"}},
          "answer":{"$type":"number","$ref":"#/$extensions/a"}
        }"##,
    )
    .unwrap()
    .resolve(&[])
    .unwrap_err();
    assert!(
        matches!(cycle, DtcgError::InvalidPointer(ref message) if message.contains("circular JSON Pointer"))
    );
}

#[test]
fn group_extends_accepts_json_pointers_and_reports_exact_errors() {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "base":{"$type":"number","value":{"$value":1}},
          "derived":{"$extends":"#/base","extra":{"$value":2}}
        }"##,
    )
    .unwrap();
    let tokens = runtime.resolve(&[]).unwrap().ready().unwrap();
    assert_eq!(tokens.get("derived.value").unwrap().value, 1);
    assert_eq!(tokens.get("derived.extra").unwrap().value, 2);
    assert_eq!(
        tokens.get("derived.extra").unwrap().token_type.as_deref(),
        Some("number")
    );

    let missing = DtcgRuntime::from_json(
        "tokens.json",
        r##"{"derived":{"$extends":"#/missing","value":{"$type":"number","$value":1}}}"##,
    )
    .unwrap()
    .resolve(&[])
    .unwrap_err();
    match missing {
        DtcgError::UnresolvedReference {
            path,
            target,
            message,
            ..
        } => {
            assert_eq!(path, "derived");
            assert_eq!(target, "#/missing");
            assert_eq!(message, "extended group was not found");
        }
        other => panic!("expected a missing-extension error, got {other:?}"),
    }

    let cycle = DtcgRuntime::from_json(
        "tokens.json",
        r##"{"a":{"$extends":"#/b"},"b":{"$extends":"#/a"}}"##,
    )
    .unwrap()
    .resolve(&[])
    .unwrap_err();
    match cycle {
        DtcgError::Token {
            resource,
            path,
            message,
        } => {
            assert_eq!(resource.as_str(), "tokens.json");
            assert_eq!(path, "b");
            assert_eq!(message, "group extension cycle: b -> a -> b");
        }
        other => panic!("expected a group-extension cycle, got {other:?}"),
    }
}

#[test]
fn resolved_iteration_preserves_final_authored_order_and_raw_aliases() {
    let resolver = r##"{
      "version":"2025.10",
      "sets":{"tokens":{"sources":[
        {
          "second":{"$type":"number","$value":2},
          "first":{"$type":"number","$value":1},
          "alias":{"$value":"{first}"}
        },
        {"second":{"$type":"number","$value":20}}
      ]}},
      "resolutionOrder":[{"$ref":"#/sets/tokens"}]
    }"##;
    let tokens = DtcgRuntime::from_json("tokens.resolver.json", resolver)
        .unwrap()
        .resolve(&[])
        .unwrap()
        .ready()
        .unwrap();

    assert_eq!(
        tokens.iter().map(|token| &*token.path).collect::<Vec<_>>(),
        ["first", "alias", "second"]
    );
    assert_eq!(tokens.get("alias").unwrap().authored_value, "{first}");
    assert_eq!(tokens.get("alias").unwrap().value, 1);

    let reversed = DtcgRuntime::from_json(
        "tokens.json",
        r#"{"second":{"$type":"number","$value":20},"alias":{"$value":"{first}"},"first":{"$type":"number","$value":1}}"#,
    )
    .unwrap()
    .resolve(&[])
    .unwrap()
    .ready()
    .unwrap();
    assert_ne!(tokens.fingerprint(), reversed.fingerprint());
}
