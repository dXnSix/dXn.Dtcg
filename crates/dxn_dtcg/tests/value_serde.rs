//! The value models serialize to their DTCG JSON form and deserialize from
//! it, validating it as their DTCG type.

use dxn_dtcg::values::{
    Border, Color, CubicBezier, Dimension, Duration, FontFamily, FontWeight, Gradient, Shadow,
    StrokeStyle, Transition, Typography,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use wasm_bindgen_test::wasm_bindgen_test;

fn color() -> Value {
    json!({ "colorSpace": "srgb", "components": [0.2, 0.4, 0.9] })
}

/// `value` deserializes, and serializes back to itself.
fn round_trip<T: Serialize + DeserializeOwned>(value: Value) {
    let model: T = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(&model).unwrap(), value);
}

/// `value` does not deserialize as `T`.
fn rejects<T: DeserializeOwned>(value: Value) {
    let error = serde_json::from_value::<T>(value.clone())
        .err()
        .unwrap_or_else(|| panic!("accepted {value}"));
    assert!(
        error.to_string().contains("does not match DTCG type"),
        "{value}: {error}"
    );
}

#[wasm_bindgen_test(unsupported = test)]
fn every_model_round_trips_its_dtcg_json() {
    round_trip::<Color>(color());
    round_trip::<Color>(
        json!({ "colorSpace": "oklch", "components": [0.5, 0.1, "none"], "alpha": 0.5 }),
    );
    round_trip::<Dimension>(json!({ "value": 4, "unit": "px" }));
    round_trip::<Duration>(json!({ "value": 0.25, "unit": "s" }));
    round_trip::<CubicBezier>(json!([0.4, 0, 0.2, 1]));
    round_trip::<FontFamily>(json!("Inter"));
    round_trip::<FontFamily>(json!(["Inter", "sans-serif"]));
    round_trip::<FontWeight>(json!(600));
    round_trip::<StrokeStyle>(json!("dashed"));
    round_trip::<StrokeStyle>(json!({
        "dashArray": [{ "value": 2, "unit": "px" }, { "value": 1, "unit": "px" }],
        "lineCap": "round"
    }));
    round_trip::<Border>(json!({
        "color": color(), "width": { "value": 1, "unit": "px" }, "style": "solid"
    }));
    round_trip::<Transition>(json!({
        "duration": { "value": 200, "unit": "ms" },
        "delay": { "value": 0, "unit": "ms" },
        "timingFunction": [0.5, 0, 1, 1]
    }));
    let layer = json!({
        "color": color(),
        "offsetX": { "value": 0, "unit": "px" },
        "offsetY": { "value": 2, "unit": "px" },
        "blur": { "value": 4, "unit": "px" },
        "spread": { "value": 0, "unit": "px" }
    });
    round_trip::<Shadow>(layer.clone());
    let mut inset = layer.clone();
    inset["inset"] = true.into();
    round_trip::<Shadow>(json!([layer, inset]));
    round_trip::<Gradient>(json!([
        { "color": color(), "position": 0 },
        { "color": color(), "position": 1 }
    ]));
    round_trip::<Typography>(json!({
        "fontFamily": "Inter",
        "fontSize": { "value": 16, "unit": "px" },
        "fontWeight": 400,
        "letterSpacing": { "value": 0, "unit": "px" },
        "lineHeight": 1.5
    }));
}

#[wasm_bindgen_test(unsupported = test)]
fn invalid_json_is_rejected_as_its_dtcg_type() {
    rejects::<Color>(json!({ "colorSpace": "srgb", "components": [2, 0, 0] }));
    rejects::<Dimension>(json!({ "value": 4, "unit": "em" }));
    rejects::<Duration>(json!({ "value": 1, "unit": "min" }));
    rejects::<CubicBezier>(json!([2, 0, 0, 1]));
    rejects::<FontFamily>(json!(4));
    rejects::<FontWeight>(json!(1200));
    rejects::<StrokeStyle>(json!("wavy"));
    rejects::<Border>(json!({ "color": color(), "style": "solid" }));
    rejects::<Transition>(json!({ "duration": { "value": 200, "unit": "ms" } }));
    rejects::<Shadow>(json!({ "color": color() }));
    rejects::<Gradient>(json!([{ "color": color() }]));
    rejects::<Typography>(json!({ "fontFamily": "Inter" }));
}

#[wasm_bindgen_test(unsupported = test)]
fn font_weight_keywords_deserialize_to_their_number() {
    let weight: FontWeight = serde_json::from_value(json!("bold")).unwrap();
    assert_eq!(serde_json::to_value(weight).unwrap(), json!(700));
}
