use crate::{DtcgError, ResolvedTokens, TypeRegistry};

pub trait DtcgTokens: Sized {
    /// Projects the value rooted at `root` (a dotted token path; empty for the
    /// document root). Shared struct types are projected at several roots.
    fn project_at(
        tokens: &ResolvedTokens,
        registry: &TypeRegistry,
        root: &str,
    ) -> Result<Self, DtcgError>;

    fn project(tokens: &ResolvedTokens, registry: &TypeRegistry) -> Result<Self, DtcgError> {
        Self::project_at(tokens, registry, "")
    }
}

/// Joins two dotted token paths, ignoring empty parts.
pub fn join_token_path(root: &str, path: &str) -> String {
    match (root.is_empty(), path.is_empty()) {
        (true, _) => path.to_owned(),
        (_, true) => root.to_owned(),
        _ => format!("{root}.{path}"),
    }
}
