use std::sync::Arc;

use serde_json::Value;

use crate::{DtcgError, LocalReferencePolicy, ResourceId, catalog::parse_document};

/// The Token Editor project envelope: `{ "data": <resolver> }` whose resolver
/// declares `version: "2025.10"` and references only relative local documents,
/// resolved against the envelope's own location.
#[derive(Clone, Debug)]
pub struct ResolverEnvelope {
    name: Option<String>,
    resolver: Arc<str>,
}

impl ResolverEnvelope {
    pub fn parse(resource: &ResourceId, json: &str) -> Result<Self, DtcgError> {
        let envelope = parse_document(resource, json)?;
        let invalid = |message: String| DtcgError::InvalidEnvelope {
            resource: resource.clone(),
            message,
        };
        let data = envelope
            .get("data")
            .filter(|data| data.is_object())
            .ok_or_else(|| invalid("the envelope requires an object `data` member".into()))?;
        let version = data
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if version != "2025.10" {
            return Err(invalid(format!(
                "expected resolver version `2025.10`, found `{version}`"
            )));
        }
        LocalReferencePolicy::check_resolver(resource, data)?;
        Ok(Self {
            name: data.get("name").and_then(Value::as_str).map(str::to_owned),
            resolver: data.to_string().into(),
        })
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// The resolver document to provide as the runtime root.
    pub fn resolver_json(&self) -> &Arc<str> {
        &self.resolver
    }
}
