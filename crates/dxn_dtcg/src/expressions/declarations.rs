//! TypeScript declarations of the DTCG value types and of every native
//! module, rendered from the same registrations the runtime uses.

use std::fmt::Write;

use super::native::{Export, NativeModule};

/// The DTCG 2025.10 value types, as expressions see them.
const VALUE_TYPES: &str = r#"/** A DTCG color space (Color §4.2). */
type ColorSpace =
  | "srgb" | "srgb-linear" | "hsl" | "hwb" | "lab" | "lch" | "oklab" | "oklch"
  | "display-p3" | "a98-rgb" | "prophoto-rgb" | "rec2020" | "xyz-d65" | "xyz-d50";
/** A color space bounded by the unit cube: the RGB spaces and XYZ. */
type RgbColorSpace =
  | "srgb" | "srgb-linear" | "display-p3" | "a98-rgb" | "prophoto-rgb" | "rec2020"
  | "xyz-d65" | "xyz-d50";
/** A `color` value. */
interface Color {
  colorSpace: ColorSpace;
  components: [number | "none", number | "none", number | "none"];
  alpha?: number;
  hex?: string;
}
/** A `dimension` value. */
interface Dimension { value: number; unit: "px" | "rem" }
/** A `duration` value. */
interface Duration { value: number; unit: "ms" | "s" }
/** A `cubicBezier` value. */
type CubicBezier = [number, number, number, number];
/** A `fontFamily` value. */
type FontFamily = string | string[];
/** A `fontWeight` value. */
type FontWeight = number | string;
/** A `strokeStyle` value. */
type StrokeStyle =
  | "solid" | "dashed" | "dotted" | "double" | "groove" | "ridge" | "outset" | "inset"
  | { dashArray: Dimension[]; lineCap: "round" | "butt" | "square" };
/** A `border` value. */
interface Border { color: Color; width: Dimension; style: StrokeStyle }
/** A `transition` value. */
interface Transition { duration: Duration; delay: Duration; timingFunction: CubicBezier }
/** One `shadow` layer. */
interface ShadowLayer {
  color: Color;
  offsetX: Dimension;
  offsetY: Dimension;
  blur: Dimension;
  spread: Dimension;
  inset?: boolean;
}
/** A `shadow` value. */
type Shadow = ShadowLayer | ShadowLayer[];
/** One `gradient` stop. */
interface GradientStop { color: Color; position: number }
/** A `gradient` value. */
type Gradient = GradientStop[];
/** A `typography` value. */
interface Typography {
  fontFamily: FontFamily;
  fontSize: Dimension;
  fontWeight: FontWeight;
  letterSpacing: Dimension;
  lineHeight: number;
}
"#;

/// The declarations of the value types and of `modules`, in order.
pub(crate) fn render<'a>(modules: impl IntoIterator<Item = &'a NativeModule>) -> String {
    let mut out = String::from("// TypeScript declarations for dxn.dtcg expressions.\n\n");
    out.push_str(VALUE_TYPES);
    for module in modules {
        out.push('\n');
        if let Some(docs) = module.docs() {
            write_docs(&mut out, docs, "");
        }
        let _ = writeln!(out, "declare module {} {{", quoted(module.specifier()));
        for export in module.exports() {
            if let Some(docs) = &export.docs {
                write_docs(&mut out, docs, "  ");
            }
            match &export.export {
                Export::Function(_) => {
                    let signature = export
                        .typescript
                        .as_deref()
                        .unwrap_or("(...args: unknown[]) => unknown");
                    let _ = writeln!(out, "  export const {}: {signature};", export.name);
                }
                Export::Constant(value) => {
                    let declared = export
                        .typescript
                        .clone()
                        .unwrap_or_else(|| literal_type(value));
                    let _ = writeln!(out, "  export const {}: {declared};", export.name);
                }
            }
        }
        out.push_str("}\n");
    }
    out
}

fn quoted(text: &str) -> String {
    serde_json::Value::String(text.to_owned()).to_string()
}

/// A TSDoc comment.
fn write_docs(out: &mut String, docs: &str, indent: &str) {
    let _ = writeln!(out, "{indent}/**");
    for line in docs.lines() {
        let line = line.replace("*/", "*\\/");
        if line.is_empty() {
            let _ = writeln!(out, "{indent} *");
        } else {
            let _ = writeln!(out, "{indent} * {line}");
        }
    }
    let _ = writeln!(out, "{indent} */");
}

/// The TypeScript type of a JSON constant.
fn literal_type(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".into(),
        serde_json::Value::Bool(_) => "boolean".into(),
        serde_json::Value::Number(_) => "number".into(),
        serde_json::Value::String(_) => "string".into(),
        serde_json::Value::Array(_) => "readonly unknown[]".into(),
        serde_json::Value::Object(_) => "{ readonly [key: string]: unknown }".into(),
    }
}
