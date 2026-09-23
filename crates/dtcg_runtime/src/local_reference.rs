use serde_json::Value;

use crate::{DtcgError, ResourceId};

/// The document-reference policy shared by RayX themes and the Token Editor: a
/// resolver may only reference relative local documents. Absolute paths,
/// backslashes and URI schemes are rejected before any document is requested.
pub struct LocalReferencePolicy;

impl LocalReferencePolicy {
    /// Validates one authored reference (`path#/fragment`); a fragment-only
    /// reference (`#/sets/x`) is local by construction.
    pub fn check(resource: &ResourceId, reference: &str) -> Result<(), DtcgError> {
        let path = reference
            .split_once('#')
            .map_or(reference, |(path, _)| path);
        let rejected = if reference.is_empty() {
            Some("reference is empty")
        } else if path.starts_with('/') {
            Some("absolute paths are not supported")
        } else if path.contains('\\') {
            Some("backslashes are not supported")
        } else if has_uri_scheme(path) {
            Some("URI schemes are not supported")
        } else {
            None
        };
        match rejected {
            Some(message) => Err(DtcgError::InvalidReference {
                resource: resource.clone(),
                reference: reference.into(),
                message: message.into(),
            }),
            None => Ok(()),
        }
    }

    /// Validates every `$ref` of a resolver's sets and modifier contexts, the
    /// places where a resolver names other documents.
    pub fn check_resolver(resource: &ResourceId, resolver: &Value) -> Result<(), DtcgError> {
        let Some(resolver) = resolver.as_object() else {
            return Ok(());
        };
        let sets = resolver
            .get("sets")
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|sets| sets.values())
            .filter_map(|set| set.get("sources").and_then(Value::as_array));
        let contexts = resolver
            .get("modifiers")
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|modifiers| modifiers.values())
            .filter_map(|modifier| modifier.get("contexts").and_then(Value::as_object))
            .flat_map(|contexts| contexts.values().filter_map(Value::as_array));
        for sources in sets.chain(contexts) {
            for source in sources {
                if let Some(reference) = source.get("$ref").and_then(Value::as_str) {
                    Self::check(resource, reference)?;
                }
            }
        }
        Ok(())
    }
}

fn has_uri_scheme(value: &str) -> bool {
    let Some((scheme, _)) = value.split_once(':') else {
        return false;
    };
    let mut characters = scheme.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && characters.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
        })
}
