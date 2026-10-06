use dtcg_runtime::{DtcgError, DtcgRuntime, DtcgTokens, Resolution, ResolvedToken};

fn runtime() -> DtcgRuntime {
    DtcgRuntime::from_json(
        "tokens.json",
        r#"{
          "font": {"$type": "fontFamily", "$value": "Inter"},
          "ratio": {"$type": "number", "$value": 1.5},
          "type": {"$type": "fontFamily", "$value": "body"},
          "leaf": {"value": {"$type": "fontFamily", "$value": "teal"}},
          "wrapper": {"inner": {"value": {"$type": "fontFamily", "$value": "nested"}}}
        }"#,
    )
    .unwrap()
}

fn project<T: DtcgTokens>() -> Result<T, DtcgError> {
    match runtime().project::<T>(&[])? {
        Resolution::Ready(value) => Ok(value),
        Resolution::Missing(_) => panic!("in-memory document"),
    }
}

fn shout(token: &ResolvedToken) -> Result<String, DtcgError> {
    Ok(token.value.as_str().unwrap_or_default().to_uppercase())
}

#[derive(Debug, DtcgTokens, PartialEq)]
struct Optional {
    font: Option<String>,
    #[dtcg(path = "missing")]
    absent: core::option::Option<String>,
    #[dtcg(path = "ratio")]
    ratio: std::option::Option<f64>,
    #[dtcg(path = "font", with = "shout")]
    shouted: Option<String>,
    #[dtcg(path = "nowhere", with = "shout")]
    nothing: ::std::option::Option<String>,
}

#[test]
fn option_fields_are_none_when_missing_and_converted_when_present() {
    assert_eq!(
        project::<Optional>().unwrap(),
        Optional {
            font: Some("Inter".into()),
            absent: None,
            ratio: Some(1.5),
            shouted: Some("INTER".into()),
            nothing: None,
        }
    );
}

#[derive(Debug, DtcgTokens)]
#[allow(dead_code)]
struct WrongOptional {
    #[dtcg(path = "ratio")]
    ratio: Option<String>,
}

#[derive(Debug, DtcgTokens)]
#[allow(dead_code)]
struct Required {
    #[dtcg(path = "missing")]
    value: String,
}

#[test]
fn present_but_unconvertible_and_missing_required_tokens_fail() {
    assert!(matches!(
        project::<WrongOptional>(),
        Err(DtcgError::Projection { path, .. }) if path == "ratio"
    ));
    assert!(matches!(
        project::<Required>(),
        Err(DtcgError::Projection { path, message }) if path == "missing" && message == "required token is missing"
    ));
}

mod hygiene {
    #![allow(dead_code)]
    // Local items that shadow the prelude must not affect the generated code.
    type Result<T> = core::result::Result<T, ()>;
    struct Ok;
    struct Some;
    trait Default {}
    type Option = ();

    #[derive(Debug, dtcg_runtime::DtcgTokens, PartialEq)]
    pub struct Shadowed {
        pub font: String,
        pub r#type: String,
        #[dtcg(path = "missing")]
        pub absent: core::option::Option<String>,
        #[dtcg(rest)]
        pub rest: Vec<String>,
    }
}

#[test]
fn generated_code_is_hygienic_and_unraws_field_names() {
    assert_eq!(
        project::<hygiene::Shadowed>().unwrap(),
        hygiene::Shadowed {
            font: "Inter".into(),
            r#type: "body".into(),
            absent: None,
            rest: Vec::new(),
        }
    );
}

#[derive(Debug, DtcgTokens, PartialEq)]
struct Leaf<T> {
    value: T,
}

#[derive(Debug, DtcgTokens, PartialEq)]
struct Wrapper<N> {
    #[dtcg(nested)]
    inner: N,
}

#[derive(Debug, DtcgTokens, PartialEq)]
struct Holder<T, R> {
    #[dtcg(nested, path = "leaf")]
    leaf: Leaf<T>,
    #[dtcg(path = "font")]
    font: Option<T>,
    #[dtcg(rest)]
    rest: R,
}

#[test]
fn generic_leaf_and_nested_fields_get_their_own_bounds() {
    let leaf = runtime()
        .project::<Holder<String, Vec<u8>>>(&[])
        .unwrap()
        .ready()
        .unwrap();
    assert_eq!(
        leaf.leaf,
        Leaf {
            value: "teal".into()
        }
    );
    assert_eq!(leaf.font.as_deref(), Some("Inter"));
    let tokens = runtime().resolve(&[]).unwrap().ready().unwrap();
    let wrapper = <Wrapper<Leaf<String>> as DtcgTokens>::project_at(
        &tokens,
        &dtcg_runtime::TypeRegistry,
        "wrapper",
    )
    .unwrap();
    assert_eq!(
        wrapper.inner,
        Leaf {
            value: "nested".into()
        }
    );
}
