// Value shapes beyond plain DTCG 2025.10: typography `lineHeight` as a `dimension` (design tools
// commonly export line heights as px tokens aliased by text styles) and the `dxn.dtcg.blur`
// extension type. Both are opt-in: these cases validate with `ValueExtensions::all()`;
// `tests/value_extensions.rs` pins their rejection without it.
use dxn_dtcg::{BLUR_TYPE, ValueExtensions, ValueValidationError, validate_value_with};
use serde_json::{Value, json};

fn validate_value(token_type: &str, value: &Value) -> Result<(), ValueValidationError> {
    validate_value_with(token_type, value, &ValueExtensions::all())
}

fn typography(line_height: serde_json::Value) -> serde_json::Value {
    json!({
        "fontFamily": "Inter",
        "fontSize": {"value": 32, "unit": "px"},
        "fontWeight": 700,
        "letterSpacing": {"value": 0, "unit": "px"},
        "lineHeight": line_height,
    })
}

#[test]
fn line_height_accepts_number_and_dimension() {
    validate_value("typography", &typography(json!(1.5))).expect("multiplier");
    validate_value(
        "typography",
        &typography(json!({"value": 40, "unit": "px"})),
    )
    .expect("px dimension");
    validate_value(
        "typography",
        &typography(json!({"value": 2.5, "unit": "rem"})),
    )
    .expect("rem dimension");
}

#[test]
fn line_height_rejects_other_shapes() {
    for line_height in [
        json!("auto"),
        json!("40px"),
        json!({"value": 40, "unit": "em"}),
        json!({"value": "40", "unit": "px"}),
        json!(null),
    ] {
        assert!(
            validate_value("typography", &typography(line_height.clone())).is_err(),
            "must reject {line_height}"
        );
    }
}

#[test]
fn blur_extension_type_accepts_layer_and_background_blurs() {
    validate_value(
        BLUR_TYPE,
        &json!({"kind": "layer", "radius": {"value": 4, "unit": "px"}}),
    )
    .expect("layer blur");
    validate_value(
        BLUR_TYPE,
        &json!({"kind": "background", "radius": {"value": 0, "unit": "px"}}),
    )
    .expect("background blur");
    for value in [
        json!({"kind": "motion", "radius": {"value": 4, "unit": "px"}}),
        json!({"kind": "layer", "radius": 4}),
        json!({"kind": "layer"}),
        json!({"kind": "layer", "radius": {"value": 4, "unit": "px"}, "extra": true}),
    ] {
        assert!(
            validate_value(BLUR_TYPE, &value).is_err(),
            "must reject {value}"
        );
    }
    assert!(validate_value("dxn.dtcg.other", &json!(1)).is_err());
}
