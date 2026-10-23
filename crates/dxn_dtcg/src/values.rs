//! Typed Rust models of resolved DTCG 2025.10 values.
//!
//! The types are plain data parsed from resolved JSON that validation already
//! accepted. They apply the consumer rules of the specification: a missing
//! color `alpha` is `1` (Color §4.1), gradient stop positions are clamped to
//! `[0, 1]` (Format §9.7), font-weight keywords map to numbers (Format §8.4),
//! and a single shadow object is one layer whose `inset` defaults to `false`.
//! [`Blur`] and the dimension form of [`LineHeight`] model the RayX value
//! extensions enabled by [`ValueExtensions`].
//!
//! ```
//! use dxn_dtcg::{DtcgRuntime, ValueExtensions, values::{Color, TypedValue}};
//!
//! let runtime = DtcgRuntime::from_json(
//!     "tokens.json",
//!     r#"{"red":{"$type":"color","$value":{"colorSpace":"srgb","components":[1,0,0]}}}"#,
//! )?;
//! let tokens = runtime.resolve(&[])?.ready().unwrap();
//! let TypedValue::Color(red) = tokens.get("red").unwrap().typed(&ValueExtensions::default())? else {
//!     unreachable!()
//! };
//! assert_eq!(red.alpha, 1.0);
//! # Ok::<(), dxn_dtcg::DtcgError>(())
//! ```

use serde_json::{Map, Value};

use crate::{
    DtcgError, FromDtcgValue, ResolvedToken, ValueExtensions, ValueValidationError,
    validate_value_with,
};

/// The 14 color spaces of the Color module (Color §4.2).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ColorSpace {
    Srgb,
    SrgbLinear,
    Hsl,
    Hwb,
    Lab,
    Lch,
    Oklab,
    Oklch,
    DisplayP3,
    A98Rgb,
    ProphotoRgb,
    Rec2020,
    XyzD65,
    XyzD50,
}

impl ColorSpace {
    /// The DTCG `colorSpace` name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Srgb => "srgb",
            Self::SrgbLinear => "srgb-linear",
            Self::Hsl => "hsl",
            Self::Hwb => "hwb",
            Self::Lab => "lab",
            Self::Lch => "lch",
            Self::Oklab => "oklab",
            Self::Oklch => "oklch",
            Self::DisplayP3 => "display-p3",
            Self::A98Rgb => "a98-rgb",
            Self::ProphotoRgb => "prophoto-rgb",
            Self::Rec2020 => "rec2020",
            Self::XyzD65 => "xyz-d65",
            Self::XyzD50 => "xyz-d50",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        [
            Self::Srgb,
            Self::SrgbLinear,
            Self::Hsl,
            Self::Hwb,
            Self::Lab,
            Self::Lch,
            Self::Oklab,
            Self::Oklch,
            Self::DisplayP3,
            Self::A98Rgb,
            Self::ProphotoRgb,
            Self::Rec2020,
            Self::XyzD65,
            Self::XyzD50,
        ]
        .into_iter()
        .find(|space| space.name() == name)
    }
}

/// One color component: a number, or the `"none"` keyword.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Component {
    Number(f64),
    None,
}

/// A `color` value.
#[derive(Clone, Debug, PartialEq)]
pub struct Color {
    pub color_space: ColorSpace,
    pub components: [Component; 3],
    /// The opacity in `[0, 1]`; `1` when the value has no `alpha`.
    pub alpha: f64,
    /// The authored `#rrggbb` fallback, if any.
    pub hex: Option<String>,
}

/// The unit of a `dimension`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DimensionUnit {
    Px,
    Rem,
}

/// A `dimension` value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dimension {
    pub value: f64,
    pub unit: DimensionUnit,
}

impl Dimension {
    /// The length in pixels, converting `rem` with `rem_base` pixels per rem.
    pub fn to_px(&self, rem_base: f64) -> f64 {
        match self.unit {
            DimensionUnit::Px => self.value,
            DimensionUnit::Rem => self.value * rem_base,
        }
    }
}

/// The unit of a `duration`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DurationUnit {
    Ms,
    S,
}

/// A `duration` value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Duration {
    pub value: f64,
    pub unit: DurationUnit,
}

impl Duration {
    /// The duration in milliseconds.
    pub fn to_millis(&self) -> f64 {
        match self.unit {
            DurationUnit::Ms => self.value,
            DurationUnit::S => self.value * 1000.0,
        }
    }
}

/// A `fontFamily` value: one name, or names in fallback order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FontFamily(pub Vec<String>);

/// A `fontWeight` value in `[1, 1000]`. Keywords are mapped to their numeric
/// weights.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontWeight(pub f64);

impl FontWeight {
    /// The numeric weight of a DTCG font-weight keyword (Format §8.4).
    pub fn keyword(name: &str) -> Option<f64> {
        Some(match name {
            "thin" | "hairline" => 100.0,
            "extra-light" | "ultra-light" => 200.0,
            "light" => 300.0,
            "normal" | "regular" | "book" => 400.0,
            "medium" => 500.0,
            "semi-bold" | "demi-bold" => 600.0,
            "bold" => 700.0,
            "extra-bold" | "ultra-bold" => 800.0,
            "black" | "heavy" => 900.0,
            "extra-black" | "ultra-black" => 950.0,
            _ => return None,
        })
    }
}

/// A `cubicBezier` value: `[x1, y1, x2, y2]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBezier(pub [f64; 4]);

/// A predefined `strokeStyle` keyword.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LineStyle {
    Solid,
    Dashed,
    Dotted,
    Double,
    Groove,
    Ridge,
    Outset,
    Inset,
}

/// The `lineCap` of a dashed `strokeStyle`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LineCap {
    Round,
    Butt,
    Square,
}

/// A `strokeStyle` value: a keyword, or a dash pattern.
#[derive(Clone, Debug, PartialEq)]
pub enum StrokeStyle {
    Style(LineStyle),
    Dashes {
        dash_array: Vec<Dimension>,
        line_cap: LineCap,
    },
}

/// A `border` value.
#[derive(Clone, Debug, PartialEq)]
pub struct Border {
    pub color: Color,
    pub width: Dimension,
    pub style: StrokeStyle,
}

/// A `transition` value.
#[derive(Clone, Debug, PartialEq)]
pub struct Transition {
    pub duration: Duration,
    pub delay: Duration,
    pub timing_function: CubicBezier,
}

/// One layer of a `shadow`.
#[derive(Clone, Debug, PartialEq)]
pub struct ShadowLayer {
    pub color: Color,
    pub offset_x: Dimension,
    pub offset_y: Dimension,
    pub blur: Dimension,
    pub spread: Dimension,
    /// `false` when the value has no `inset`.
    pub inset: bool,
}

/// A `shadow` value: its layers, one for a single shadow object.
#[derive(Clone, Debug, PartialEq)]
pub struct Shadow(pub Vec<ShadowLayer>);

/// One stop of a `gradient`.
#[derive(Clone, Debug, PartialEq)]
pub struct GradientStop {
    pub color: Color,
    /// The authored position clamped to `[0, 1]`.
    pub position: f64,
}

/// A `gradient` value.
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient(pub Vec<GradientStop>);

/// A typography `lineHeight`: the specification's multiplier of the font
/// size, or, with [`ValueExtensions::line_height_dimension`], a length.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineHeight {
    Multiplier(f64),
    Dimension(Dimension),
}

/// A `typography` value.
#[derive(Clone, Debug, PartialEq)]
pub struct Typography {
    pub font_family: FontFamily,
    pub font_size: Dimension,
    pub font_weight: FontWeight,
    pub letter_spacing: Dimension,
    pub line_height: LineHeight,
}

/// The kind of a RayX blur effect.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BlurKind {
    Layer,
    Background,
}

/// A RayX `dev.rayx.blur` value ([`ValueExtensions::rayx_blur`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blur {
    pub kind: BlurKind,
    pub radius: Dimension,
}

/// A resolved value converted by its DTCG type.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum TypedValue {
    Color(Color),
    Dimension(Dimension),
    FontFamily(FontFamily),
    FontWeight(FontWeight),
    Duration(Duration),
    CubicBezier(CubicBezier),
    Number(f64),
    StrokeStyle(StrokeStyle),
    Border(Border),
    Transition(Transition),
    Shadow(Shadow),
    Gradient(Gradient),
    Typography(Typography),
    Blur(Blur),
}

impl TypedValue {
    /// Validates `value` as `token_type` with `extensions`, then converts it.
    pub fn from_resolved(
        token_type: &str,
        value: &Value,
        extensions: &ValueExtensions,
    ) -> Result<Self, ValueValidationError> {
        validate_value_with(token_type, value, extensions)?;
        Ok(match token_type {
            "color" => Self::Color(color(value)),
            "dimension" => Self::Dimension(dimension(value)),
            "fontFamily" => Self::FontFamily(font_family(value)),
            "fontWeight" => Self::FontWeight(font_weight(value)),
            "duration" => Self::Duration(duration(value)),
            "cubicBezier" => Self::CubicBezier(cubic_bezier(value)),
            "number" => Self::Number(value.as_f64().expect("validated number")),
            "strokeStyle" => Self::StrokeStyle(stroke_style(value)),
            "border" => Self::Border(border(value)),
            "transition" => Self::Transition(transition(value)),
            "shadow" => Self::Shadow(shadow(value)),
            "gradient" => Self::Gradient(gradient(value)),
            "typography" => Self::Typography(typography(value)),
            crate::BLUR_TYPE => Self::Blur(blur(value)),
            other => return Err(ValueValidationError::UnsupportedType(other.into())),
        })
    }
}

impl ResolvedToken {
    /// Converts the resolved value by the token's effective type, validating
    /// it with `extensions`.
    pub fn typed(&self, extensions: &ValueExtensions) -> Result<TypedValue, DtcgError> {
        let token_type = self
            .token_type
            .as_deref()
            .ok_or_else(|| projection(self, "token has no DTCG type".into()))?;
        TypedValue::from_resolved(token_type, &self.value, extensions)
            .map_err(|error| projection(self, error.to_string()))
    }
}

fn projection(token: &ResolvedToken, message: String) -> DtcgError {
    DtcgError::Projection {
        path: token.path.to_string(),
        message,
    }
}

/// Implements `FromDtcgValue` for a model type: the value must have the
/// shape of `token_type` (the RayX extension shapes are accepted, because the
/// runtime already validated the value with its own extensions).
macro_rules! from_dtcg_value {
    ($model:ty, $token_type:expr, $parse:expr) => {
        impl FromDtcgValue for $model {
            fn from_dtcg(token: &ResolvedToken) -> Result<Self, DtcgError> {
                validate_value_with($token_type, &token.value, &ValueExtensions::rayx())
                    .map_err(|_| projection(token, format!("expected a {} value", $token_type)))?;
                Ok($parse(&token.value))
            }
        }
    };
}

from_dtcg_value!(Color, "color", color);
from_dtcg_value!(Dimension, "dimension", dimension);
from_dtcg_value!(FontFamily, "fontFamily", font_family);
from_dtcg_value!(FontWeight, "fontWeight", font_weight);
from_dtcg_value!(Duration, "duration", duration);
from_dtcg_value!(CubicBezier, "cubicBezier", cubic_bezier);
from_dtcg_value!(StrokeStyle, "strokeStyle", stroke_style);
from_dtcg_value!(Border, "border", border);
from_dtcg_value!(Transition, "transition", transition);
from_dtcg_value!(Shadow, "shadow", shadow);
from_dtcg_value!(Gradient, "gradient", gradient);
from_dtcg_value!(Typography, "typography", typography);
from_dtcg_value!(Blur, crate::BLUR_TYPE, blur);

impl FromDtcgValue for LineHeight {
    fn from_dtcg(token: &ResolvedToken) -> Result<Self, DtcgError> {
        if let Some(multiplier) = token.value.as_f64() {
            return Ok(Self::Multiplier(multiplier));
        }
        validate_value_with("dimension", &token.value, &ValueExtensions::default())
            .map_err(|_| projection(token, "expected a number or dimension value".into()))?;
        Ok(Self::Dimension(dimension(&token.value)))
    }
}

impl FromDtcgValue for TypedValue {
    fn from_dtcg(token: &ResolvedToken) -> Result<Self, DtcgError> {
        token.typed(&ValueExtensions::rayx())
    }
}

// The parsers below read values that validation already accepted.

fn object(value: &Value) -> &Map<String, Value> {
    value.as_object().expect("validated object")
}

fn number(value: &Value) -> f64 {
    value.as_f64().expect("validated number")
}

fn text(value: &Value) -> &str {
    value.as_str().expect("validated string")
}

fn color(value: &Value) -> Color {
    let object = object(value);
    let components = object["components"]
        .as_array()
        .expect("validated components");
    let component = |index: usize| match components[index].as_f64() {
        Some(number) => Component::Number(number),
        None => Component::None,
    };
    Color {
        color_space: ColorSpace::parse(text(&object["colorSpace"])).expect("validated space"),
        components: [component(0), component(1), component(2)],
        alpha: object.get("alpha").map_or(1.0, number),
        hex: object.get("hex").map(|hex| text(hex).to_owned()),
    }
}

fn dimension(value: &Value) -> Dimension {
    let object = object(value);
    Dimension {
        value: number(&object["value"]),
        unit: match text(&object["unit"]) {
            "px" => DimensionUnit::Px,
            _ => DimensionUnit::Rem,
        },
    }
}

fn duration(value: &Value) -> Duration {
    let object = object(value);
    Duration {
        value: number(&object["value"]),
        unit: match text(&object["unit"]) {
            "ms" => DurationUnit::Ms,
            _ => DurationUnit::S,
        },
    }
}

fn font_family(value: &Value) -> FontFamily {
    FontFamily(match value {
        Value::Array(names) => names.iter().map(|name| text(name).to_owned()).collect(),
        name => vec![text(name).to_owned()],
    })
}

fn font_weight(value: &Value) -> FontWeight {
    FontWeight(
        value
            .as_f64()
            .unwrap_or_else(|| FontWeight::keyword(text(value)).expect("validated keyword")),
    )
}

fn cubic_bezier(value: &Value) -> CubicBezier {
    let values = value.as_array().expect("validated array");
    CubicBezier([
        number(&values[0]),
        number(&values[1]),
        number(&values[2]),
        number(&values[3]),
    ])
}

fn stroke_style(value: &Value) -> StrokeStyle {
    if let Some(keyword) = value.as_str() {
        return StrokeStyle::Style(match keyword {
            "solid" => LineStyle::Solid,
            "dashed" => LineStyle::Dashed,
            "dotted" => LineStyle::Dotted,
            "double" => LineStyle::Double,
            "groove" => LineStyle::Groove,
            "ridge" => LineStyle::Ridge,
            "outset" => LineStyle::Outset,
            _ => LineStyle::Inset,
        });
    }
    let object = object(value);
    StrokeStyle::Dashes {
        dash_array: object["dashArray"]
            .as_array()
            .expect("validated dash array")
            .iter()
            .map(dimension)
            .collect(),
        line_cap: match text(&object["lineCap"]) {
            "round" => LineCap::Round,
            "butt" => LineCap::Butt,
            _ => LineCap::Square,
        },
    }
}

fn border(value: &Value) -> Border {
    let object = object(value);
    Border {
        color: color(&object["color"]),
        width: dimension(&object["width"]),
        style: stroke_style(&object["style"]),
    }
}

fn transition(value: &Value) -> Transition {
    let object = object(value);
    Transition {
        duration: duration(&object["duration"]),
        delay: duration(&object["delay"]),
        timing_function: cubic_bezier(&object["timingFunction"]),
    }
}

fn shadow_layer(value: &Value) -> ShadowLayer {
    let object = object(value);
    ShadowLayer {
        color: color(&object["color"]),
        offset_x: dimension(&object["offsetX"]),
        offset_y: dimension(&object["offsetY"]),
        blur: dimension(&object["blur"]),
        spread: dimension(&object["spread"]),
        inset: object
            .get("inset")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

fn shadow(value: &Value) -> Shadow {
    Shadow(match value {
        Value::Array(layers) => layers.iter().map(shadow_layer).collect(),
        layer => vec![shadow_layer(layer)],
    })
}

fn gradient(value: &Value) -> Gradient {
    Gradient(
        value
            .as_array()
            .expect("validated stops")
            .iter()
            .map(|stop| {
                let object = object(stop);
                GradientStop {
                    color: color(&object["color"]),
                    position: number(&object["position"]).clamp(0.0, 1.0),
                }
            })
            .collect(),
    )
}

fn typography(value: &Value) -> Typography {
    let object = object(value);
    let line_height = &object["lineHeight"];
    Typography {
        font_family: font_family(&object["fontFamily"]),
        font_size: dimension(&object["fontSize"]),
        font_weight: font_weight(&object["fontWeight"]),
        letter_spacing: dimension(&object["letterSpacing"]),
        line_height: match line_height.as_f64() {
            Some(multiplier) => LineHeight::Multiplier(multiplier),
            None => LineHeight::Dimension(dimension(line_height)),
        },
    }
}

fn blur(value: &Value) -> Blur {
    let object = object(value);
    Blur {
        kind: match text(&object["kind"]) {
            "layer" => BlurKind::Layer,
            _ => BlurKind::Background,
        },
        radius: dimension(&object["radius"]),
    }
}
