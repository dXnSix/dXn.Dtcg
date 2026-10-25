//! Opt-in, redacted diagnostics for Test Suite and DevKit harnesses.
//!
//! This module is intentionally compiled only with the `diagnostics` feature. It
//! exposes logical identifiers and paths, never supplied document bodies.

use crate::{DtcgError, ResourceId};

/// A stable category for a DTCG failure. New codes may be added together with
/// new [`DtcgError`] variants.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
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
    /// [`DtcgError::UnresolvedReference`]: a missing alias or `$extends` target.
    UnresolvedReference,
    Projection,
    Reference,
    Selection,
    /// [`DtcgError::InvalidInput`]: malformed resolver inputs.
    InvalidInput,
    /// [`DtcgError::Expression`]: an invalid or failing token expression.
    Expression,
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
            DtcgError::UnresolvedReference {
                resource,
                path,
                target,
                message,
            } => (
                DtcgDiagnosticCode::UnresolvedReference,
                Some(resource.clone()),
                Some(path.clone()),
                format!("unresolved reference `{target}`: {message}"),
            ),
            DtcgError::Projection { path, message } => (
                DtcgDiagnosticCode::Projection,
                None,
                Some(path.clone()),
                message.clone(),
            ),
            DtcgError::InvalidReference {
                resource,
                reference,
                message,
            } => (
                DtcgDiagnosticCode::Reference,
                Some(resource.clone()),
                Some(reference.clone()),
                message.clone(),
            ),
            DtcgError::Expression {
                resource,
                path,
                message,
            } => (
                DtcgDiagnosticCode::Expression,
                Some(resource.clone()),
                Some(path.clone()),
                redacted_expression_message(message),
            ),
            DtcgError::InvalidInput { message } => (
                DtcgDiagnosticCode::InvalidInput,
                None,
                None,
                message.clone(),
            ),
            DtcgError::UnknownModifier { modifier, .. }
            | DtcgError::UnknownContext { modifier, .. } => (
                DtcgDiagnosticCode::Selection,
                None,
                Some(modifier.clone()),
                error.to_string(),
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

/// The kind of an expression failure and its authored position, without
/// the JavaScript message, which may quote document text or thrown values:
/// `TypeError at line 2, column 5`, `invalid expression object`.
fn redacted_expression_message(message: &str) -> String {
    let position = message
        .rfind(" at line ")
        .map(|index| &message[index..])
        .filter(|tail| {
            tail.strip_prefix(" at line ")
                .and_then(|rest| rest.split_once(", column "))
                .is_some_and(|(line, column)| {
                    !line.is_empty()
                        && !column.is_empty()
                        && line
                            .bytes()
                            .chain(column.bytes())
                            .all(|byte| byte.is_ascii_digit())
                })
        })
        .unwrap_or_default();
    let class = message
        .split_once(':')
        .map(|(head, _)| head)
        .filter(|head| {
            head.ends_with("Error") && head.bytes().all(|byte| byte.is_ascii_alphanumeric())
        });
    let kind = match class {
        Some(class) => class,
        None if message.starts_with("uncaught exception") => "uncaught exception",
        None if message.starts_with("stack overflow") => "stack overflow",
        None if message.contains("not a JSON value") => "result is not a JSON value",
        None if message.starts_with("the source nests too deeply") => "source nests too deeply",
        None if message.starts_with("dynamic `import()`") => "dynamic import",
        None => "invalid expression",
    };
    format!("{kind}{position}")
}
