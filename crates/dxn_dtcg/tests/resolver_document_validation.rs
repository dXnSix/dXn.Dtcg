use dxn_dtcg::{DtcgError, DtcgRuntime};
use serde_json::{Value, json};

fn two_contexts() -> Value {
    json!({"light": [], "dark": []})
}

/// A valid resolver with a root set, a root modifier, an inline set and an
/// inline modifier.
fn resolver() -> Value {
    json!({
        "name": "brand",
        "version": "2025.10",
        "description": "a resolver",
        "$schema": "https://www.designtokens.org/schemas/2025.10/resolver.json",
        "$defs": {"anything": {"goes": true}},
        "sets": {"base": {"description": "base", "sources": [], "$extensions": {"x": 1}}},
        "modifiers": {"theme": {"description": "theme", "contexts": two_contexts(), "default": "light", "$extensions": {}}},
        "resolutionOrder": [
            {"$ref": "#/sets/base"},
            {"$ref": "#/modifiers/theme"},
            {"type": "set", "name": "inline", "description": "d", "sources": [], "$extensions": {}},
            {"type": "modifier", "name": "density", "description": "d", "contexts": two_contexts(), "default": "dark", "$extensions": {}}
        ]
    })
}

fn invalid_message(document: &Value) -> String {
    let runtime = DtcgRuntime::from_json("root.json", document.to_string()).unwrap();
    let messages = [
        runtime.resolve(&[]).map(|_| ()),
        runtime.validate_complete().map(|_| ()),
        runtime.modifiers().map(|_| ()),
    ]
    .map(|result| match result {
        Err(DtcgError::InvalidDocument { message, .. }) => message,
        other => panic!("{document} must be invalid, got {other:?}"),
    });
    assert!(
        messages.iter().all(|message| *message == messages[0]),
        "{messages:?}"
    );
    messages[0].clone()
}

#[test]
fn every_allowed_member_is_accepted() {
    let runtime = DtcgRuntime::from_json("root.json", resolver().to_string()).unwrap();
    assert!(runtime.resolve(&[]).unwrap().is_ready());
    assert!(runtime.validate_complete().unwrap().is_ready());
    assert_eq!(runtime.modifiers().unwrap().ready().unwrap().len(), 2);
}

#[test]
fn unknown_members_are_rejected_at_every_location() {
    let cases = [
        ("/extra", "the resolver root"),
        ("/$extensions", "the resolver root"),
        ("/x-vendor", "the resolver root"),
        ("/sets/base/extra", "set `base`"),
        ("/modifiers/theme/extra", "modifier `theme`"),
        ("/resolutionOrder/2/extra", "inline set step `inline`"),
        ("/resolutionOrder/3/extra", "inline modifier step `density`"),
    ];
    for (pointer, location) in cases {
        let mut document = resolver();
        let (parent, member) = pointer.rsplit_once('/').unwrap();
        document
            .pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(member.into(), json!(1));
        assert_eq!(
            invalid_message(&document),
            format!("unknown resolver member `{member}` in {location}"),
            "{pointer}"
        );
    }
}

#[test]
fn reference_objects_keep_their_override_members() {
    let mut document = resolver();
    document["resolutionOrder"][1] =
        json!({"$ref": "#/modifiers/theme", "default": "dark", "anything": 1});
    let runtime = DtcgRuntime::from_json("root.json", document.to_string()).unwrap();
    assert!(runtime.resolve(&[]).unwrap().is_ready());
}

#[test]
fn an_empty_resolution_order_and_single_context_modifiers_are_rejected() {
    let mut empty = resolver();
    empty["resolutionOrder"] = json!([]);
    assert_eq!(invalid_message(&empty), "resolutionOrder must not be empty");

    let mut root_single = resolver();
    root_single["modifiers"]["theme"] = json!({"contexts": {"light": []}, "default": "light"});
    assert_eq!(
        invalid_message(&root_single),
        "modifier requires at least two contexts"
    );

    let mut inline_single = resolver();
    inline_single["resolutionOrder"][3]["contexts"] = json!({"dark": []});
    assert_eq!(
        invalid_message(&inline_single),
        "modifier requires at least two contexts"
    );

    // Context arrays may be empty.
    let runtime = DtcgRuntime::from_json("root.json", resolver().to_string()).unwrap();
    assert!(runtime.resolve(&[("theme", "dark")]).unwrap().is_ready());
}

#[test]
fn nested_resolvers_follow_the_same_rules() {
    let mut nested = resolver();
    nested["sets"]["base"]["extra"] = json!(1);
    let mut runtime = DtcgRuntime::from_json(
        "root.json",
        json!({
            "version": "2025.10",
            "resolutionOrder": [{"type": "set", "name": "all", "sources": [{"$ref": "nested.json"}]}]
        })
        .to_string(),
    )
    .unwrap();
    runtime
        .provide_json("nested.json", nested.to_string())
        .unwrap();
    assert!(matches!(
        runtime.resolve(&[]),
        Err(DtcgError::InvalidDocument { message, .. })
            if message == "unknown resolver member `extra` in set `base`"
    ));
}

#[test]
fn source_references_apply_the_same_structure_rules() {
    let mut document = resolver();
    document["$extensions"] = json!({"example": {"$ref": "do-not-load.json"}});
    let runtime = DtcgRuntime::from_json("root.json", document.to_string()).unwrap();
    assert!(matches!(
        runtime.source_references(),
        Err(DtcgError::InvalidDocument { message, .. })
            if message == "unknown resolver member `$extensions` in the resolver root"
    ));
    let valid = DtcgRuntime::from_json("root.json", resolver().to_string()).unwrap();
    assert!(valid.source_references().unwrap().is_ready());
}
