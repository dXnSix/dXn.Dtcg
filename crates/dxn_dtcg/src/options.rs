use crate::RuntimeLimits;

/// Value shapes beyond DTCG 2025.10 that a runtime may accept. All are off by
/// default, so validation follows the specification strictly.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct ValueExtensions {
    /// Typography `lineHeight` may be a `dimension`, and a `lineHeight` alias
    /// may target a `dimension` token: design tools commonly export line
    /// heights as pixel dimensions that text styles alias.
    pub line_height_dimension: bool,
    /// The `dxn.dtcg.blur` type (`{ "kind": "layer" | "background",
    /// "radius": <dimension> }`) for layer and background blur effects, such
    /// as Figma's blur effect styles.
    pub blur: bool,
    /// The `dxn.dtcg.string` type, whose `$value` is a JSON string. DTCG
    /// 2025.10 has no string type, but design tools commonly export string
    /// variables, such as Figma's `STRING` variables, as tokens.
    pub string: bool,
    /// The `dxn.dtcg.boolean` type, whose `$value` is a JSON boolean. DTCG
    /// 2025.10 has no boolean type, but design tools commonly export boolean
    /// variables, such as Figma's `BOOLEAN` variables, as tokens.
    pub boolean: bool,
}

impl ValueExtensions {
    /// Every value extension on.
    pub fn all() -> Self {
        Self {
            line_height_dimension: true,
            blur: true,
            string: true,
            boolean: true,
        }
    }

    /// Sets [`ValueExtensions::line_height_dimension`].
    pub fn with_line_height_dimension(mut self, enabled: bool) -> Self {
        self.line_height_dimension = enabled;
        self
    }

    /// Sets [`ValueExtensions::blur`].
    pub fn with_blur(mut self, enabled: bool) -> Self {
        self.blur = enabled;
        self
    }

    /// Sets [`ValueExtensions::string`].
    pub fn with_string(mut self, enabled: bool) -> Self {
        self.string = enabled;
        self
    }

    /// Sets [`ValueExtensions::boolean`].
    pub fn with_boolean(mut self, enabled: bool) -> Self {
        self.boolean = enabled;
        self
    }
}

/// How a [`DtcgRuntime`](crate::DtcgRuntime) bounds its work and validates
/// values. The default applies [`RuntimeLimits::default`] and strict DTCG
/// 2025.10 values.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct RuntimeOptions {
    /// Bounds on the work of every resolution, discovery and inspection.
    pub limits: RuntimeLimits,
    /// The value shapes beyond DTCG 2025.10 that validation accepts.
    pub extensions: ValueExtensions,
    /// Whether and how token expressions evaluate
    /// (`$extensions["dxn.dtcg.expressions"]`); `None`, the default, leaves
    /// the extension opaque metadata.
    #[cfg(feature = "expressions")]
    pub expressions: Option<crate::expressions::ExpressionOptions>,
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

    /// Enables token expressions with `expressions`.
    #[cfg(feature = "expressions")]
    pub fn with_expressions(mut self, expressions: crate::expressions::ExpressionOptions) -> Self {
        self.expressions = Some(expressions);
        self
    }
}
