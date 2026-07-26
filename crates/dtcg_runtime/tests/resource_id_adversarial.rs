use dtcg_runtime::ResourceId;

#[test]
fn canonicalizes_relative_references_and_keeps_fragments_out_of_catalog_keys() {
    let parent = ResourceId::parse("themes/base/root.json").unwrap();
    let reference = parent
        .resolve("../shared/palette.json#/colors/primary")
        .unwrap();

    assert_eq!(
        reference.resource_id().as_str(),
        "themes/shared/palette.json"
    );
    assert_eq!(reference.fragment(), Some("/colors/primary"));
    assert_eq!(
        reference.original(),
        "../shared/palette.json#/colors/primary"
    );
    assert_eq!(
        parent.resolve("#/tokens/button").unwrap().resource_id(),
        &parent
    );
}

#[test]
fn rejects_invalid_logical_resource_references() {
    let parent = ResourceId::parse("themes/base/root.json").unwrap();
    for invalid in [
        "",
        "/absolute.json",
        "../../../escape.json",
        "#not-a-pointer",
        "#/tokens/~2bad",
    ] {
        assert!(
            parent.resolve(invalid).is_err(),
            "{invalid} must be rejected"
        );
    }
}
