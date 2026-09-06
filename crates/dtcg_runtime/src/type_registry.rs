use crate::{DtcgError, FromDtcgValue, ResolvedToken};

#[derive(Default)]
pub struct TypeRegistry;

impl TypeRegistry {
    pub fn convert<T: FromDtcgValue>(&self, token: &ResolvedToken) -> Result<T, DtcgError> {
        T::from_dtcg(token)
    }
}
