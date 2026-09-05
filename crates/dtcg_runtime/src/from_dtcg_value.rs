use crate::{DtcgError, ResolvedToken};

pub trait FromDtcgValue: Sized + 'static {
    fn from_dtcg(token: &ResolvedToken) -> Result<Self, DtcgError>;
}

impl FromDtcgValue for String {
    fn from_dtcg(token: &ResolvedToken) -> Result<Self, DtcgError> {
        token
            .value
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| DtcgError::Projection {
                path: token.path.to_string(),
                message: "expected string value".into(),
            })
    }
}

impl FromDtcgValue for f64 {
    fn from_dtcg(token: &ResolvedToken) -> Result<Self, DtcgError> {
        token.value.as_f64().ok_or_else(|| DtcgError::Projection {
            path: token.path.to_string(),
            message: "expected numeric value".into(),
        })
    }
}
