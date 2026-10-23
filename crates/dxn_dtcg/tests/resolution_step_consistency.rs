use dxn_dtcg::{DtcgError, DtcgRuntime};
use serde_json::{Value, json};

fn number(value: i64) -> Value {
    json!({"value": {"$type": "number", "$value": value}})
}

fn theme(root_default: Option<&str>) -> Value {
    let mut modifier = json!({"contexts": {"light": [number(1)], "dark": [number(2)]}});
    if let Some(default) = root_default {
        modifier["default"] = json!(default);
    }
    json!({
        "version": "2025.10",
        "modifiers": {"theme": modifier},
        "resolutionOrder": [{"$ref": "#/modifiers/theme", "default": "dark"}]
    })
}

#[test]
fn a_ref_step_override_defines_the_modifier_for_every_api() {
    for root_default in [None, Some("light")] {
        let runtime = DtcgRuntime::from_json("root.json", theme(root_default).to_string()).unwrap();
        let tokens = runtime.resolve(&[]).unwrap().ready().unwrap();
        assert_eq!(
            tokens.get("value").unwrap().value,
            json!(2),
            "{root_default:?}"
        );
        assert_eq!(
            runtime.normalize_selection(&[]).unwrap().ready().unwrap(),
            [("theme".to_owned(), "dark".to_owned())]
        );
        let axes = runtime.modifiers().unwrap().ready().unwrap();
        assert_eq!(axes.len(), 1);
        assert_eq!(axes[0].default.as_deref(), Some("dark"));
        assert!(runtime.validate_complete().unwrap().is_ready());
        let light = runtime
            .resolve(&[("theme", "light")])
            .unwrap()
            .ready()
            .unwrap();
        assert_eq!(light.get("value").unwrap().value, json!(1));
        let inspected = runtime
            .inspect_source(&json!({"value": {"$type": "number", "$value": 9}}), &[])
            .unwrap();
        assert!(inspected.is_ready());
    }

    // Overridden contexts are what every API sees.
    let runtime = DtcgRuntime::from_json(
        "root.json",
        json!({
            "version": "2025.10",
            "modifiers": {"theme": {"contexts": {"light": [], "dark": []}, "default": "light"}},
            "resolutionOrder": [{"$ref": "#/modifiers/theme", "contexts": {"a": [number(1)], "b": [number(2)]}, "default": "b"}]
        })
        .to_string(),
    )
    .unwrap();
    let axes = runtime.modifiers().unwrap().ready().unwrap();
    assert_eq!(axes[0].contexts, ["a", "b"]);
    assert_eq!(
        runtime
            .normalize_selection(&[("theme", "a")])
            .unwrap()
            .ready()
            .unwrap(),
        [("theme".to_owned(), "a".to_owned())]
    );
    let tokens = runtime.resolve(&[("theme", "a")]).unwrap().ready().unwrap();
    assert_eq!(tokens.get("value").unwrap().value, json!(1));
}

fn assert_invalid_everywhere(document: Value, expected: &str) {
    let runtime = DtcgRuntime::from_json("root.json", document.to_string()).unwrap();
    let results = [
        runtime.modifiers().map(|_| ()),
        runtime.normalize_selection(&[]).map(|_| ()),
        runtime.resolve(&[]).map(|_| ()),
        runtime.validate_complete().map(|_| ()),
    ];
    for result in results {
        assert!(
            matches!(&result, Err(DtcgError::InvalidDocument { message, .. }) if message == expected),
            "{document}: {result:?}"
        );
    }
}

#[test]
fn duplicate_and_unnamed_modifiers_are_rejected_everywhere() {
    assert_invalid_everywhere(
        json!({
            "version": "2025.10",
            "modifiers": {"theme": {"contexts": {"light": [], "dark": []}, "default": "light"}},
            "resolutionOrder": [
                {"$ref": "#/modifiers/theme"},
                {"type": "modifier", "name": "theme", "contexts": {"a": [], "b": []}, "default": "a"}
            ]
        }),
        "resolutionOrder defines modifier `theme` more than once",
    );
    assert_invalid_everywhere(
        json!({
            "version": "2025.10",
            "resolutionOrder": [{"type": "modifier", "contexts": {"a": [], "b": []}, "default": "a"}]
        }),
        "inline resolution step requires `name`",
    );
}

#[test]
fn a_token_document_with_a_modifiers_group_has_no_axes() {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        json!({"modifiers": {"$type": "number", "scale": {"$value": 2}}}).to_string(),
    )
    .unwrap();
    assert!(runtime.modifiers().unwrap().ready().unwrap().is_empty());
    assert!(
        runtime
            .normalize_selection(&[])
            .unwrap()
            .ready()
            .unwrap()
            .is_empty()
    );
    assert!(runtime.resolve(&[]).unwrap().is_ready());
}

#[test]
fn reference_sites_follow_overrides_of_referenced_resolvers() {
    let mut runtime = DtcgRuntime::from_json(
        "root.json",
        json!({
            "version": "2025.10",
            "resolutionOrder": [{"type": "set", "name": "brand", "sources": [{
                "$ref": "brand.resolver.json",
                "sets": {"base": {"sources": [{"$ref": "override.json"}]}}
            }]}]
        })
        .to_string(),
    )
    .unwrap();
    runtime
        .provide_json(
            "brand.resolver.json",
            json!({
                "version": "2025.10",
                "sets": {"base": {"sources": [{"$ref": "original.json"}]}},
                "resolutionOrder": [{"$ref": "#/sets/base"}]
            })
            .to_string(),
        )
        .unwrap();
    runtime
        .provide_json(
            "override.json",
            json!({"value": {"$type": "number", "$value": 3}}).to_string(),
        )
        .unwrap();

    let references = runtime.source_references().unwrap().ready().expect(
        "original.json is reachable only through the replaced definition and is not requested",
    );
    let sites = references
        .iter()
        .map(|reference| {
            (
                reference.reference().resource_id().as_str().to_owned(),
                reference
                    .origin()
                    .resource()
                    .map(|id| id.as_str().to_owned()),
                reference.origin().pointer().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    assert!(
        sites.contains(&(
            "override.json".into(),
            Some("root.json".into()),
            "/resolutionOrder/0/sources/0/sets/base/sources/0/$ref".into()
        )),
        "{sites:?}"
    );
    assert!(
        sites.iter().all(|(target, ..)| target != "original.json"),
        "{sites:?}"
    );

    let tokens = runtime.resolve(&[]).unwrap().ready().unwrap();
    assert_eq!(tokens.get("value").unwrap().value, json!(3));
}
