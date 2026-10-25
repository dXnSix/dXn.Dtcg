use dxn_dtcg::{DtcgError, DtcgRuntime, ResourceId};
use wasm_bindgen_test::wasm_bindgen_test;

const COLON_PATHS: [&str; 7] = [
    "C:/x.json",
    "c:x.json",
    "a/C:/b.json",
    "https://example.com/a.json",
    "a/../C:/x.json",
    "tokens.json:hidden",
    "tokens.json::$DATA",
];

fn assert_invalid_resource_id(result: Result<impl std::fmt::Debug, DtcgError>, expected: &str) {
    match result {
        Err(DtcgError::InvalidResourceId(id)) => assert_eq!(id, expected),
        other => panic!("{expected} must fail with InvalidResourceId, got {other:?}"),
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn parse_rejects_a_colon_in_any_segment() {
    for path in COLON_PATHS {
        assert_invalid_resource_id(ResourceId::parse(path), path);
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn resolve_rejects_a_colon_in_any_segment() {
    for parent in ["root.json", "themes/base/root.json"] {
        let parent = ResourceId::parse(parent).unwrap();
        for path in COLON_PATHS {
            assert_invalid_resource_id(parent.resolve(path), path);
            let reference = format!("{path}#/colors/primary");
            assert_invalid_resource_id(parent.resolve(&reference), path);
        }
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn climbing_cannot_move_a_drive_segment_to_the_front() {
    let parent = ResourceId::parse("themes/root.json").unwrap();
    assert_invalid_resource_id(parent.resolve("../C:/x.json"), "../C:/x.json");
    assert_invalid_resource_id(parent.resolve("C:/../../x.json"), "C:/../../x.json");
}

#[wasm_bindgen_test(unsupported = test)]
fn colons_remain_valid_in_fragments() {
    let parent = ResourceId::parse("themes/root.json").unwrap();
    let reference = parent.resolve("palette.json#/colors/a:b").unwrap();
    assert_eq!(reference.resource_id().as_str(), "themes/palette.json");
    assert_eq!(reference.fragment(), Some("/colors/a:b"));
    assert_eq!(parent.resolve("#/c:d").unwrap().resource_id(), &parent);
}

#[wasm_bindgen_test(unsupported = test)]
fn resolver_source_with_a_drive_prefix_fails_instead_of_requesting_the_document() {
    let runtime = DtcgRuntime::from_json(
        "root.json",
        r##"{
          "version": "2025.10",
          "sets": { "base": { "sources": [{ "$ref": "C:/Windows/win.ini" }] } },
          "resolutionOrder": [{ "$ref": "#/sets/base" }]
        }"##,
    )
    .unwrap();

    assert_invalid_resource_id(runtime.resolve(&[]), "C:/Windows/win.ini");
    assert_invalid_resource_id(runtime.validate_complete(), "C:/Windows/win.ini");
}

#[wasm_bindgen_test(unsupported = test)]
fn modifier_context_with_a_drive_prefix_fails_instead_of_requesting_the_document() {
    let runtime = DtcgRuntime::from_json(
        "themes/root.json",
        r##"{
          "version": "2025.10",
          "modifiers": { "theme": { "default": "light", "contexts": {
            "light": [{ "$ref": "light.json" }],
            "dark": [{ "$ref": "../C:/Windows/win.ini" }]
          } } },
          "resolutionOrder": [{ "$ref": "#/modifiers/theme" }]
        }"##,
    )
    .unwrap();

    assert_invalid_resource_id(
        runtime.resolve(&[("theme", "dark")]),
        "../C:/Windows/win.ini",
    );
    assert_invalid_resource_id(runtime.validate_complete(), "../C:/Windows/win.ini");
}
