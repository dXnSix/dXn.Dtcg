//! String-backed DTCG document resolution and narrow typed projection.

mod affected_resolution;
mod catalog;
#[cfg(feature = "diagnostics")]
mod diagnostics;
mod dtcg_component_tokens;
mod dtcg_tokens;
mod envelope;
mod error;
mod flatten;
mod from_dtcg_value;
mod json_pointer;
mod local_reference;
mod resolver;
mod resolver_definition;
mod resource;
mod runtime;
mod selection;
mod source_discovery;
mod source_occurrence;
mod source_origin;
mod token_resolution;
mod type_registry;
mod value;

pub use affected_resolution::AffectedResolution;
pub use catalog::DocumentCatalog;
#[cfg(feature = "diagnostics")]
pub use diagnostics::{DtcgDiagnostic, DtcgDiagnosticCode};
pub use dtcg_component_tokens::DtcgComponentTokens;
pub use dtcg_runtime_derive::{DtcgComponentTokens, DtcgTokens};
pub use dtcg_tokens::{DtcgTokens, join_token_path};
pub use envelope::ResolverEnvelope;
pub use error::DtcgError;
pub use from_dtcg_value::FromDtcgValue;
pub use local_reference::LocalReferencePolicy;
pub use resolver::{DocumentRequest, Resolution, ResolvedToken, ResolvedTokens, RuntimeLimits};
pub use resource::{DocumentReference, ResourceId};
pub use runtime::DtcgRuntime;
pub use selection::ModifierAxis;
pub use type_registry::TypeRegistry;
pub use value::{BLUR_TYPE, ValueValidationError, validate_value};

pub use resolver_definition::{modifier_contexts, set_sources};
pub use source_occurrence::SourceOccurrence;
pub use source_origin::SourceOrigin;
mod source_reference;
mod source_references;
pub use source_reference::SourceReference;
