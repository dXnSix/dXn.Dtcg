use dtcg_runtime::{DtcgRuntime, DtcgTokens};

#[derive(Debug, DtcgTokens, PartialEq)]
struct Palette {
    #[dtcg(path = "color.primary")]
    primary: String,
}

#[test]
fn derives_a_narrow_projection() {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r#"{"color":{"primary":{"$type":"fontFamily","$value":"teal"}},"internal":{"spacing":{"$type":"number","$value":8}}}"#,
    )
    .unwrap();
    assert_eq!(
        runtime.project::<Palette>(&[]).unwrap().ready(),
        Some(Palette {
            primary: "teal".into()
        })
    );
}
