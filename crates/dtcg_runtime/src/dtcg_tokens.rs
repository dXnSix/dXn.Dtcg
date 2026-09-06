use crate::{DtcgError, ResolvedTokens, TypeRegistry};

pub trait DtcgTokens: Sized {
    fn project(tokens: &ResolvedTokens, registry: &TypeRegistry) -> Result<Self, DtcgError>;
}
