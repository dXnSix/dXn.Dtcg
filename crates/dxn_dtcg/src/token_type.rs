//! The token types the runtime knows by their `$type` spelling: the 13 types
//! of DTCG 2025.10 (Format §8, §9) and the library's opt-in extension types.
//! This is the single table of type names; validation, typed values and
//! alias positions dispatch through it.

use std::fmt;

use crate::{ResolvedToken, ValueExtensions};

/// A token type, named as `$type` spells it. The 13 DTCG 2025.10 types are
/// always accepted; the `dxn.dtcg.*` extension types only by a runtime whose
/// [`ValueExtensions`] enable them. New variants may be added in minor
/// releases, so matches need a wildcard arm.
///
/// ```
/// use dxn_dtcg::TokenType;
///
/// assert_eq!(TokenType::from_name("cubicBezier"), Some(TokenType::CubicBezier));
/// assert_eq!(TokenType::Blur.name(), "dxn.dtcg.blur");
/// assert_eq!(TokenType::from_name("colour"), None); // names are case-sensitive
/// ```
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum TokenType {
    Color,
    Dimension,
    FontFamily,
    FontWeight,
    Duration,
    CubicBezier,
    Number,
    StrokeStyle,
    Border,
    Transition,
    Shadow,
    Gradient,
    Typography,
    /// `dxn.dtcg.blur`, accepted with [`ValueExtensions::blur`].
    Blur,
    /// `dxn.dtcg.string`, accepted with [`ValueExtensions::string`].
    String,
    /// `dxn.dtcg.boolean`, accepted with [`ValueExtensions::boolean`].
    Boolean,
}

impl TokenType {
    /// Every token type: the 13 DTCG 2025.10 types in the specification's
    /// order, then the extension types.
    pub const ALL: &'static [Self] = &[
        Self::Color,
        Self::Dimension,
        Self::FontFamily,
        Self::FontWeight,
        Self::Duration,
        Self::CubicBezier,
        Self::Number,
        Self::StrokeStyle,
        Self::Border,
        Self::Transition,
        Self::Shadow,
        Self::Gradient,
        Self::Typography,
        Self::Blur,
        Self::String,
        Self::Boolean,
    ];

    /// The `$type` spelling.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Color => "color",
            Self::Dimension => "dimension",
            Self::FontFamily => "fontFamily",
            Self::FontWeight => "fontWeight",
            Self::Duration => "duration",
            Self::CubicBezier => "cubicBezier",
            Self::Number => "number",
            Self::StrokeStyle => "strokeStyle",
            Self::Border => "border",
            Self::Transition => "transition",
            Self::Shadow => "shadow",
            Self::Gradient => "gradient",
            Self::Typography => "typography",
            Self::Blur => "dxn.dtcg.blur",
            Self::String => "dxn.dtcg.string",
            Self::Boolean => "dxn.dtcg.boolean",
        }
    }

    /// The type a `$type` spelling names, compared exactly (type names are
    /// case-sensitive). Extension types are named whatever the runtime's
    /// extensions; whether a runtime accepts them is up to its
    /// [`ValueExtensions`].
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|token_type| token_type.name() == name)
    }

    /// Whether this is a `dxn.dtcg.*` extension type rather than one of the
    /// 13 DTCG 2025.10 types.
    pub const fn is_extension(self) -> bool {
        matches!(self, Self::Blur | Self::String | Self::Boolean)
    }

    /// Whether validation with `extensions` accepts this type.
    pub(crate) fn is_enabled(self, extensions: &ValueExtensions) -> bool {
        match self {
            Self::Blur => extensions.blur,
            Self::String => extensions.string,
            Self::Boolean => extensions.boolean,
            _ => true,
        }
    }
}

/// Writes the `$type` spelling.
impl fmt::Display for TokenType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

impl ResolvedToken {
    /// The token's effective type as a [`TokenType`]: [`ResolvedToken::token_type`]
    /// by its `$type` spelling. Every resolved token has a type the runtime
    /// validated, so this is `None` only for a token without one.
    pub fn dtcg_type(&self) -> Option<TokenType> {
        self.token_type.as_deref().and_then(TokenType::from_name)
    }
}
