//! String-backed DTCG document resolution and narrow typed projection.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
};

pub use dtcg_runtime_derive::{DtcgComponentTokens, DtcgTokens};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct ResourceId(Arc<str>);

impl ResourceId {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, DtcgError> {
        let value = value.as_ref();
        let path = value.split('#').next().unwrap_or_default();
        if path.is_empty()
            || path.starts_with('/')
            || path
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err(DtcgError::InvalidResourceId(value.to_owned()));
        }
        Ok(Self(path.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ResourceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentRequest {
    pub resource_id: ResourceId,
    pub referenced_from: ResourceId,
}

/// Owns already-loaded DTCG document text; callers retain all loading authority.
#[derive(Clone, Debug, Default)]
pub struct DocumentCatalog {
    documents: BTreeMap<ResourceId, Arc<str>>,
}

impl DocumentCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    pub fn len(&self) -> usize {
        self.documents.len()
    }

    fn provide(&mut self, id: ResourceId, json: Arc<str>) {
        self.documents.insert(id, json);
    }

    fn get(&self, id: &ResourceId) -> Option<&Arc<str>> {
        self.documents.get(id)
    }

    fn first_id(&self) -> Option<ResourceId> {
        self.documents.keys().next().cloned()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Resolution<T> {
    Ready(T),
    Missing(Vec<DocumentRequest>),
}

impl<T> Resolution<T> {
    pub fn ready(self) -> Option<T> {
        match self {
            Self::Ready(value) => Some(value),
            Self::Missing(_) => None,
        }
    }
}

#[derive(Debug, Error)]
pub enum DtcgError {
    #[error("invalid logical resource id `{0}`")]
    InvalidResourceId(String),
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
    #[error("token `{path}` in `{resource}`: {message}")]
    Token {
        resource: ResourceId,
        path: String,
        message: String,
    },
    #[error("projection `{path}`: {message}")]
    Projection { path: String, message: String },
}

#[derive(Clone, Debug)]
pub struct ResolvedToken {
    pub path: Arc<str>,
    pub token_type: Option<Arc<str>>,
    pub value: Value,
    pub source: ResourceId,
}

#[derive(Clone, Debug, Default)]
pub struct ResolvedTokens {
    tokens: BTreeMap<String, ResolvedToken>,
    fingerprint: Arc<str>,
}

impl ResolvedTokens {
    pub fn get(&self, path: &str) -> Option<&ResolvedToken> {
        self.tokens.get(path)
    }
    pub fn iter(&self) -> impl Iterator<Item = (&str, &ResolvedToken)> {
        self.tokens
            .iter()
            .map(|(path, token)| (path.as_str(), token))
    }
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
}

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

#[derive(Default)]
pub struct TypeRegistry;

impl TypeRegistry {
    pub fn convert<T: FromDtcgValue>(&self, token: &ResolvedToken) -> Result<T, DtcgError> {
        T::from_dtcg(token)
    }
}

pub trait DtcgTokens: Sized {
    fn project(tokens: &ResolvedTokens, registry: &TypeRegistry) -> Result<Self, DtcgError>;
}
pub trait DtcgComponentTokens: DtcgTokens {}

#[derive(Clone, Debug)]
struct RawToken {
    token_type: Option<Arc<str>>,
    value: Value,
    source: ResourceId,
}

#[derive(Default)]
pub struct DtcgRuntime {
    catalog: DocumentCatalog,
    root: Option<ResourceId>,
    registry: TypeRegistry,
}

impl DtcgRuntime {
    pub fn from_json(
        resource_id: impl AsRef<str>,
        json: impl Into<Arc<str>>,
    ) -> Result<Self, DtcgError> {
        let mut runtime = Self::default();
        runtime.provide_json(resource_id, json)?;
        runtime.root = runtime.catalog.first_id();
        Ok(runtime)
    }

    pub fn provide_json(
        &mut self,
        resource_id: impl AsRef<str>,
        json: impl Into<Arc<str>>,
    ) -> Result<(), DtcgError> {
        let id = ResourceId::parse(resource_id)?;
        let json = json.into();
        serde_json::from_str::<Value>(&json).map_err(|error| DtcgError::InvalidJson {
            resource: id.clone(),
            message: error.to_string(),
        })?;
        self.catalog.provide(id, json);
        Ok(())
    }

    pub fn set_root(&mut self, resource_id: impl AsRef<str>) -> Result<(), DtcgError> {
        self.root = Some(ResourceId::parse(resource_id)?);
        Ok(())
    }
    pub fn registry(&self) -> &TypeRegistry {
        &self.registry
    }

    pub fn resolve(
        &self,
        selection: &[(&str, &str)],
    ) -> Result<Resolution<ResolvedTokens>, DtcgError> {
        let root = self
            .root
            .as_ref()
            .ok_or_else(|| DtcgError::InvalidDocument {
                resource: ResourceId(Arc::from("catalog")),
                message: "root document is not set".into(),
            })?;
        let Some(root_json) = self.catalog.get(root) else {
            return Ok(Resolution::Missing(vec![DocumentRequest {
                resource_id: root.clone(),
                referenced_from: root.clone(),
            }]));
        };
        let root_value: Value =
            serde_json::from_str(root_json).map_err(|error| DtcgError::InvalidJson {
                resource: root.clone(),
                message: error.to_string(),
            })?;
        let sources = source_ids(root, &root_value, selection)?;
        let mut missing = BTreeSet::new();
        let mut raw = BTreeMap::new();
        for source in sources {
            match self.catalog.get(&source) {
                Some(json) => {
                    let value = parse_document(&source, json)?;
                    flatten_tokens(&source, "", &value, None, &mut raw)?;
                }
                None => {
                    missing.insert((source, root.clone()));
                }
            }
        }
        if !missing.is_empty() {
            return Ok(Resolution::Missing(
                missing
                    .into_iter()
                    .map(|(resource_id, referenced_from)| DocumentRequest {
                        resource_id,
                        referenced_from,
                    })
                    .collect(),
            ));
        }
        let mut resolved = BTreeMap::new();
        for path in raw.keys() {
            resolve_token(path, &raw, &mut resolved, &mut Vec::new())?;
        }
        let mut hasher = Sha256::new();
        for token in resolved.values() {
            hasher.update(token.path.as_bytes());
            hasher.update(token.value.to_string().as_bytes());
            hasher.update(token.source.as_str().as_bytes());
        }
        Ok(Resolution::Ready(ResolvedTokens {
            tokens: resolved,
            fingerprint: format!("{:x}", hasher.finalize()).into(),
        }))
    }

    pub fn project<T: DtcgTokens>(
        &self,
        selection: &[(&str, &str)],
    ) -> Result<Resolution<T>, DtcgError> {
        match self.resolve(selection)? {
            Resolution::Ready(tokens) => T::project(&tokens, &self.registry).map(Resolution::Ready),
            Resolution::Missing(requests) => Ok(Resolution::Missing(requests)),
        }
    }
}

fn parse_document(resource: &ResourceId, json: &str) -> Result<Value, DtcgError> {
    serde_json::from_str(json).map_err(|error| DtcgError::InvalidJson {
        resource: resource.clone(),
        message: error.to_string(),
    })
}

fn source_ids(
    root: &ResourceId,
    value: &Value,
    selection: &[(&str, &str)],
) -> Result<Vec<ResourceId>, DtcgError> {
    let object = value
        .as_object()
        .ok_or_else(|| DtcgError::InvalidDocument {
            resource: root.clone(),
            message: "expected object".into(),
        })?;
    if !object.contains_key("resolutionOrder") {
        return Ok(vec![root.clone()]);
    }
    let sets = object
        .get("sets")
        .and_then(Value::as_object)
        .ok_or_else(|| DtcgError::InvalidDocument {
            resource: root.clone(),
            message: "resolver sets are required".into(),
        })?;
    let empty_modifiers = serde_json::Map::new();
    let modifiers = object
        .get("modifiers")
        .and_then(Value::as_object)
        .unwrap_or(&empty_modifiers);
    let mut sources = Vec::new();
    let selected: BTreeMap<_, _> = selection.iter().copied().collect();
    for step in object
        .get("resolutionOrder")
        .and_then(Value::as_array)
        .ok_or_else(|| DtcgError::InvalidDocument {
            resource: root.clone(),
            message: "resolutionOrder is required".into(),
        })?
    {
        let reference =
            step.get("$ref")
                .and_then(Value::as_str)
                .ok_or_else(|| DtcgError::InvalidDocument {
                    resource: root.clone(),
                    message: "resolution step must have $ref".into(),
                })?;
        let parts: Vec<_> = reference.trim_start_matches("#/").split('/').collect();
        let documents = match parts.as_slice() {
            ["sets", name] => sets
                .get(*name)
                .and_then(|set| set.get("sources"))
                .and_then(Value::as_array),
            ["modifiers", name] => {
                let modifier = modifiers.get(*name).and_then(Value::as_object);
                let chosen = selected.get(*name).copied().or_else(|| {
                    modifier
                        .and_then(|m| m.get("default"))
                        .and_then(Value::as_str)
                });
                modifier.and_then(|m| {
                    chosen
                        .and_then(|choice| m.get("contexts").and_then(|c| c.get(choice)))
                        .and_then(Value::as_array)
                })
            }
            _ => None,
        }
        .ok_or_else(|| DtcgError::InvalidDocument {
            resource: root.clone(),
            message: format!("unknown resolver source `{reference}`"),
        })?;
        for document in documents {
            let source = document
                .get("$ref")
                .and_then(Value::as_str)
                .unwrap_or_else(|| document.as_str().unwrap_or_default());
            sources.push(ResourceId::parse(source)?);
        }
    }
    Ok(sources)
}

fn flatten_tokens(
    resource: &ResourceId,
    prefix: &str,
    value: &Value,
    inherited_type: Option<&str>,
    target: &mut BTreeMap<String, RawToken>,
) -> Result<(), DtcgError> {
    let object = value
        .as_object()
        .ok_or_else(|| DtcgError::InvalidDocument {
            resource: resource.clone(),
            message: "token document groups must be objects".into(),
        })?;
    let token_type = object
        .get("$type")
        .and_then(Value::as_str)
        .or(inherited_type);
    if let Some(token_value) = object.get("$value") {
        if prefix.is_empty() {
            return Err(DtcgError::InvalidDocument {
                resource: resource.clone(),
                message: "root token is invalid".into(),
            });
        }
        target.insert(
            prefix.into(),
            RawToken {
                token_type: token_type.map(Arc::from),
                value: token_value.clone(),
                source: resource.clone(),
            },
        );
        return Ok(());
    }
    for (name, child) in object {
        if name.starts_with('$') {
            continue;
        }
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}.{name}")
        };
        flatten_tokens(resource, &path, child, token_type, target)?;
    }
    Ok(())
}

fn resolve_token(
    path: &str,
    raw: &BTreeMap<String, RawToken>,
    resolved: &mut BTreeMap<String, ResolvedToken>,
    visiting: &mut Vec<String>,
) -> Result<(), DtcgError> {
    if resolved.contains_key(path) {
        return Ok(());
    }
    if visiting.iter().any(|item| item == path) {
        let resource = raw
            .get(path)
            .map(|token| token.source.clone())
            .unwrap_or_else(|| ResourceId(Arc::from("tokens")));
        return Err(DtcgError::Token {
            resource,
            path: path.into(),
            message: format!("alias cycle: {}", visiting.join(" -> ")),
        });
    }
    let token = raw.get(path).ok_or_else(|| DtcgError::Projection {
        path: path.into(),
        message: "missing alias target".into(),
    })?;
    visiting.push(path.into());
    let value = resolve_value(&token.value, raw, resolved, visiting)?;
    visiting.pop();
    resolved.insert(
        path.into(),
        ResolvedToken {
            path: path.into(),
            token_type: token.token_type.clone(),
            value,
            source: token.source.clone(),
        },
    );
    Ok(())
}

fn resolve_value(
    value: &Value,
    raw: &BTreeMap<String, RawToken>,
    resolved: &mut BTreeMap<String, ResolvedToken>,
    visiting: &mut Vec<String>,
) -> Result<Value, DtcgError> {
    if let Some(reference) = value.as_str().and_then(|text| {
        text.strip_prefix('{')
            .and_then(|text| text.strip_suffix('}'))
    }) {
        resolve_token(reference, raw, resolved, visiting)?;
        return resolved
            .get(reference)
            .map(|token| token.value.clone())
            .ok_or_else(|| DtcgError::Projection {
                path: reference.into(),
                message: "alias target was not resolved".into(),
            });
    }
    match value {
        Value::Array(values) => values
            .iter()
            .map(|item| resolve_value(item, raw, resolved, visiting))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(values) => values
            .iter()
            .map(|(key, item)| {
                resolve_value(item, raw, resolved, visiting).map(|value| (key.clone(), value))
            })
            .collect::<Result<serde_json::Map<_, _>, _>>()
            .map(Value::Object),
        _ => Ok(value.clone()),
    }
}
