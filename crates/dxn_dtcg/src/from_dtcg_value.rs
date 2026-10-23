use crate::{DtcgError, ResolvedToken};

/// Converts one resolved token into a Rust value. A value of the wrong shape
/// fails with [`DtcgError::Projection`] naming the token path. The typed models
/// of [`crate::values`] implement it, as do `String`, `f64`, `f32` and `bool`.
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

impl FromDtcgValue for f32 {
    /// Fails for a number outside the finite `f32` range instead of producing
    /// an infinity.
    fn from_dtcg(token: &ResolvedToken) -> Result<Self, DtcgError> {
        let value = f64::from_dtcg(token)?;
        let narrowed = value as f32;
        if narrowed.is_finite() {
            Ok(narrowed)
        } else {
            Err(DtcgError::Projection {
                path: token.path.to_string(),
                message: "expected numeric value within f32 range".into(),
            })
        }
    }
}

impl FromDtcgValue for bool {
    fn from_dtcg(token: &ResolvedToken) -> Result<Self, DtcgError> {
        token.value.as_bool().ok_or_else(|| DtcgError::Projection {
            path: token.path.to_string(),
            message: "expected boolean value".into(),
        })
    }
}
