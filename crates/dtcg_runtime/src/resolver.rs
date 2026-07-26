use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    DocumentReference, DtcgError, ResourceId,
    catalog::{DocumentCatalog, parse_document},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentRequest {
    pub resource_id: ResourceId,
    pub referenced_from: ResourceId,
    pub original_reference: Arc<str>,
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

#[derive(Clone, Debug)]
pub struct RuntimeLimits {
    pub max_documents: usize,
    pub max_reference_depth: usize,
    pub max_alias_depth: usize,
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            max_documents: 256,
            max_reference_depth: 64,
            max_alias_depth: 64,
        }
    }
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

#[derive(Clone)]
struct Source {
    id: ResourceId,
    fragment: Option<Arc<str>>,
    referenced_from: ResourceId,
    original: Arc<str>,
}

pub(crate) fn resolve(
    catalog: &DocumentCatalog,
    root: Option<&ResourceId>,
    limits: &RuntimeLimits,
    selection: &[(&str, &str)],
) -> Result<Resolution<ResolvedTokens>, DtcgError> {
    let sources = discover(catalog, required_root(root)?, limits, selection, false)?;
    let Resolution::Ready(sources) = sources else {
        return Ok(sources.map(|_| unreachable!()));
    };
    let mut raw = BTreeMap::new();
    for source in sources {
        flatten_source(catalog, &source, &mut raw)?;
    }
    let mut resolved = BTreeMap::new();
    for path in raw.keys() {
        resolve_token(path, &raw, &mut resolved, &mut Vec::new(), limits)?;
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

pub(crate) fn validate_complete(
    catalog: &DocumentCatalog,
    root: Option<&ResourceId>,
    limits: &RuntimeLimits,
) -> Result<Resolution<()>, DtcgError> {
    match discover(catalog, required_root(root)?, limits, &[], true)? {
        Resolution::Ready(_) => Ok(Resolution::Ready(())),
        Resolution::Missing(requests) => Ok(Resolution::Missing(requests)),
    }
}

trait ResolutionMap<T> {
    fn map<U>(self, function: impl FnOnce(T) -> U) -> Resolution<U>;
}
impl<T> ResolutionMap<T> for Resolution<T> {
    fn map<U>(self, function: impl FnOnce(T) -> U) -> Resolution<U> {
        match self {
            Resolution::Ready(value) => Resolution::Ready(function(value)),
            Resolution::Missing(requests) => Resolution::Missing(requests),
        }
    }
}

fn required_root(root: Option<&ResourceId>) -> Result<&ResourceId, DtcgError> {
    root.ok_or_else(|| DtcgError::InvalidDocument {
        resource: ResourceId::parse("catalog").expect("static id"),
        message: "root document is not set".into(),
    })
}

fn discover(
    catalog: &DocumentCatalog,
    root: &ResourceId,
    limits: &RuntimeLimits,
    selection: &[(&str, &str)],
    include_inactive: bool,
) -> Result<Resolution<Vec<Source>>, DtcgError> {
    let selections: BTreeMap<_, _> = selection.iter().copied().collect();
    let mut queue = vec![(
        Source {
            id: root.clone(),
            fragment: None,
            referenced_from: root.clone(),
            original: root.as_str().into(),
        },
        0usize,
        Vec::new(),
    )];
    let mut terminal = Vec::new();
    let mut missing = BTreeMap::<ResourceId, DocumentRequest>::new();
    let mut expanded = BTreeSet::new();
    let mut terminal_ids = BTreeSet::new();
    while let Some((source, depth, ancestors)) = queue.pop() {
        if depth > limits.max_reference_depth {
            return Err(DtcgError::LimitExceeded {
                kind: "reference depth",
                limit: limits.max_reference_depth,
            });
        }
        if queue.len() + terminal.len() > limits.max_documents {
            return Err(DtcgError::LimitExceeded {
                kind: "documents",
                limit: limits.max_documents,
            });
        }
        let Some(text) = catalog.get(&source.id) else {
            missing.entry(source.id.clone()).or_insert(DocumentRequest {
                resource_id: source.id,
                referenced_from: source.referenced_from,
                original_reference: source.original,
            });
            continue;
        };
        let value = parse_document(&source.id, text)?;
        if !is_resolver(&value) {
            if terminal_ids.insert((source.id.clone(), source.fragment.clone())) {
                terminal.push(source);
            }
            continue;
        }
        if ancestors.contains(&source.id) {
            return Err(DtcgError::DocumentCycle(source.id.to_string()));
        }
        if !expanded.insert(source.id.clone()) {
            continue;
        }
        let children = resolver_sources(&source.id, &value, &selections, include_inactive)?;
        let mut child_ancestors = ancestors;
        child_ancestors.push(source.id.clone());
        for child in children.into_iter().rev() {
            queue.push((child, depth + 1, child_ancestors.clone()));
        }
    }
    if missing.is_empty() {
        Ok(Resolution::Ready(terminal))
    } else {
        Ok(Resolution::Missing(missing.into_values().collect()))
    }
}

fn is_resolver(value: &Value) -> bool {
    value
        .as_object()
        .is_some_and(|object| object.contains_key("resolutionOrder"))
}

fn resolver_sources(
    parent: &ResourceId,
    value: &Value,
    selections: &BTreeMap<&str, &str>,
    include_inactive: bool,
) -> Result<Vec<Source>, DtcgError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid(parent, "expected resolver object"))?;
    let sets = object
        .get("sets")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid(parent, "resolver sets are required"))?;
    let modifiers = object.get("modifiers").and_then(Value::as_object);
    let order = object
        .get("resolutionOrder")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid(parent, "resolutionOrder is required"))?;
    let mut result = Vec::new();
    for step in order {
        let pointer = step
            .get("$ref")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid(parent, "resolution step must have $ref"))?;
        let parts = pointer_parts(pointer)?;
        let documents = match parts.as_slice() {
            [kind, name] if kind == "sets" => sets
                .get(name)
                .and_then(|set| set.get("sources"))
                .and_then(Value::as_array)
                .cloned()
                .ok_or_else(|| invalid(parent, "set sources are required"))?,
            [kind, name] if kind == "modifiers" => {
                modifier_documents(parent, modifiers, name, selections, include_inactive)?
            }
            _ => {
                return Err(invalid(
                    parent,
                    &format!("unknown resolver source `{pointer}`"),
                ));
            }
        };
        for document in documents {
            result.push(source_from_value(parent, &document)?);
        }
    }
    Ok(result)
}

fn modifier_documents(
    parent: &ResourceId,
    modifiers: Option<&serde_json::Map<String, Value>>,
    name: &str,
    selections: &BTreeMap<&str, &str>,
    include_inactive: bool,
) -> Result<Vec<Value>, DtcgError> {
    let modifier = modifiers
        .and_then(|modifiers| modifiers.get(name))
        .and_then(Value::as_object)
        .ok_or_else(|| invalid(parent, "unknown modifier"))?;
    let contexts = modifier
        .get("contexts")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid(parent, "modifier contexts are required"))?;
    if include_inactive {
        return contexts
            .iter()
            .flat_map(|(_, value)| value.as_array().into_iter().flatten().cloned())
            .collect::<Vec<_>>()
            .pipe(Ok);
    }
    let choice = selections
        .get(name)
        .copied()
        .or_else(|| modifier.get("default").and_then(Value::as_str))
        .ok_or_else(|| invalid(parent, "modifier has no selected or default context"))?;
    contexts
        .get(choice)
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| invalid(parent, "selected modifier context is unknown"))
}

trait Pipe: Sized {
    fn pipe<T>(self, function: impl FnOnce(Self) -> T) -> T {
        function(self)
    }
}
impl<T> Pipe for T {}

fn source_from_value(parent: &ResourceId, value: &Value) -> Result<Source, DtcgError> {
    let reference = value
        .get("$ref")
        .and_then(Value::as_str)
        .or_else(|| value.as_str())
        .ok_or_else(|| invalid(parent, "document source must be a string or $ref object"))?;
    let reference: DocumentReference = parent.resolve(reference)?;
    Ok(Source {
        id: reference.resource_id().clone(),
        fragment: reference.fragment().map(Arc::from),
        referenced_from: parent.clone(),
        original: reference.original().into(),
    })
}

fn pointer_parts(pointer: &str) -> Result<Vec<String>, DtcgError> {
    if !pointer.starts_with("#/") {
        return Err(DtcgError::InvalidPointer(pointer.into()));
    }
    pointer[2..].split('/').map(unescape).collect()
}

fn unescape(value: &str) -> Result<String, DtcgError> {
    let mut output = String::new();
    let mut chars = value.chars();
    while let Some(character) = chars.next() {
        if character == '~' {
            match chars.next() {
                Some('0') => output.push('~'),
                Some('1') => output.push('/'),
                _ => return Err(DtcgError::InvalidPointer(value.into())),
            }
        } else {
            output.push(character);
        }
    }
    Ok(output)
}

fn flatten_source(
    catalog: &DocumentCatalog,
    source: &Source,
    target: &mut BTreeMap<String, RawToken>,
) -> Result<(), DtcgError> {
    let json = catalog
        .get(&source.id)
        .ok_or_else(|| DtcgError::UnknownDocument(source.id.clone()))?;
    let value = parse_document(&source.id, json)?;
    let value = source.fragment.as_deref().map_or(Ok(&value), |fragment| {
        value
            .pointer(fragment)
            .ok_or_else(|| DtcgError::InvalidPointer(fragment.into()))
    })?;
    flatten_group(&source.id, "", value, None, value, target, &mut Vec::new())
}

#[derive(Clone)]
struct RawToken {
    token_type: Option<Arc<str>>,
    value: Value,
    source: ResourceId,
}

fn flatten_group(
    resource: &ResourceId,
    prefix: &str,
    value: &Value,
    inherited_type: Option<&str>,
    document: &Value,
    target: &mut BTreeMap<String, RawToken>,
    extensions: &mut Vec<String>,
) -> Result<(), DtcgError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid(resource, "token document groups must be objects"))?;
    let token_type = effective_group_type(value, document, inherited_type, &mut Vec::new())?;
    if let Some(token_value) = object.get("$value") {
        if prefix.is_empty() {
            return Err(invalid(resource, "root token is invalid"));
        }
        target.insert(
            prefix.into(),
            RawToken {
                token_type: token_type.as_deref().map(Arc::from),
                value: token_value.clone(),
                source: resource.clone(),
            },
        );
        return Ok(());
    }
    if let Some(reference) = object.get("$extends").and_then(Value::as_str) {
        let group_path = extension_path(reference)?;
        if extensions.iter().any(|path| path == &group_path) {
            return Err(DtcgError::Token {
                resource: resource.clone(),
                path: prefix.into(),
                message: format!(
                    "group extension cycle: {} -> {group_path}",
                    extensions.join(" -> ")
                ),
            });
        }
        let extended = group_at_path(document, &group_path)
            .ok_or_else(|| invalid(resource, "extended group was not found"))?;
        extensions.push(group_path);
        flatten_group(
            resource,
            prefix,
            extended,
            inherited_type,
            document,
            target,
            extensions,
        )?;
        extensions.pop();
    }
    if let Some(root) = object.get("$root") {
        if prefix.is_empty() {
            return Err(invalid(resource, "root token is invalid"));
        }
        flatten_group(
            resource,
            prefix,
            root,
            token_type.as_deref(),
            document,
            target,
            extensions,
        )?;
    }
    for (name, child) in object {
        if !name.starts_with('$') {
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}.{name}")
            };
            flatten_group(
                resource,
                &path,
                child,
                token_type.as_deref(),
                document,
                target,
                extensions,
            )?;
        }
    }
    Ok(())
}

fn effective_group_type(
    group: &Value,
    document: &Value,
    inherited: Option<&str>,
    visiting: &mut Vec<String>,
) -> Result<Option<String>, DtcgError> {
    if let Some(token_type) = group.get("$type").and_then(Value::as_str) {
        return Ok(Some(token_type.into()));
    }
    if let Some(reference) = group.get("$extends").and_then(Value::as_str) {
        let path = extension_path(reference)?;
        if visiting.iter().any(|entry| entry == &path) {
            return Err(DtcgError::Token {
                resource: ResourceId::parse("tokens").expect("static id"),
                path,
                message: "group extension cycle".into(),
            });
        }
        let extended = group_at_path(document, &path).ok_or_else(|| DtcgError::Projection {
            path,
            message: "extended group was not found".into(),
        })?;
        visiting.push(reference.into());
        let result = effective_group_type(extended, document, inherited, visiting);
        visiting.pop();
        return result;
    }
    Ok(inherited.map(str::to_owned))
}

fn extension_path(reference: &str) -> Result<String, DtcgError> {
    reference
        .strip_prefix('{')
        .and_then(|path| path.strip_suffix('}'))
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| DtcgError::Projection {
            path: reference.into(),
            message: "$extends must be a group alias".into(),
        })
}

fn group_at_path<'a>(document: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .try_fold(document, |group, segment| group.get(segment))
}

fn resolve_token(
    path: &str,
    raw: &BTreeMap<String, RawToken>,
    resolved: &mut BTreeMap<String, ResolvedToken>,
    visiting: &mut Vec<String>,
    limits: &RuntimeLimits,
) -> Result<(), DtcgError> {
    if resolved.contains_key(path) {
        return Ok(());
    }
    if visiting.len() >= limits.max_alias_depth {
        return Err(DtcgError::LimitExceeded {
            kind: "alias depth",
            limit: limits.max_alias_depth,
        });
    }
    if visiting.iter().any(|item| item == path) {
        let resource = raw
            .get(path)
            .map(|token| token.source.clone())
            .unwrap_or_else(|| ResourceId::parse("tokens").expect("static id"));
        return Err(DtcgError::Token {
            resource,
            path: path.into(),
            message: format!("alias cycle: {} -> {path}", visiting.join(" -> ")),
        });
    }
    let token = raw.get(path).ok_or_else(|| DtcgError::Projection {
        path: path.into(),
        message: "missing alias target".into(),
    })?;
    visiting.push(path.into());
    let value = resolve_value(&token.value, raw, resolved, visiting, limits)?;
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
    limits: &RuntimeLimits,
) -> Result<Value, DtcgError> {
    if let Some(reference) = value.as_str().and_then(|text| {
        text.strip_prefix('{')
            .and_then(|text| text.strip_suffix('}'))
    }) {
        resolve_token(reference, raw, resolved, visiting, limits)?;
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
            .map(|item| resolve_value(item, raw, resolved, visiting, limits))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(values) => values
            .iter()
            .map(|(key, item)| {
                resolve_value(item, raw, resolved, visiting, limits)
                    .map(|value| (key.clone(), value))
            })
            .collect::<Result<serde_json::Map<_, _>, _>>()
            .map(Value::Object),
        _ => Ok(value.clone()),
    }
}

fn invalid(resource: &ResourceId, message: &str) -> DtcgError {
    DtcgError::InvalidDocument {
        resource: resource.clone(),
        message: message.into(),
    }
}
