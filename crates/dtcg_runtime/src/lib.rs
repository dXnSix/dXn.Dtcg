//! String-backed DTCG document resolution and narrow typed projection.

mod affected_resolution;
mod catalog;
#[cfg(feature = "rayx_diagnostics")]
mod diagnostics;
mod dtcg_component_tokens;
mod dtcg_tokens;
mod error;
mod from_dtcg_value;
mod resolver;
mod resource;
mod runtime;
mod type_registry;
mod value;

pub use affected_resolution::AffectedResolution;
pub use catalog::DocumentCatalog;
#[cfg(feature = "rayx_diagnostics")]
pub use diagnostics::{DtcgDiagnostic, DtcgDiagnosticCode};
pub use dtcg_component_tokens::DtcgComponentTokens;
pub use dtcg_runtime_derive::{DtcgComponentTokens, DtcgTokens};
pub use dtcg_tokens::DtcgTokens;
pub use error::DtcgError;
pub use from_dtcg_value::FromDtcgValue;
pub use resolver::{DocumentRequest, Resolution, ResolvedToken, ResolvedTokens, RuntimeLimits};
pub use resource::{DocumentReference, ResourceId};
pub use runtime::DtcgRuntime;
pub use type_registry::TypeRegistry;
pub use value::{ValueValidationError, validate_value};
