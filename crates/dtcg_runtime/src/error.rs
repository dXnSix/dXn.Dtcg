use thiserror::Error;

use crate::ResourceId;

#[derive(Debug, Error)]
pub enum DtcgError {
    #[error("invalid logical resource id `{0}")]
    InvalidResourceId(String),
    #[error("invalid JSON pointer `{0}")]
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
    #[error("projection `{path}`: {message}")]
    Projection { path: String, message: String },
}

pub(crate) fn invalid(resource: &ResourceId, message: &str) -> DtcgError {
    DtcgError::InvalidDocument {
        resource: resource.clone(),
        message: message.into(),
    }
}
