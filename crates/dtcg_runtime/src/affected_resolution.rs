use std::collections::BTreeMap;

use crate::{ResolvedGroup, ResolvedToken};

/// The part of a resolution an edit can change: the re-resolved tokens, the
/// removed paths, the full new token order and the groups. Apply it with
/// [`ResolvedTokens::apply_affected`](crate::ResolvedTokens::apply_affected).
#[derive(Clone, Debug, Default)]
pub struct AffectedResolution {
    pub(crate) tokens: Vec<ResolvedToken>,
    pub(crate) removed: Vec<String>,
    pub(crate) order: Vec<String>,
    pub(crate) groups: BTreeMap<String, ResolvedGroup>,
    #[cfg(feature = "diagnostics")]
    pub(crate) nodes_visited: usize,
}

impl AffectedResolution {
    /// The re-resolved tokens, in the new resolution order.
    pub fn iter(&self) -> impl Iterator<Item = &ResolvedToken> {
        self.tokens.iter()
    }

    /// The token paths the edit removed.
    pub fn removed(&self) -> &[String] {
        &self.removed
    }

    /// Every token path after the edit, in resolution order.
    pub fn order(&self) -> &[String] {
        &self.order
    }

    #[cfg(feature = "diagnostics")]
    pub fn nodes_visited(&self) -> usize {
        self.nodes_visited
    }
}
