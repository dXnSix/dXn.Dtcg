use dtcg_runtime::{
    DtcgDiagnostic, DtcgDiagnosticCode, DtcgError, DtcgRuntime, Resolution, ResourceId,
    RuntimeLimits,
};
use serde_json::{Value, json};

fn resolve_error(document: Value) -> DtcgError {
    DtcgRuntime::from_json("tokens.json", document.to_string())
        .unwrap()
        .resolve(&[])
        .unwrap_err()
}

fn number(value: i64) -> Value {
    json!({"$type": "number", "$value": value})
}

#[test]
fn runtime_limits_use_chainable_setters_and_are_exposed() {
    let limits = RuntimeLimits::default()
        .with_max_documents(3)
        .with_max_reference_depth(4)
        .with_max_alias_depth(5);
    assert_eq!(limits.max_documents, 3);
    assert_eq!(limits.max_reference_depth, 4);
    assert_eq!(limits.max_alias_depth, 5);

    let runtime = DtcgRuntime::with_limits(limits.clone());
    assert_eq!(runtime.limits(), &limits);
    assert_eq!(DtcgRuntime::default().limits(), &RuntimeLimits::default());
}

#[test]
fn runtime_debug_output_names_documents_but_never_their_bodies() {
    let mut runtime = DtcgRuntime::from_json(
        "tokens.json",
        json!({"secret-body-marker": {"$type": "number", "$value": 7}}).to_string(),
    )
    .unwrap();
    runtime
        .provide_json("library.json", r#"{"another-hidden-marker":{}}"#)
        .unwrap();

    let debug = format!("{runtime:?}");
    assert!(debug.starts_with("DtcgRuntime"), "{debug}");
    assert!(debug.contains("tokens.json"), "{debug}");
    assert!(debug.contains("library.json"), "{debug}");
    assert!(debug.contains("max_alias_depth"), "{debug}");
    assert!(debug.contains("catalog_revision: 2"), "{debug}");
    assert!(!debug.contains("secret-body-marker"), "{debug}");
    assert!(!debug.contains("another-hidden-marker"), "{debug}");
}

#[test]
fn missing_alias_targets_are_unresolved_references_from_the_referring_token() {
    for (alias, target) in [("{missing}", "missing"), ("{group}", "group"), ("{}", "")] {
        let error = resolve_error(json!({
            "group": {"inner": number(1)},
            "uses": {"$type": "number", "$value": alias}
        }));
        match &error {
            DtcgError::UnresolvedReference {
                resource,
                path,
                target: reported,
                message,
            } => {
                assert_eq!(resource.as_str(), "tokens.json");
                assert_eq!(path, "uses");
                assert_eq!(reported, target);
                assert_eq!(message, "missing alias target");
            }
            other => panic!("expected an unresolved reference for {alias}, got {other:?}"),
        }
        assert_eq!(
            DtcgDiagnostic::from_error(&error).code(),
            DtcgDiagnosticCode::UnresolvedReference
        );
    }
}

#[test]
fn missing_and_malformed_extends_targets_are_unresolved_references() {
    let missing = resolve_error(json!({"theme": {"$extends": "{base}", "a": number(1)}}));
    assert!(
        matches!(&missing, DtcgError::UnresolvedReference { path, target, message, .. }
            if path == "theme" && target == "{base}" && message == "extended group was not found"),
        "{missing:?}"
    );

    let malformed = resolve_error(json!({"theme": {"$extends": "base", "a": number(1)}}));
    assert!(
        matches!(&malformed, DtcgError::UnresolvedReference { path, target, .. }
            if path == "theme" && target == "base"),
        "{malformed:?}"
    );

    let diagnostic = DtcgDiagnostic::from_error(&missing);
    assert_eq!(diagnostic.code(), DtcgDiagnosticCode::UnresolvedReference);
    assert_eq!(
        diagnostic.resource().map(ResourceId::as_str),
        Some("tokens.json")
    );
    assert_eq!(diagnostic.path(), Some("theme"));
    assert!(diagnostic.message().contains("{base}"));
}

#[test]
fn errors_are_comparable_clonable_and_close_their_backticks() {
    let error = ResourceId::parse("/abs.json").unwrap_err();
    assert_eq!(error.clone(), error);
    assert_eq!(error.to_string(), "invalid logical resource id `/abs.json`");
    assert_eq!(
        DtcgError::InvalidPointer("#/a~2".into()).to_string(),
        "invalid JSON pointer `#/a~2`"
    );
    assert_ne!(error, DtcgError::InvalidResourceId("other.json".into()));
}

#[test]
fn resolution_helpers_map_and_inspect_both_states() {
    let ready: Resolution<i32> = Resolution::Ready(2);
    assert!(ready.is_ready());
    assert!(!ready.is_missing());
    assert_eq!(ready.missing(), None);
    assert_eq!(ready.map(|value| value * 3), Resolution::Ready(6));

    let mut runtime = DtcgRuntime::from_json(
        "root.json",
        json!({
            "version": "2025.10",
            "resolutionOrder": [{"type": "set", "name": "base", "sources": [{"$ref": "base.json"}]}]
        })
        .to_string(),
    )
    .unwrap();
    let missing = runtime.resolve(&[]).unwrap();
    assert!(missing.is_missing());
    assert!(!missing.is_ready());
    let requests = missing.missing().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].resource_id.as_str(), "base.json");
    let mapped = missing.clone().map(|tokens| tokens.len());
    assert_eq!(mapped.missing(), missing.missing());

    runtime
        .provide_json("base.json", json!({"a": number(1)}).to_string())
        .unwrap();
    let ready = runtime.resolve(&[]).unwrap().map(|tokens| tokens.len());
    assert_eq!(ready, Resolution::Ready(1));
}

#[test]
fn source_occurrences_name_their_document() {
    let mut runtime = DtcgRuntime::from_json(
        "root.json",
        json!({
            "version": "2025.10",
            "resolutionOrder": [{"type": "set", "name": "base", "sources": [{"$ref": "base.json"}]}]
        })
        .to_string(),
    )
    .unwrap();
    runtime
        .provide_json("base.json", json!({"a": number(1)}).to_string())
        .unwrap();
    let occurrences = runtime
        .inspect_source(&json!({"$ref": "base.json"}), &[])
        .unwrap()
        .ready()
        .unwrap();
    assert_eq!(occurrences.len(), 1);
    assert_eq!(occurrences[0].resource().as_str(), "base.json");
}

fn envelope(value: i64) -> String {
    json!({
        "data": {
            "name": format!("project-{value}"),
            "version": "2025.10",
            "resolutionOrder": [{"type": "set", "name": "base", "sources": [{"x": number(value)}]}]
        }
    })
    .to_string()
}

#[test]
fn replace_envelope_swaps_the_root_resolver() {
    let (mut runtime, first) =
        DtcgRuntime::from_envelope("project.tokeneditor", &envelope(1)).unwrap();
    assert_eq!(first.name(), Some("project-1"));
    let before = runtime.resolve(&[]).unwrap().ready().unwrap();
    assert_eq!(before.get("x").unwrap().value, json!(1));

    let second = runtime
        .replace_envelope("project.tokeneditor", &envelope(2))
        .unwrap();
    assert_eq!(second.name(), Some("project-2"));
    let after = runtime.resolve(&[]).unwrap().ready().unwrap();
    assert_eq!(after.get("x").unwrap().value, json!(2));

    let revision = runtime.catalog_revision();
    let invalid = runtime.replace_envelope(
        "project.tokeneditor",
        &json!({"data": {"version": "2024", "resolutionOrder": []}}).to_string(),
    );
    assert!(matches!(invalid, Err(DtcgError::InvalidEnvelope { .. })));
    assert_eq!(runtime.catalog_revision(), revision);
    assert_eq!(
        runtime
            .resolve(&[])
            .unwrap()
            .ready()
            .unwrap()
            .get("x")
            .unwrap()
            .value,
        json!(2)
    );
    assert!(matches!(
        runtime.replace_envelope("other.tokeneditor", &envelope(3)),
        Err(DtcgError::UnknownDocument(_))
    ));
}

#[test]
fn fingerprints_and_equality_ignore_object_member_order() {
    let resolve = |document: &str| {
        DtcgRuntime::from_json("tokens.json", document.to_owned())
            .unwrap()
            .resolve(&[])
            .unwrap()
            .ready()
            .unwrap()
    };
    let first = resolve(
        r#"{"size":{"$type":"dimension","$value":{"value":4,"unit":"px"}},
            "shadow":{"$type":"shadow","$value":{"color":{"colorSpace":"srgb","components":[0,0,0],"alpha":0.5},
              "offsetX":{"value":0,"unit":"px"},"offsetY":{"value":1,"unit":"px"},
              "blur":{"value":2,"unit":"px"},"spread":{"value":0,"unit":"px"}}}}"#,
    );
    let reordered = resolve(
        r#"{"size":{"$type":"dimension","$value":{"unit":"px","value":4}},
            "shadow":{"$type":"shadow","$value":{"spread":{"unit":"px","value":0},"blur":{"unit":"px","value":2},
              "offsetY":{"unit":"px","value":1},"offsetX":{"unit":"px","value":0},
              "color":{"alpha":0.5,"components":[0,0,0],"colorSpace":"srgb"}}}}"#,
    );
    assert_eq!(first.fingerprint(), reordered.fingerprint());
    assert_eq!(first, reordered);

    let different = resolve(r#"{"size":{"$type":"dimension","$value":{"unit":"px","value":5}}}"#);
    assert_ne!(first.fingerprint(), different.fingerprint());
}
