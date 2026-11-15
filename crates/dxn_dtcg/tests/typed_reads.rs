// Typed reads by path (`ResolvedTokens::get_as`) and the direct members of a group
// (`ResolvedTokens::children`).
use dxn_dtcg::{
    DtcgError, DtcgRuntime, GroupChild, ResolvedTokens,
    values::{Color, Dimension, DimensionUnit, TypedValue},
};
use serde_json::{Value, json};
use wasm_bindgen_test::wasm_bindgen_test;

fn resolve(document: Value) -> ResolvedTokens {
    DtcgRuntime::from_json("tokens.json", document.to_string())
        .unwrap()
        .resolve(&[])
        .unwrap()
        .ready()
        .unwrap()
}

fn px(value: f64) -> Value {
    json!({"value": value, "unit": "px"})
}

fn document() -> Value {
    json!({
        "space": {
            "$type": "dimension",
            "sm": {"$value": px(4.0)},
            "inset": {
                "card": {"$value": px(12.0)},
                "$root": {"$value": px(8.0)}
            },
            "md": {"$value": "{space.sm}"},
            "scale": {"$type": "number", "ratio": {"$value": 1.5}}
        },
        "brand": {"$type": "fontFamily", "$value": "Inter"},
        "ink": {"$type": "color", "$value": {"colorSpace": "srgb", "components": [0, 0, 0]}}
    })
}

#[wasm_bindgen_test(unsupported = test)]
fn get_as_converts_the_token_at_a_path() {
    let tokens = resolve(document());
    let md: Dimension = tokens.get_as("space.md").unwrap();
    assert_eq!(
        md,
        Dimension {
            value: 4.0,
            unit: DimensionUnit::Px
        }
    );
    assert_eq!(tokens.get_as::<f64>("space.scale.ratio").unwrap(), 1.5);
    assert_eq!(tokens.get_as::<String>("brand").unwrap(), "Inter");
    assert_eq!(tokens.get_as::<Color>("ink").unwrap().alpha, 1.0);
    assert_eq!(
        tokens
            .get_as::<Dimension>("space.inset.$root")
            .unwrap()
            .value,
        8.0
    );
    assert!(matches!(
        tokens.get_as::<TypedValue>("space.sm").unwrap(),
        TypedValue::Dimension(_)
    ));
}

#[wasm_bindgen_test(unsupported = test)]
fn get_as_names_a_missing_path() {
    let tokens = resolve(document());
    for path in ["space.lg", "space", "", "space.sm.value"] {
        let error = tokens.get_as::<Dimension>(path).unwrap_err();
        assert_eq!(
            error,
            DtcgError::Projection {
                path: path.into(),
                message: "required token is missing".into(),
            }
        );
    }
    assert_eq!(
        tokens
            .get_as::<Dimension>("space.lg")
            .unwrap_err()
            .to_string(),
        "projection `space.lg`: required token is missing"
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn get_as_reports_a_wrong_shape_as_the_conversion_does() {
    let tokens = resolve(document());
    assert_eq!(
        tokens.get_as::<f64>("space.sm").unwrap_err(),
        DtcgError::Projection {
            path: "space.sm".into(),
            message: "expected numeric value".into(),
        }
    );
    assert_eq!(
        tokens.get_as::<Color>("brand").unwrap_err(),
        DtcgError::Projection {
            path: "brand".into(),
            message: "expected a color value".into(),
        }
    );
}

fn listing(children: &[GroupChild<'_>]) -> Vec<(&'static str, String)> {
    children
        .iter()
        .map(|child| match child {
            GroupChild::Token(token) => {
                assert_eq!(child.path(), &*token.path);
                ("token", child.name().to_owned())
            }
            GroupChild::Group(group) => {
                assert_eq!(child.path(), &*group.path);
                ("group", child.name().to_owned())
            }
            _ => unreachable!(),
        })
        .collect()
}

#[wasm_bindgen_test(unsupported = test)]
fn children_lists_direct_tokens_and_subgroups_in_resolution_order() {
    let tokens = resolve(document());
    let space = tokens.children("space").unwrap();
    assert_eq!(
        listing(&space),
        [
            ("token", "sm".to_owned()),
            ("group", "inset".to_owned()),
            ("token", "md".to_owned()),
            ("group", "scale".to_owned()),
        ]
    );
    let GroupChild::Group(scale) = space[3] else {
        panic!()
    };
    assert_eq!(scale.token_type.as_deref(), Some("number"));

    assert_eq!(
        listing(&tokens.children("").unwrap()),
        [
            ("group", "space".to_owned()),
            ("token", "brand".to_owned()),
            ("token", "ink".to_owned()),
        ]
    );
    assert_eq!(
        listing(&tokens.children("space.inset").unwrap()),
        [("token", "card".to_owned()), ("token", "$root".to_owned())]
    );
    assert_eq!(
        tokens.children("space.inset").unwrap()[1].path(),
        "space.inset.$root"
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn children_of_a_token_or_an_unknown_path_is_none() {
    let tokens = resolve(document());
    assert!(tokens.children("space.sm").is_none());
    assert!(tokens.children("space.lg").is_none());
    assert!(tokens.children("spa").is_none());
    assert!(tokens.children("space.").is_none());
    // A group with no tokens is not part of the resolution.
    let tokens =
        resolve(json!({"empty": {"$type": "number"}, "n": {"$type": "number", "$value": 1}}));
    assert!(tokens.children("empty").is_none());
    assert_eq!(tokens.children("").unwrap().len(), 1);
    assert!(ResolvedTokens::default().children("").is_none());
}

#[wasm_bindgen_test(unsupported = test)]
fn children_follow_the_merged_structure_of_a_resolver() {
    let runtime = DtcgRuntime::from_json(
        "r.json",
        json!({
            "version": "2025.10",
            "resolutionOrder": [
                {"type": "set", "name": "base", "sources": [
                    {"size": {"$type": "number", "sm": {"$value": 1}, "lg": {"$value": 3}}}
                ]},
                {"type": "set", "name": "more", "sources": [
                    {"size": {"md": {"$value": 2}, "sm": {"$value": 0}}}
                ]}
            ]
        })
        .to_string(),
    )
    .unwrap();
    let tokens = runtime.resolve(&[]).unwrap().ready().unwrap();
    let names = tokens
        .children("size")
        .unwrap()
        .iter()
        .map(GroupChild::name)
        .collect::<Vec<_>>();
    // Each path sits at its last definition.
    assert_eq!(names, ["lg", "md", "sm"]);
    assert_eq!(tokens.get_as::<f64>("size.sm").unwrap(), 0.0);
}
