//! The built-in modules `dxn.dtcg` (DTCG value helpers) and
//! `dxn.dtcg/color` (the color API of [`Color`]).

use std::sync::Arc;

use serde_json::{Value, json};

use super::native::{Export, NativeError, NativeExport, NativeFn, NativeModule};
use crate::{
    ValueExtensions, validate_value_with,
    values::{Color, ColorSpace, TypedValue},
};

/// The built-in modules, `dxn.dtcg` validating with `extensions`.
pub(crate) fn modules(extensions: &ValueExtensions) -> [NativeModule; 2] {
    [dtcg_module(extensions), color_module()]
}

/// The `dxn.dtcg` module, validating with `extensions`.
pub(crate) fn dtcg_module(extensions: &ValueExtensions) -> NativeModule {
    let extensions = extensions.clone();
    let module = NativeModule::new("dxn.dtcg");
    let module = raw(
        module,
        "px",
        "(value: number) => Dimension",
        "A `px` dimension.",
        |args| Ok(json!({ "value": number(args, 0)?, "unit": "px" })),
    );
    let module = raw(
        module,
        "rem",
        "(value: number) => Dimension",
        "A `rem` dimension.",
        |args| Ok(json!({ "value": number(args, 0)?, "unit": "rem" })),
    );
    let module = raw(
        module,
        "dimension",
        "(value: number, unit: \"px\" | \"rem\") => Dimension",
        "A dimension of `value` in `unit`.",
        |args| {
            let value = number(args, 0)?;
            let unit = unit(args, 1, &["px", "rem"])?;
            Ok(json!({ "value": value, "unit": unit }))
        },
    );
    let module = raw(
        module,
        "ms",
        "(value: number) => Duration",
        "A duration in milliseconds.",
        |args| Ok(json!({ "value": number(args, 0)?, "unit": "ms" })),
    );
    let module = raw(
        module,
        "s",
        "(value: number) => Duration",
        "A duration in seconds.",
        |args| Ok(json!({ "value": number(args, 0)?, "unit": "s" })),
    );
    let module = raw(
        module,
        "duration",
        "(value: number, unit: \"ms\" | \"s\") => Duration",
        "A duration of `value` in `unit`.",
        |args| {
            let value = number(args, 0)?;
            let unit = unit(args, 1, &["ms", "s"])?;
            Ok(json!({ "value": value, "unit": unit }))
        },
    );
    let module = raw(
        module,
        "toPx",
        "(dimension: Dimension, remBase: number) => number",
        "The length of `dimension` in pixels, with `remBase` pixels per `rem`.",
        |args| {
            let dimension = typed(args, 0, "dimension")?;
            let rem_base = number(args, 1)?;
            let TypedValue::Dimension(dimension) = dimension else {
                unreachable!()
            };
            Ok(json!(dimension.to_px(rem_base)))
        },
    );
    let module = raw(
        module,
        "toMillis",
        "(duration: Duration) => number",
        "The length of `duration` in milliseconds.",
        |args| {
            let TypedValue::Duration(duration) = typed(args, 0, "duration")? else {
                unreachable!()
            };
            Ok(json!(duration.to_millis()))
        },
    );
    let module = raw(
        module,
        "round",
        "(value: number, step?: number) => number",
        "`value` rounded to the nearest multiple of `step` (default 1).",
        |args| {
            let value = number(args, 0)?;
            let step = optional_number(args, 1)?.unwrap_or(1.0);
            if step <= 0.0 {
                return Err(type_error("step must be a positive number"));
            }
            Ok(json!((value / step).round() * step))
        },
    );
    let module = raw(
        module,
        "clamp",
        "(value: number, min: number, max: number) => number",
        "`value` limited to `[min, max]`.",
        |args| {
            let value = number(args, 0)?;
            let (min, max) = (number(args, 1)?, number(args, 2)?);
            if min > max {
                return Err(type_error("min must not exceed max"));
            }
            Ok(json!(value.clamp(min, max)))
        },
    );
    raw(
        module,
        "isValid",
        "(type: string, value: unknown) => boolean",
        "Whether `value` is a valid DTCG value of `type`, with the runtime's value extensions.",
        move |args| {
            let token_type = string(args, 0)?;
            let value = args.get(1).cloned().unwrap_or(Value::Null);
            Ok(json!(validate_value_with(&token_type, &value, &extensions).is_ok()))
        },
    )
    .with_docs("Helpers for DTCG values: dimension and duration constructors and conversions, rounding, clamping and validation.")
}

/// The `dxn.dtcg/color` module.
pub(crate) fn color_module() -> NativeModule {
    let module = NativeModule::new("dxn.dtcg/color");
    let module = raw(
        module,
        "convert",
        "(color: Color, space: ColorSpace) => Color",
        "`color` in `space`, clipped into the space's ranges.",
        |args| Ok(color(&input(args, 0)?.convert(space(args, 1)?))),
    );
    let module = raw(
        module,
        "mix",
        "(color: Color, other: Color, amount?: number, space?: ColorSpace) => Color",
        "`other` mixed into `color` by `amount` (default 0.5) in `space` (default `oklch`), with the shorter hue arc.",
        |args| {
            let first = input(args, 0)?;
            let second = input(args, 1)?;
            let amount = optional_number(args, 2)?.unwrap_or(0.5);
            let space = match args.get(3) {
                None | Some(Value::Null) => ColorSpace::Oklch,
                Some(_) => space(args, 3)?,
            };
            Ok(color(&first.mix_in(&second, amount, space)))
        },
    );
    let module = raw(
        module,
        "lighten",
        "(color: Color, amount: number) => Color",
        "`color` with its OKLCH lightness raised by `amount`.",
        |args| Ok(color(&input(args, 0)?.lighten(number(args, 1)?))),
    );
    let module = raw(
        module,
        "darken",
        "(color: Color, amount: number) => Color",
        "`color` with its OKLCH lightness lowered by `amount`.",
        |args| Ok(color(&input(args, 0)?.darken(number(args, 1)?))),
    );
    let module = raw(
        module,
        "withAlpha",
        "(color: Color, alpha: number) => Color",
        "`color` with `alpha`.",
        |args| Ok(color(&input(args, 0)?.with_alpha(number(args, 1)?))),
    );
    let module = raw(
        module,
        "toHex",
        "(color: Color) => string",
        "The `#rrggbb` sRGB form of `color`.",
        |args| Ok(json!(input(args, 0)?.to_hex())),
    );
    let module = raw(
        module,
        "fromHex",
        "(hex: string) => Color",
        "The sRGB color of a `#rrggbb` hex string.",
        |args| {
            let hex = string(args, 0)?;
            Color::from_hex(&hex)
                .map(|parsed| color(&parsed))
                .ok_or_else(|| type_error("expected a `#rrggbb` hex string"))
        },
    );
    let module = raw(
        module,
        "contrast",
        "(color: Color, other: Color) => number",
        "The WCAG 2.x contrast ratio of two colors, from 1 to 21.",
        |args| Ok(json!(input(args, 0)?.contrast(&input(args, 1)?))),
    );
    let module = raw(
        module,
        "inGamut",
        "(color: Color, space: RgbColorSpace) => boolean",
        "Whether `color` lies inside the gamut of an RGB or XYZ space.",
        |args| Ok(json!(input(args, 0)?.in_gamut(space(args, 1)?))),
    );
    raw(
        module,
        "clampToGamut",
        "(color: Color, space: RgbColorSpace) => Color",
        "`color` in an RGB or XYZ space, clipped into its gamut.",
        |args| Ok(color(&input(args, 0)?.clamp_to_gamut(space(args, 1)?))),
    )
    .with_docs("Color conversions between the DTCG color spaces and the operations of CSS Color 4.")
}

fn raw(
    module: NativeModule,
    name: &str,
    typescript: &str,
    docs: &str,
    function: impl Fn(&[Value]) -> Result<Value, NativeError> + Send + Sync + 'static,
) -> NativeModule {
    module.raw_export(NativeExport {
        name: name.into(),
        export: Export::Function(Arc::new(function) as NativeFn),
        typescript: Some(typescript.into()),
        docs: Some(docs.into()),
    })
}

fn type_error(message: &str) -> NativeError {
    NativeError::Type(message.to_owned())
}

fn number(args: &[Value], index: usize) -> Result<f64, NativeError> {
    optional_number(args, index)?
        .ok_or_else(|| type_error(&format!("argument {} must be a number", index + 1)))
}

fn optional_number(args: &[Value], index: usize) -> Result<Option<f64>, NativeError> {
    match args.get(index) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_f64()
            .map(Some)
            .ok_or_else(|| type_error(&format!("argument {} must be a number", index + 1))),
    }
}

fn string(args: &[Value], index: usize) -> Result<String, NativeError> {
    args.get(index)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| type_error(&format!("argument {} must be a string", index + 1)))
}

fn unit<'a>(args: &[Value], index: usize, units: &[&'a str]) -> Result<&'a str, NativeError> {
    let unit = string(args, index)?;
    units
        .iter()
        .copied()
        .find(|candidate| *candidate == unit)
        .ok_or_else(|| type_error(&format!("unit `{unit}` is not one of {}", units.join(", "))))
}

fn typed(args: &[Value], index: usize, token_type: &str) -> Result<TypedValue, NativeError> {
    let value = args.get(index).cloned().unwrap_or(Value::Null);
    TypedValue::from_resolved(token_type, &value, &ValueExtensions::default()).map_err(|error| {
        type_error(&format!(
            "argument {} is not a valid {token_type}: {error}",
            index + 1
        ))
    })
}

fn input(args: &[Value], index: usize) -> Result<Color, NativeError> {
    match typed(args, index, "color")? {
        TypedValue::Color(color) => Ok(color),
        _ => unreachable!("a validated color"),
    }
}

fn space(args: &[Value], index: usize) -> Result<ColorSpace, NativeError> {
    let name = string(args, index)?;
    ColorSpace::from_name(&name)
        .ok_or_else(|| type_error(&format!("`{name}` is not a DTCG color space")))
}

fn color(color: &Color) -> Value {
    color.to_json()
}
