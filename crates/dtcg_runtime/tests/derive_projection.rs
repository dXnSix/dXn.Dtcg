use std::collections::BTreeMap;

use dtcg_runtime::{DtcgComponentTokens, DtcgRuntime, DtcgTokens};

#[derive(Debug, DtcgTokens, PartialEq)]
#[dtcg(root = "palette")]
struct PaletteView {
    #[dtcg(rename = "primary")]
    main: String,
    #[dtcg(rest)]
    extra: BTreeMap<String, String>,
}

#[derive(Debug, DtcgComponentTokens, PartialEq)]
#[dtcg(id = "button")]
struct ButtonView {
    #[dtcg(path = "button.fill")]
    fill: String,
}

#[test]
fn projects_renamed_root_fields_and_tolerates_unprojected_tokens() {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r#"{"palette":{"primary":{"$value":"teal"},"spacing":{"$value":8}},"button":{"fill":{"$value":"blue"}}}"#,
    )
    .unwrap();

    assert_eq!(
        runtime.project::<PaletteView>(&[]).unwrap().ready(),
        Some(PaletteView {
            main: "teal".into(),
            extra: BTreeMap::new()
        })
    );
    assert_eq!(ButtonView::component_id(), "button");
    assert_eq!(
        runtime
            .project::<ButtonView>(&[])
            .unwrap()
            .ready()
            .unwrap()
            .fill,
        "blue"
    );
}
