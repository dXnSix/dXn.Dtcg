use crate::RuntimeLimits;

/// Value shapes beyond DTCG 2025.10 that a runtime may accept. Both are off by
/// default, so validation follows the specification strictly.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct ValueExtensions {
    /// Typography `lineHeight` may be a `dimension`, and a `lineHeight` alias
    /// may target a `dimension` token. RayX design-system line heights are px
    /// tokens that text styles alias.
    pub line_height_dimension: bool,
    /// The RayX `dev.rayx.blur` type (`{ "kind": "layer" | "background",
    /// "radius": <dimension> }`) for Figma blur effect styles.
    pub rayx_blur: bool,
}

impl ValueExtensions {
    /// The profile RayX and RayX.TokenEditor use: every RayX extension on.
    pub fn rayx() -> Self {
        Self {
            line_height_dimension: true,
            rayx_blur: true,
        }
    }

    /// Sets [`ValueExtensions::line_height_dimension`].
    pub fn with_line_height_dimension(mut self, enabled: bool) -> Self {
        self.line_height_dimension = enabled;
        self
    }

    /// Sets [`ValueExtensions::rayx_blur`].
    pub fn with_rayx_blur(mut self, enabled: bool) -> Self {
        self.rayx_blur = enabled;
        self
    }
}

/// How a [`DtcgRuntime`](crate::DtcgRuntime) bounds its work and validates
/// values. The default applies [`RuntimeLimits::default`] and strict DTCG
/// 2025.10 values.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct RuntimeOptions {
    pub limits: RuntimeLimits,
    pub extensions: ValueExtensions,
}

impl RuntimeOptions {
    /// Sets [`RuntimeOptions::limits`].
    pub fn with_limits(mut self, limits: RuntimeLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Sets [`RuntimeOptions::extensions`].
    pub fn with_extensions(mut self, extensions: ValueExtensions) -> Self {
        self.extensions = extensions;
        self
    }
}
