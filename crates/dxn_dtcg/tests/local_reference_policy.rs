use wasm_bindgen_test::wasm_bindgen_test;
// `LocalReferencePolicy`, the opt-in rule that a resolver may only reference relative local
// documents: single references, and every `$ref` of a resolver's sets and modifier contexts.
use dxn_dtcg::{DtcgError, LocalReferencePolicy, ResourceId};
use serde_json::json;

fn resource() -> ResourceId {
    ResourceId::parse("themes/root.resolver.json").unwrap()
}

fn rejection(reference: &str) -> String {
    match LocalReferencePolicy::check(&resource(), reference) {
        Err(DtcgError::InvalidReference {
            resource,
            reference: reported,
            message,
        }) => {
            assert_eq!(resource, self::resource());
            assert_eq!(reported, reference);
            message
        }
        other => panic!("`{reference}` must be rejected, got {other:?}"),
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn relative_and_fragment_references_are_local() {
    for reference in [
        "base.json",
        "../shared/colors.json",
        "sets/dark.json#/colors",
        "#/sets/base",
    ] {
        LocalReferencePolicy::check(&resource(), reference).expect(reference);
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn empty_absolute_backslash_and_scheme_references_are_rejected() {
    assert_eq!(rejection(""), "reference is empty");
    assert_eq!(
        rejection("/tokens/base.json"),
        "absolute paths are not supported"
    );
    assert_eq!(
        rejection("sets\\dark.json"),
        "backslashes are not supported"
    );
    for reference in [
        "https://example.com/base.json",
        "file:base.json",
        "c:base.json",
        "x-custom+v1.2:base.json#/a",
    ] {
        assert_eq!(rejection(reference), "URI schemes are not supported");
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn resolver_checks_cover_set_sources_and_modifier_contexts() {
    let local = json!({
        "sets": {"base": {"sources": [{"$ref": "base.json"}, {"inline": {}}]}},
        "modifiers": {"theme": {"contexts": {"light": [{"$ref": "light.json"}], "dark": []}}}
    });
    LocalReferencePolicy::check_resolver(&resource(), &local).expect("local resolver");

    let remote_set =
        json!({"sets": {"base": {"sources": [{"$ref": "https://example.com/a.json"}]}}});
    assert!(matches!(
        LocalReferencePolicy::check_resolver(&resource(), &remote_set),
        Err(DtcgError::InvalidReference { reference, .. }) if reference == "https://example.com/a.json"
    ));

    let absolute_context =
        json!({"modifiers": {"theme": {"contexts": {"dark": [{"$ref": "/dark.json"}]}}}});
    assert!(matches!(
        LocalReferencePolicy::check_resolver(&resource(), &absolute_context),
        Err(DtcgError::InvalidReference { reference, .. }) if reference == "/dark.json"
    ));

    // Not an object: nothing to check.
    LocalReferencePolicy::check_resolver(&resource(), &json!([])).expect("non-object");
}
