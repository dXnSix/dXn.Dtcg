use crate::ResolvedToken;

/// The resolved projection changed by one or more authored token paths.
#[derive(Clone, Debug, Default)]
pub struct AffectedResolution {
    pub(crate) tokens: Vec<ResolvedToken>,
    #[cfg(feature = "diagnostics")]
    pub(crate) nodes_visited: usize,
}

impl AffectedResolution {
    pub fn iter(&self) -> impl Iterator<Item = &ResolvedToken> {
        self.tokens.iter()
    }

    #[cfg(feature = "diagnostics")]
    pub fn nodes_visited(&self) -> usize {
        self.nodes_visited
    }
}
