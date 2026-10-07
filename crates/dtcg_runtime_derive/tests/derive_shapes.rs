use dtcg_runtime::{DtcgComponentTokens, DtcgTokens};

#[derive(DtcgTokens)]
#[dtcg(root = "palette")]
#[allow(dead_code)]
struct Palette {
    #[dtcg(rename = "primary")]
    main: String,
}

#[derive(DtcgComponentTokens)]
#[dtcg(id = "button")]
#[allow(dead_code)]
struct Button {
    #[dtcg(path = "button.fill")]
    fill: String,
}

#[test]
fn derives_named_projection_shapes_with_stable_component_metadata() {
    assert_eq!(Button::component_id(), "button");
    let _ = core::mem::size_of::<Palette>();
}
