use dxn_dtcg::{Deprecation, DtcgRuntime, ResolvedTokens};
use serde_json::{Value, json};

fn resolve(document: Value) -> ResolvedTokens {
    DtcgRuntime::from_json("tokens.json", document.to_string())
        .unwrap()
        .resolve(&[])
        .unwrap()
        .ready()
        .unwrap()
}

fn layered(sources: &[Value]) -> ResolvedTokens {
    let references = (0..sources.len())
        .map(|index| json!({"$ref": format!("s{index}.json")}))
        .collect::<Vec<_>>();
    let mut runtime = DtcgRuntime::from_json(
        "root.json",
        json!({
            "version": "2025.10",
            "resolutionOrder": [{"type": "set", "name": "all", "sources": references}]
        })
        .to_string(),
    )
    .unwrap();
    for (index, source) in sources.iter().enumerate() {
        runtime
            .provide_json(format!("s{index}.json"), source.to_string())
            .unwrap();
    }
    runtime.resolve(&[]).unwrap().ready().unwrap()
}

fn deprecation(tokens: &ResolvedTokens, path: &str) -> Deprecation {
    tokens.get(path).unwrap().deprecated.clone()
}

#[test]
fn tokens_expose_their_own_description_deprecation_and_extensions() {
    let tokens = resolve(json!({
        "g": {
            "$type": "number",
            "$description": "group text",
            "$extensions": {"group": true},
            "plain": {"$value": 1},
            "described": {"$value": 2, "$description": "token text", "$extensions": {"k": 1}},
            "flagged": {"$value": 3, "$deprecated": true},
            "reasoned": {"$value": 4, "$deprecated": "Use {g.plain}"},
            "kept": {"$value": 5, "$deprecated": false}
        }
    }));
    let plain = tokens.get("g.plain").unwrap();
    assert_eq!(plain.description, None);
    assert_eq!(plain.extensions, Value::Null);
    assert_eq!(plain.deprecated, Deprecation::NotDeprecated);

    let described = tokens.get("g.described").unwrap();
    assert_eq!(described.description.as_deref(), Some("token text"));
    assert_eq!(described.extensions, json!({"k": 1}));

    assert_eq!(deprecation(&tokens, "g.flagged"), Deprecation::Deprecated);
    let reasoned = deprecation(&tokens, "g.reasoned");
    assert_eq!(
        reasoned,
        Deprecation::DeprecatedWithReason("Use {g.plain}".into())
    );
    assert_eq!(reasoned.reason(), Some("Use {g.plain}"));
    assert!(reasoned.is_deprecated());
    assert!(!deprecation(&tokens, "g.kept").is_deprecated());
}

#[test]
fn group_deprecation_is_inherited_until_a_nearer_value_overrides_it() {
    let tokens = resolve(json!({
        "legacy": {
            "$type": "number",
            "$deprecated": "Use the new scale",
            "a": {"$value": 1},
            "b": {"$value": 2, "$deprecated": false},
            "c": {"$value": 3, "$deprecated": true},
            "inner": {
                "$deprecated": false,
                "d": {"$value": 4},
                "deeper": {"$deprecated": true, "e": {"$value": 5}}
            }
        }
    }));
    assert_eq!(
        deprecation(&tokens, "legacy.a"),
        Deprecation::DeprecatedWithReason("Use the new scale".into())
    );
    assert_eq!(deprecation(&tokens, "legacy.b"), Deprecation::NotDeprecated);
    assert_eq!(deprecation(&tokens, "legacy.c"), Deprecation::Deprecated);
    assert_eq!(
        deprecation(&tokens, "legacy.inner.d"),
        Deprecation::NotDeprecated
    );
    assert_eq!(
        deprecation(&tokens, "legacy.inner.deeper.e"),
        Deprecation::Deprecated
    );
}

#[test]
fn deprecation_is_evaluated_on_the_merged_structure_and_through_extends() {
    let tokens = layered(&[
        json!({"old": {"$type": "number", "a": {"$value": 1}}}),
        json!({"old": {"$deprecated": true, "b": {"$value": 2}}}),
        json!({
            "base": {"$type": "number", "$deprecated": "base is retired", "$description": "base", "x": {"$value": 1}},
            "derived": {"$extends": "{base}", "y": {"$value": 2}},
            "revived": {"$extends": "{base}", "$deprecated": false, "z": {"$value": 3}}
        }),
    ]);
    assert_eq!(deprecation(&tokens, "old.a"), Deprecation::Deprecated);
    assert_eq!(deprecation(&tokens, "old.b"), Deprecation::Deprecated);

    let retired = Deprecation::DeprecatedWithReason("base is retired".into());
    assert_eq!(deprecation(&tokens, "derived.x"), retired);
    assert_eq!(deprecation(&tokens, "derived.y"), retired);
    assert_eq!(
        deprecation(&tokens, "revived.x"),
        Deprecation::NotDeprecated
    );
    let derived = tokens.group("derived").unwrap();
    assert_eq!(derived.description.as_deref(), Some("base"));
    assert_eq!(derived.deprecated, Some(retired));
    assert_eq!(derived.token_type.as_deref(), Some("number"));
    assert_eq!(
        tokens.group("revived").unwrap().deprecated,
        Some(Deprecation::NotDeprecated)
    );
}

#[test]
fn groups_with_resolved_tokens_are_listed_in_path_order() {
    let tokens = resolve(json!({
        "$description": "root",
        "$extensions": {"vendor": {"x": 1}},
        "z": {"$type": "number", "t": {"$value": 1}},
        "a": {
            "$type": "color",
            "$description": "colors",
            "nested": {"t": {"$value": {"colorSpace": "srgb", "components": [0, 0, 0]}}},
            "empty": {}
        },
        "unused": {"$description": "no tokens"}
    }));
    assert_eq!(
        tokens
            .groups()
            .map(|group| group.path.as_ref())
            .collect::<Vec<_>>(),
        ["", "a", "a.nested", "z"]
    );
    let root = tokens.group("").unwrap();
    assert_eq!(root.description.as_deref(), Some("root"));
    assert_eq!(root.extensions, json!({"vendor": {"x": 1}}));
    assert_eq!(root.token_type, None);
    assert_eq!(root.deprecated, None);
    let nested = tokens.group("a.nested").unwrap();
    assert_eq!(nested.token_type.as_deref(), Some("color"));
    assert_eq!(nested.description, None);
    assert_eq!(nested.extensions, Value::Null);
    assert!(tokens.group("a.empty").is_none());
    assert!(tokens.group("unused").is_none());
    assert!(tokens.group("missing").is_none());
}

#[test]
fn metadata_does_not_change_the_fingerprint() {
    let plain = resolve(json!({"g": {"$type": "number", "t": {"$value": 1}}}));
    let annotated = resolve(json!({"g": {
        "$type": "number",
        "$description": "g",
        "$deprecated": true,
        "$extensions": {"k": 1},
        "t": {"$value": 1, "$description": "t", "$deprecated": "x", "$extensions": {"k": 2}}
    }}));
    assert_eq!(plain.fingerprint(), annotated.fingerprint());
    assert_ne!(plain, annotated);
}
