//! Opt-in, redacted diagnostics for Test Suite and DevKit harnesses.
//!
//! This module is intentionally compiled only with `rayx_diagnostics`. It
//! exposes logical identifiers and paths, never supplied document bodies.

use crate::{DtcgError, ResourceId};

/// A stable category for a DTCG failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DtcgDiagnosticCode {
    InvalidResourceId,
    InvalidPointer,
    InvalidJson,
    InvalidDocument,
    UnknownDocument,
    DuplicateDocument,
    LimitExceeded,
    DocumentCycle,
    Token,
    Projection,
}

/// Redacted diagnostic information derived from a [`DtcgError`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DtcgDiagnostic {
    code: DtcgDiagnosticCode,
    resource: Option<ResourceId>,
    path: Option<String>,
    message: String,
}

impl DtcgDiagnostic {
    /// Creates a diagnostic without retaining document text or JSON values.
    pub fn from_error(error: &DtcgError) -> Self {
        let (code, resource, path, message) = match error {
            DtcgError::InvalidResourceId(message) => (
                DtcgDiagnosticCode::InvalidResourceId,
                None,
                None,
                message.clone(),
            ),
            DtcgError::InvalidPointer(message) => (
                DtcgDiagnosticCode::InvalidPointer,
                None,
                None,
                message.clone(),
            ),
            DtcgError::InvalidJson { resource, message } => (
                DtcgDiagnosticCode::InvalidJson,
                Some(resource.clone()),
                None,
                message.clone(),
            ),
            DtcgError::InvalidDocument { resource, message } => (
                DtcgDiagnosticCode::InvalidDocument,
                Some(resource.clone()),
                None,
                message.clone(),
            ),
            DtcgError::UnknownDocument(resource) => (
                DtcgDiagnosticCode::UnknownDocument,
                Some(resource.clone()),
                None,
                String::new(),
            ),
            DtcgError::DuplicateDocument(resource) => (
                DtcgDiagnosticCode::DuplicateDocument,
                Some(resource.clone()),
                None,
                String::new(),
            ),
            DtcgError::LimitExceeded { kind, limit } => (
                DtcgDiagnosticCode::LimitExceeded,
                None,
                Some((*kind).into()),
                limit.to_string(),
            ),
            DtcgError::DocumentCycle(path) => (
                DtcgDiagnosticCode::DocumentCycle,
                None,
                Some(path.clone()),
                String::new(),
            ),
            DtcgError::Token {
                resource,
                path,
                message,
            }
            | DtcgError::TokenType {
                resource,
                path,
                message,
            } => (
                DtcgDiagnosticCode::Token,
                Some(resource.clone()),
                Some(path.clone()),
                message.clone(),
            ),
            DtcgError::Projection { path, message } => (
                DtcgDiagnosticCode::Projection,
                None,
                Some(path.clone()),
                message.clone(),
            ),
        };
        Self {
            code,
            resource,
            path,
            message,
        }
    }

    pub fn code(&self) -> DtcgDiagnosticCode {
        self.code
    }

    pub fn resource(&self) -> Option<&ResourceId> {
        self.resource.as_ref()
    }

    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}
