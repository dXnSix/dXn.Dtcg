use std::fmt;

use serde_json::{Map, Value};

/// A final DTCG 2025.10 token-value validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValueValidationError {
    UnsupportedType(String),
    InvalidValue(String),
}

impl fmt::Display for ValueValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedType(token_type) => write!(
                formatter,
                "unsupported DTCG type `{token_type}` for final 2025.10"
            ),
            Self::InvalidValue(token_type) => write!(
                formatter,
                "resolved value does not match DTCG type `{token_type}`"
            ),
        }
    }
}

impl std::error::Error for ValueValidationError {}

/// Validates a resolved value against one of the 13 types in DTCG 2025.10.
pub fn validate_value(token_type: &str, value: &Value) -> Result<(), ValueValidationError> {
    let valid = match token_type {
        "color" => color(value),
        "dimension" => dimension(value),
        "fontFamily" => font_family(value),
        "fontWeight" => font_weight(value),
        "duration" => duration(value),
        "cubicBezier" => cubic_bezier(value),
        "number" => value.is_number(),
        "strokeStyle" => stroke_style(value),
        "border" => border(value),
        "transition" => transition(value),
        "shadow" => shadow(value),
        "gradient" => gradient(value),
        "typography" => typography(value),
        unsupported => {
            return Err(ValueValidationError::UnsupportedType(unsupported.into()));
        }
    };
    valid
        .then_some(())
        .ok_or_else(|| ValueValidationError::InvalidValue(token_type.into()))
}

fn color(value: &Value) -> bool {
    let Some(object) = exact_object(value, &["colorSpace", "components"], &["alpha", "hex"]) else {
        return false;
    };
    let Some(space) = object.get("colorSpace").and_then(Value::as_str) else {
        return false;
    };
    let Some(components) = object.get("components").and_then(Value::as_array) else {
        return false;
    };
    if components.len() != 3 {
        return false;
    }
    let ranges = match space {
        "srgb" | "srgb-linear" | "display-p3" | "a98-rgb" | "prophoto-rgb" | "rec2020"
        | "xyz-d65" | "xyz-d50" => [(Some(0.0), Some(1.0), true); 3],
        "hsl" | "hwb" => [
            (Some(0.0), Some(360.0), false),
            (Some(0.0), Some(100.0), true),
            (Some(0.0), Some(100.0), true),
        ],
        "lab" => [
            (Some(0.0), Some(100.0), true),
            (None, None, true),
            (None, None, true),
        ],
        "lch" => [
            (Some(0.0), Some(100.0), true),
            (Some(0.0), None, true),
            (Some(0.0), Some(360.0), false),
        ],
        "oklab" => [
            (Some(0.0), Some(1.0), true),
            (None, None, true),
            (None, None, true),
        ],
        "oklch" => [
            (Some(0.0), Some(1.0), true),
            (Some(0.0), None, true),
            (Some(0.0), Some(360.0), false),
        ],
        _ => return false,
    };
    if !components
        .iter()
        .zip(ranges)
        .all(|(component, (minimum, maximum, inclusive_maximum))| {
            component.as_str() == Some("none")
                || component.as_f64().is_some_and(|value| {
                    minimum.is_none_or(|minimum| value >= minimum)
                        && maximum.is_none_or(|maximum| {
                            if inclusive_maximum {
                                value <= maximum
                            } else {
                                value < maximum
                            }
                        })
                })
        })
    {
        return false;
    }
    if object.get("alpha").is_some_and(|alpha| {
        !alpha
            .as_f64()
            .is_some_and(|alpha| (0.0..=1.0).contains(&alpha))
    }) {
        return false;
    }
    object.get("hex").is_none_or(|hex| {
        hex.as_str().is_some_and(|hex| {
            hex.len() == 7
                && hex.starts_with('#')
                && hex[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
        })
    })
}

fn dimension(value: &Value) -> bool {
    measured(value, &["px", "rem"])
}

fn duration(value: &Value) -> bool {
    measured(value, &["ms", "s"])
}

fn measured(value: &Value, units: &[&str]) -> bool {
    exact_object(value, &["value", "unit"], &[]).is_some_and(|object| {
        object.get("value").is_some_and(Value::is_number)
            && object
                .get("unit")
                .and_then(Value::as_str)
                .is_some_and(|unit| units.contains(&unit))
    })
}

fn font_family(value: &Value) -> bool {
    value.is_string()
        || value
            .as_array()
            .is_some_and(|families| !families.is_empty() && families.iter().all(Value::is_string))
}

fn font_weight(value: &Value) -> bool {
    value
        .as_f64()
        .is_some_and(|weight| (1.0..=1000.0).contains(&weight))
        || value.as_str().is_some_and(|weight| {
            matches!(
                weight,
                "thin"
                    | "hairline"
                    | "extra-light"
                    | "ultra-light"
                    | "light"
                    | "normal"
                    | "regular"
                    | "book"
                    | "medium"
                    | "semi-bold"
                    | "demi-bold"
                    | "bold"
                    | "extra-bold"
                    | "ultra-bold"
                    | "black"
                    | "heavy"
                    | "extra-black"
                    | "ultra-black"
            )
        })
}

fn cubic_bezier(value: &Value) -> bool {
    value.as_array().is_some_and(|values| {
        values.len() == 4
            && values.iter().all(Value::is_number)
            && values[0]
                .as_f64()
                .is_some_and(|value| (0.0..=1.0).contains(&value))
            && values[2]
                .as_f64()
                .is_some_and(|value| (0.0..=1.0).contains(&value))
    })
}

fn stroke_style(value: &Value) -> bool {
    value.as_str().is_some_and(|style| {
        matches!(
            style,
            "solid" | "dashed" | "dotted" | "double" | "groove" | "ridge" | "outset" | "inset"
        )
    }) || exact_object(value, &["dashArray", "lineCap"], &[]).is_some_and(|object| {
        object
            .get("dashArray")
            .and_then(Value::as_array)
            .is_some_and(|values| !values.is_empty() && values.iter().all(dimension))
            && object
                .get("lineCap")
                .and_then(Value::as_str)
                .is_some_and(|line_cap| matches!(line_cap, "round" | "butt" | "square"))
    })
}

fn border(value: &Value) -> bool {
    exact_object(value, &["color", "width", "style"], &[]).is_some_and(|object| {
        color(&object["color"]) && dimension(&object["width"]) && stroke_style(&object["style"])
    })
}

fn transition(value: &Value) -> bool {
    exact_object(value, &["duration", "delay", "timingFunction"], &[]).is_some_and(|object| {
        duration(&object["duration"])
            && duration(&object["delay"])
            && cubic_bezier(&object["timingFunction"])
    })
}

fn shadow(value: &Value) -> bool {
    shadow_object(value)
        || value
            .as_array()
            .is_some_and(|values| !values.is_empty() && values.iter().all(shadow_object))
}

fn shadow_object(value: &Value) -> bool {
    exact_object(
        value,
        &["color", "offsetX", "offsetY", "blur", "spread"],
        &["inset"],
    )
    .is_some_and(|object| {
        color(&object["color"])
            && dimension(&object["offsetX"])
            && dimension(&object["offsetY"])
            && dimension(&object["blur"])
            && dimension(&object["spread"])
            && object.get("inset").is_none_or(Value::is_boolean)
    })
}

fn gradient(value: &Value) -> bool {
    value.as_array().is_some_and(|stops| {
        !stops.is_empty()
            && stops.iter().all(|stop| {
                exact_object(stop, &["color", "position"], &[])
                    .is_some_and(|object| color(&object["color"]) && object["position"].is_number())
            })
    })
}

fn typography(value: &Value) -> bool {
    exact_object(
        value,
        &[
            "fontFamily",
            "fontSize",
            "fontWeight",
            "letterSpacing",
            "lineHeight",
        ],
        &[],
    )
    .is_some_and(|object| {
        font_family(&object["fontFamily"])
            && dimension(&object["fontSize"])
            && font_weight(&object["fontWeight"])
            && dimension(&object["letterSpacing"])
            && object["lineHeight"].is_number()
    })
}

fn exact_object<'a>(
    value: &'a Value,
    required: &[&str],
    optional: &[&str],
) -> Option<&'a Map<String, Value>> {
    let object = value.as_object()?;
    required
        .iter()
        .all(|key| object.contains_key(*key))
        .then_some(())?;
    object
        .keys()
        .all(|key| required.contains(&key.as_str()) || optional.contains(&key.as_str()))
        .then_some(object)
}
