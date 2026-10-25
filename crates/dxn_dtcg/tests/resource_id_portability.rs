use dxn_dtcg::{DtcgError, DtcgRuntime, ResourceId};
use wasm_bindgen_test::wasm_bindgen_test;

fn assert_invalid(result: Result<impl std::fmt::Debug, DtcgError>, id: &str) {
    match result {
        Err(DtcgError::InvalidResourceId(_)) => {}
        other => panic!("{id:?} must fail with InvalidResourceId, got {other:?}"),
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn ids_with_fragments_are_rejected_everywhere_an_id_is_named() {
    for id in ["tokens.json#/colors", "tokens.json#", "#/colors"] {
        assert_invalid(ResourceId::parse(id), id);
        let mut runtime = DtcgRuntime::from_json("tokens.json", "{}").unwrap();
        assert_invalid(runtime.set_root(id), id);
        assert_invalid(runtime.provide_json(id, "{}"), id);
        assert_invalid(runtime.contains_document(id), id);
        assert_invalid(runtime.replace_json(id, "{}"), id);
        assert_invalid(runtime.remove_document(id), id);
        assert_invalid(DtcgRuntime::from_json(id, "{}"), id);
    }
    let runtime = DtcgRuntime::from_json("tokens.json", "{}").unwrap();
    assert!(runtime.contains_document("tokens.json").unwrap());
}

#[wasm_bindgen_test(unsupported = test)]
fn references_keep_their_fragments() {
    let parent = ResourceId::parse("themes/root.json").unwrap();
    let reference = parent.resolve("palette.json#/colors/primary").unwrap();
    assert_eq!(reference.resource_id().as_str(), "themes/palette.json");
    assert_eq!(reference.fragment(), Some("/colors/primary"));
    assert_eq!(
        parent.resolve("#/sets/base").unwrap().resource_id(),
        &parent
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn non_portable_segments_are_rejected() {
    let rejected = [
        "nul\0.json",
        "a/b\u{1}.json",
        "tab\t.json",
        "line\n.json",
        "del\u{7f}.json",
        "a<b.json",
        "a>b.json",
        "a\"b.json",
        "a|b.json",
        "what?.json",
        "star*.json",
        "trailing.",
        "dir./file.json",
        "trailing ",
        "dir /file.json",
        "...",
        "CON",
        "con.json",
        "Prn.tokens.json",
        "aux",
        "themes/NUL.json",
        "COM1",
        "com9.json",
        "LPT1.txt",
        "lpt9",
    ];
    let parent = ResourceId::parse("themes/root.json").unwrap();
    for id in rejected {
        assert_invalid(ResourceId::parse(id), id);
        assert_invalid(parent.resolve(id), id);
        assert_invalid(parent.resolve(format!("{id}#/x")), id);
    }
}

#[wasm_bindgen_test(unsupported = test)]
fn portable_ids_keep_working() {
    for (id, canonical) in [
        ("tokens.json", "tokens.json"),
        ("themes/base/root.json", "themes/base/root.json"),
        ("themes/../shared/palette.json", "shared/palette.json"),
        ("ünïcödé/файл.json", "ünïcödé/файл.json"),
        ("console.json", "console.json"),
        ("com0.json", "com0.json"),
        ("com10.json", "com10.json"),
        ("lpt.json", "lpt.json"),
        ("connection/aux-tokens.json", "connection/aux-tokens.json"),
        (".hidden.json", ".hidden.json"),
        ("with space.json", "with space.json"),
        ("percent%20.json", "percent%20.json"),
    ] {
        assert_eq!(ResourceId::parse(id).unwrap().as_str(), canonical, "{id}");
    }
    let parent = ResourceId::parse("themes/base/root.json").unwrap();
    assert_eq!(
        parent
            .resolve("../shared/palette.json")
            .unwrap()
            .resource_id()
            .as_str(),
        "themes/shared/palette.json"
    );
}
