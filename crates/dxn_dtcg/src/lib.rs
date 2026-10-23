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
mod options;
mod reference_positions;
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
pub mod values;

pub use affected_resolution::AffectedResolution;
pub use catalog::DocumentCatalog;
#[cfg(feature = "diagnostics")]
pub use diagnostics::{DtcgDiagnostic, DtcgDiagnosticCode};
pub use dtcg_component_tokens::DtcgComponentTokens;
pub use dtcg_tokens::{DtcgTokens, join_token_path};
pub use dxn_dtcg_derive::{DtcgComponentTokens, DtcgTokens};
pub use envelope::ResolverEnvelope;
pub use error::DtcgError;
pub use from_dtcg_value::FromDtcgValue;
pub use local_reference::LocalReferencePolicy;
pub use options::{RuntimeOptions, ValueExtensions};
pub use resolver::{
    Deprecation, DocumentRequest, Resolution, ResolvedGroup, ResolvedToken, ResolvedTokens,
    RuntimeLimits,
};
pub use resource::{DocumentReference, ResourceId};
pub use runtime::DtcgRuntime;
pub use selection::{ModifierAxis, selection_from_json, selection_refs};
pub use type_registry::TypeRegistry;
pub use value::{BLUR_TYPE, ValueValidationError, validate_value, validate_value_with};

pub use resolver_definition::{modifier_contexts, set_sources};
pub use source_occurrence::SourceOccurrence;
pub use source_origin::SourceOrigin;
mod source_reference;
mod source_references;
pub use source_reference::SourceReference;

/// The README and the user guide, compiled as doctests: every `rust` block is
/// compiled and run, every `rust,no_run` block is compiled. Only rustdoc's
/// doctest pass sees these modules.
#[cfg(doctest)]
mod documentation {
    #[doc = include_str!("../../../README.md")]
    mod readme {}
    #[doc = include_str!("../../../docs/README.md")]
    mod guide_index {}
    #[doc = include_str!("../../../docs/01-overview.md")]
    mod overview {}
    #[doc = include_str!("../../../docs/02-getting-started.md")]
    mod getting_started {}
    #[doc = include_str!("../../../docs/03-token-documents.md")]
    mod token_documents {}
    #[doc = include_str!("../../../docs/04-aliases-and-references.md")]
    mod aliases_and_references {}
    #[doc = include_str!("../../../docs/05-value-types.md")]
    mod value_types {}
    #[doc = include_str!("../../../docs/06-resolvers-and-modifiers.md")]
    mod resolvers_and_modifiers {}
    #[doc = include_str!("../../../docs/07-document-loading.md")]
    mod document_loading {}
    #[doc = include_str!("../../../docs/08-typed-projection.md")]
    mod typed_projection {}
    #[doc = include_str!("../../../docs/09-incremental-resolution.md")]
    mod incremental_resolution {}
    #[doc = include_str!("../../../docs/10-source-inspection.md")]
    mod source_inspection {}
    #[doc = include_str!("../../../docs/11-errors-limits-and-diagnostics.md")]
    mod errors_limits_and_diagnostics {}
    #[doc = include_str!("../../../docs/12-conformance.md")]
    mod conformance {}
    #[doc = include_str!("../../../docs/13-api-reference.md")]
    mod api_reference {}
    #[doc = include_str!("../../../docs/14-architecture-and-contributing.md")]
    mod architecture_and_contributing {}
}
