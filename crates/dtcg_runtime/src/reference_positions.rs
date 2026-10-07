//! Where a curly-brace alias may appear inside a token value (Format §7, §9).
//!
//! An alias is always allowed as the whole `$value`. Inside a value it is
//! allowed only at a composite member or a composite array element listed in
//! the table below, and it must then reference a token of the member's type.
//! Everywhere else (color components, dimension members, `cubicBezier`
//! elements, `fontFamily` array elements, `lineCap`, shadow `inset`, …) it is
//! rejected. JSON Pointer `{ "$ref": … }` objects are allowed at any depth;
//! their targets are not position-checked, because their resolved value is
//! validated by shape as part of the whole value.

/// The composite structure a value position belongs to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Context {
    Border,
    Transition,
    /// A shadow object, or an array of shadow objects.
    Shadow,
    /// A gradient array, or one of its stop objects.
    Gradient,
    Typography,
    StrokeStyle,
    /// A `strokeStyle` `dashArray`.
    DashArray,
    /// The RayX `dev.rayx.blur` extension type.
    Blur,
    /// A value whose members admit no alias.
    Leaf,
}

impl Context {
    pub(crate) fn of_type(token_type: &str) -> Self {
        match token_type {
            "border" => Self::Border,
            "transition" => Self::Transition,
            "shadow" => Self::Shadow,
            "gradient" => Self::Gradient,
            "typography" => Self::Typography,
            "strokeStyle" => Self::StrokeStyle,
            crate::BLUR_TYPE => Self::Blur,
            _ => Self::Leaf,
        }
    }
}

/// A position inside the value being resolved.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Position {
    /// Inside a JSON Pointer target, or a value whose type is unknown: not
    /// checked.
    Unchecked,
    /// A token's whole value: an alias is allowed, and members follow the
    /// token type's context.
    Root(Context),
    /// A composite member or element: an alias must reference one of
    /// `expected`.
    Member {
        expected: &'static [&'static str],
        context: Context,
    },
    /// Any other position: aliases are rejected.
    Inside(Context),
}

/// The step from a composite value to one of its members or elements.
pub(crate) enum Step<'a> {
    Member(&'a str),
    Element,
}

const COLOR: &[&str] = &["color"];
const DIMENSION: &[&str] = &["dimension"];
const DURATION: &[&str] = &["duration"];
const NUMBER: &[&str] = &["number"];
const NUMBER_OR_DIMENSION: &[&str] = &["number", "dimension"];

impl Position {
    /// The position of a member or element of the value at this position.
    /// `line_height_dimension` admits `dimension` targets for typography
    /// `lineHeight` (the RayX value extension).
    pub(crate) fn child(self, step: Step<'_>, line_height_dimension: bool) -> Self {
        let context = match self {
            Self::Unchecked => return Self::Unchecked,
            Self::Root(context) | Self::Inside(context) | Self::Member { context, .. } => context,
        };
        let member = |expected: &'static [&'static str], context: Context| Self::Member {
            expected,
            context,
        };
        match (context, step) {
            (Context::Border, Step::Member("color")) => member(COLOR, Context::Leaf),
            (Context::Border, Step::Member("width")) => member(DIMENSION, Context::Leaf),
            (Context::Border, Step::Member("style")) => {
                member(&["strokeStyle"], Context::StrokeStyle)
            }
            (Context::Transition, Step::Member("duration" | "delay")) => {
                member(DURATION, Context::Leaf)
            }
            (Context::Transition, Step::Member("timingFunction")) => {
                member(&["cubicBezier"], Context::Leaf)
            }
            (Context::Shadow, Step::Member("color")) => member(COLOR, Context::Leaf),
            (Context::Shadow, Step::Member("offsetX" | "offsetY" | "blur" | "spread")) => {
                member(DIMENSION, Context::Leaf)
            }
            (Context::Shadow, Step::Element) => member(&["shadow"], Context::Shadow),
            (Context::Gradient, Step::Member("color")) => member(COLOR, Context::Leaf),
            (Context::Gradient, Step::Member("position")) => member(NUMBER, Context::Leaf),
            (Context::Gradient, Step::Element) => member(&["gradient"], Context::Gradient),
            (Context::Typography, Step::Member("fontFamily")) => {
                member(&["fontFamily"], Context::Leaf)
            }
            (Context::Typography, Step::Member("fontSize" | "letterSpacing")) => {
                member(DIMENSION, Context::Leaf)
            }
            (Context::Typography, Step::Member("fontWeight")) => {
                member(&["fontWeight"], Context::Leaf)
            }
            (Context::Typography, Step::Member("lineHeight")) => member(
                if line_height_dimension {
                    NUMBER_OR_DIMENSION
                } else {
                    NUMBER
                },
                Context::Leaf,
            ),
            (Context::StrokeStyle, Step::Member("dashArray")) => Self::Inside(Context::DashArray),
            (Context::DashArray, Step::Element) => member(DIMENSION, Context::Leaf),
            (Context::Blur, Step::Member("radius")) => member(DIMENSION, Context::Leaf),
            _ => Self::Inside(Context::Leaf),
        }
    }
}
