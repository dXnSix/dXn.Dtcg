use thiserror::Error;

use crate::ResourceId;

/// Every failure the runtime reports. New variants may be added in minor
/// releases, so matches need a wildcard arm.
#[derive(Clone, Debug, Error, PartialEq)]
#[non_exhaustive]
pub enum DtcgError {
    #[error("invalid logical resource id `{0}`")]
    InvalidResourceId(String),
    #[error("invalid JSON pointer `{0}`")]
    InvalidPointer(String),
    #[error("invalid JSON in `{resource}`: {message}")]
    InvalidJson {
        resource: ResourceId,
        message: String,
    },
    #[error("invalid DTCG document `{resource}`: {message}")]
    InvalidDocument {
        resource: ResourceId,
        message: String,
    },
    #[error("document `{0}` was not previously provided")]
    UnknownDocument(ResourceId),
    #[error("duplicate document `{0}` in batch")]
    DuplicateDocument(ResourceId),
    #[error("document graph exceeds {limit} {kind}")]
    LimitExceeded { kind: &'static str, limit: usize },
    #[error("document reference cycle: {0}")]
    DocumentCycle(String),
    #[error("token `{path}` in `{resource}`: {message}")]
    Token {
        resource: ResourceId,
        path: String,
        message: String,
    },
    /// A token reference that names no resolvable target: a missing alias
    /// target (including `{group}` and empty `{}` aliases), or a missing or
    /// malformed `$extends` target. `resource` and `path` name the referring
    /// token or group; `target` is the authored alias path or pointer.
    #[error("token `{path}` in `{resource}`: unresolved reference `{target}`: {message}")]
    UnresolvedReference {
        resource: ResourceId,
        path: String,
        target: String,
        message: String,
    },
    /// A resolved token that cannot be projected into the requested Rust type.
    #[error("projection `{path}`: {message}")]
    Projection { path: String, message: String },
    /// A token without a usable DTCG type, or a value that does not match it.
    #[error("token `{path}` in `{resource}`: {message}")]
    TokenType {
        resource: ResourceId,
        path: String,
        message: String,
    },
    /// A document reference outside the relative local policy.
    #[error("reference `{reference}` in `{resource}`: {message}")]
    InvalidReference {
        resource: ResourceId,
        reference: String,
        message: String,
    },
    /// An invalid or failing expression (`$extensions["dxn.dtcg.expressions"]`):
    /// a malformed expression object or module declaration, a syntax error,
    /// an uncaught exception, a result that is not a JSON value, or a module
    /// that cannot be imported. `resource` and `path` name the token (or the
    /// document and its module declaration, or the module resource).
    #[error("expression `{path}` in `{resource}`: {message}")]
    Expression {
        resource: ResourceId,
        path: String,
        message: String,
    },
    /// Resolver inputs that are not a JSON object of strings.
    #[error("invalid resolver inputs: {message}")]
    InvalidInput { message: String },
    /// A selection names a modifier no reached resolver declares, or a name
    /// that matches several declared modifiers case-insensitively (then
    /// `known` lists the candidates).
    #[error("unknown modifier `{modifier}`; known modifiers: {}", known.join(", "))]
    UnknownModifier {
        modifier: String,
        known: Vec<String>,
    },
    /// A selection names a context the modifier does not declare, selects a
    /// modifier twice, or omits a modifier without a default.
    #[error("modifier `{modifier}` context `{context}`: {message}; allowed contexts: {}", allowed.join(", "))]
    UnknownContext {
        modifier: String,
        context: String,
        allowed: Vec<String>,
        message: String,
    },
}

pub(crate) fn invalid(resource: &ResourceId, message: &str) -> DtcgError {
    DtcgError::InvalidDocument {
        resource: resource.clone(),
        message: message.into(),
    }
}
