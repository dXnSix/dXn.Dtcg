use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    sync::Arc,
};

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::{
    AffectedResolution, DtcgError, ResourceId, SourceOccurrence,
    catalog::DocumentCatalog,
    error::invalid,
    json_pointer::pointer_parts,
    source_discovery::{discover, required_root},
    validate_value,
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

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedToken {
    pub path: Arc<str>,
    pub token_type: Option<Arc<str>>,
    pub authored_value: Value,
    pub value: Value,
    /// The token's authored `$extensions` object, or `Null` when it has none.
    pub extensions: Value,
    pub source: ResourceId,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResolvedTokens {
    tokens: BTreeMap<String, ResolvedToken>,
    order: Vec<String>,
    fingerprint: Arc<str>,
}

impl ResolvedTokens {
    pub fn get(&self, path: &str) -> Option<&ResolvedToken> {
        self.tokens.get(path)
    }
    /// Tokens in resolution order.
    pub fn iter(&self) -> impl Iterator<Item = &ResolvedToken> {
        self.order.iter().filter_map(|path| self.tokens.get(path))
    }
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
    pub fn len(&self) -> usize {
        self.order.len()
    }
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
    /// Merge an affected re-resolution into this projection and refresh the fingerprint.
    pub fn apply_affected(&mut self, affected: &AffectedResolution) {
        for token in affected.iter() {
            let path = token.path.to_string();
            if self.tokens.insert(path.clone(), token.clone()).is_none() {
                self.order.push(path);
            }
        }
        self.fingerprint = fingerprint(&self.order, &self.tokens);
    }
}

fn fingerprint(order: &[String], tokens: &BTreeMap<String, ResolvedToken>) -> Arc<str> {
    let mut hasher = Sha256::new();
    for path in order {
        let token = tokens
            .get(path)
            .expect("every ordered raw token was resolved");
        hasher.update(token.path.as_bytes());
        hasher.update([0]);
        if let Some(token_type) = &token.token_type {
            hasher.update(token_type.as_bytes());
        }
        hasher.update([0]);
        hasher.update(token.value.to_string().as_bytes());
        hasher.update([0]);
        hasher.update(token.source.as_str().as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize()).into()
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
    resolve_sources(&sources, limits).map(Resolution::Ready)
}

pub(crate) fn resolve_sources(
    sources: &[SourceOccurrence],
    limits: &RuntimeLimits,
) -> Result<ResolvedTokens, DtcgError> {
    let mut raw = BTreeMap::new();
    let mut order = Vec::new();
    for source in sources {
        flatten_source(source, &mut raw, &mut order)?;
    }
    keep_last_definitions(&mut order);
    let mut resolved = BTreeMap::new();
    for path in &order {
        resolve_token(
            path,
            &raw,
            &mut resolved,
            &mut Vec::new(),
            &mut Vec::new(),
            limits,
        )?;
    }
    let fingerprint = fingerprint(&order, &resolved);
    Ok(ResolvedTokens {
        tokens: resolved,
        order,
        fingerprint,
    })
}

pub(crate) fn resolve_affected(
    catalog: &DocumentCatalog,
    root: Option<&ResourceId>,
    limits: &RuntimeLimits,
    selection: &[(&str, &str)],
    changed_paths: &[&str],
) -> Result<Resolution<AffectedResolution>, DtcgError> {
    let sources = discover(catalog, required_root(root)?, limits, selection, false)?;
    let Resolution::Ready(sources) = sources else {
        return Ok(sources.map(|_| unreachable!()));
    };
    let mut raw = BTreeMap::new();
    let mut order = Vec::new();
    for source in sources {
        flatten_source(&source, &mut raw, &mut order)?;
    }
    keep_last_definitions(&mut order);

    let mut affected = changed_paths
        .iter()
        .filter(|path| raw.contains_key(**path))
        .map(|path| (*path).to_owned())
        .collect::<BTreeSet<_>>();
    let mut inspected_documents = BTreeSet::new();
    if raw.values().any(|token| {
        inspected_documents.insert(Arc::as_ptr(&token.document) as usize)
            && contains_group_extension(token.document.as_ref())
    }) {
        affected.extend(order.iter().cloned());
    }
    let mut reverse_dependencies = BTreeMap::<String, BTreeSet<String>>::new();
    for (path, token) in &raw {
        let mut dependencies = BTreeSet::new();
        collect_token_dependencies(
            &token.value,
            token.document.as_ref(),
            &raw,
            &mut dependencies,
            &mut Vec::new(),
        )?;
        for dependency in dependencies {
            reverse_dependencies
                .entry(dependency)
                .or_default()
                .insert(path.clone());
        }
    }
    let mut pending = affected.iter().cloned().collect::<Vec<_>>();
    while let Some(path) = pending.pop() {
        if let Some(dependents) = reverse_dependencies.get(&path) {
            for dependent in dependents {
                if affected.insert(dependent.clone()) {
                    pending.push(dependent.clone());
                }
            }
        }
    }

    let mut resolved = BTreeMap::new();
    for path in order.iter().filter(|path| affected.contains(*path)) {
        resolve_token(
            path,
            &raw,
            &mut resolved,
            &mut Vec::new(),
            &mut Vec::new(),
            limits,
        )?;
    }
    #[cfg(feature = "diagnostics")]
    let nodes_visited = resolved.len();
    let tokens = order
        .iter()
        .filter(|path| affected.contains(*path))
        .filter_map(|path| resolved.get(path).cloned())
        .collect();
    Ok(Resolution::Ready(AffectedResolution {
        tokens,
        #[cfg(feature = "diagnostics")]
        nodes_visited,
    }))
}

fn contains_group_extension(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            object.contains_key("$extends") || object.values().any(contains_group_extension)
        }
        Value::Array(array) => array.iter().any(contains_group_extension),
        _ => false,
    }
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

fn flatten_source(
    source: &SourceOccurrence,
    target: &mut BTreeMap<String, RawToken>,
    order: &mut Vec<String>,
) -> Result<(), DtcgError> {
    let value = Arc::new(source.value.clone());
    let mut output = FlattenedTokens { target, order };
    flatten_group(
        &source.resource,
        "",
        value.as_ref(),
        None,
        &value,
        &mut output,
        &mut Vec::new(),
    )
}

#[derive(Clone)]
struct RawToken {
    token_type: Option<Arc<str>>,
    authored_value: Value,
    value: Value,
    extensions: Value,
    source: ResourceId,
    document: Arc<Value>,
}

struct FlattenedTokens<'a> {
    target: &'a mut BTreeMap<String, RawToken>,
    order: &'a mut Vec<String>,
}

fn flatten_group(
    resource: &ResourceId,
    prefix: &str,
    value: &Value,
    inherited_type: Option<&str>,
    document: &Arc<Value>,
    output: &mut FlattenedTokens<'_>,
    extensions: &mut Vec<String>,
) -> Result<(), DtcgError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid(resource, "token document groups must be objects"))?;
    for name in object.keys().filter(|name| name.starts_with('$')) {
        if !matches!(
            name.as_str(),
            "$value"
                | "$type"
                | "$description"
                | "$extensions"
                | "$deprecated"
                | "$root"
                | "$extends"
                | "$ref"
        ) {
            return Err(DtcgError::Token {
                resource: resource.clone(),
                path: prefix.into(),
                message: format!("unknown reserved property `{name}`"),
            });
        }
    }
    if object.get("$type").is_some_and(|value| !value.is_string()) {
        return Err(DtcgError::Token {
            resource: resource.clone(),
            path: prefix.into(),
            message: "`$type` must be a string".into(),
        });
    }
    if object
        .get("$extends")
        .is_some_and(|value| !value.is_string())
    {
        return Err(DtcgError::Token {
            resource: resource.clone(),
            path: prefix.into(),
            message: "`$extends` must be a string".into(),
        });
    }
    let token_type = effective_group_type(
        resource,
        value,
        document.as_ref(),
        inherited_type,
        &mut Vec::new(),
    )?;
    if let Some(token_value) = object.get("$value") {
        if prefix.is_empty() {
            return Err(invalid(resource, "root token is invalid"));
        }
        insert_raw_token(
            prefix,
            RawToken {
                token_type: token_type.as_deref().map(Arc::from),
                authored_value: token_value.clone(),
                value: token_value.clone(),
                extensions: object.get("$extensions").cloned().unwrap_or(Value::Null),
                source: resource.clone(),
                document: Arc::clone(document),
            },
            output,
        );
        return Ok(());
    }
    if let Some(reference) = object.get("$ref") {
        let reference = reference.as_str().ok_or_else(|| DtcgError::Token {
            resource: resource.clone(),
            path: prefix.into(),
            message: "token-level `$ref` must be a string".into(),
        })?;
        if !reference.starts_with("#/") {
            return Err(DtcgError::Token {
                resource: resource.clone(),
                path: prefix.into(),
                message: "token-level `$ref` must be a local JSON Pointer".into(),
            });
        }
        pointer_parts(reference)?;
        insert_raw_token(
            prefix,
            RawToken {
                token_type: object.get("$type").and_then(Value::as_str).map(Arc::from),
                authored_value: Value::Object(object.clone()),
                value: Value::Object(Map::from_iter([(
                    "$ref".into(),
                    Value::String(reference.into()),
                )])),
                extensions: object.get("$extensions").cloned().unwrap_or(Value::Null),
                source: resource.clone(),
                document: Arc::clone(document),
            },
            output,
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
        if extended.get("$value").is_some() {
            return Err(DtcgError::Token {
                resource: resource.clone(),
                path: prefix.into(),
                message: format!("`$extends` target `{reference}` must be a group"),
            });
        }
        extensions.push(group_path);
        flatten_group(
            resource,
            prefix,
            extended,
            inherited_type,
            document,
            output,
            extensions,
        )?;
        extensions.pop();
    }
    if let Some(root) = object.get("$root") {
        if prefix.is_empty() {
            return Err(invalid(resource, "root token is invalid"));
        }
        let root_path = format!("{prefix}.$root");
        flatten_group(
            resource,
            &root_path,
            root,
            token_type.as_deref(),
            document,
            output,
            extensions,
        )?;
    }
    for (name, child) in object {
        if !name.starts_with('$') {
            validate_name(resource, prefix, name)?;
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
                output,
                extensions,
            )?;
        }
    }
    Ok(())
}

fn insert_raw_token(path: &str, token: RawToken, output: &mut FlattenedTokens<'_>) {
    output.target.insert(path.into(), token);
    output.order.push(path.into());
}

/// Keep each path once, at the position of its last definition: a later source
/// overrides an earlier one and takes its place in resolution order. One pass
/// over the flattened order replaces a scan per override, which dominated
/// resolution once most tokens were overridden by a later set.
fn keep_last_definitions(order: &mut Vec<String>) {
    let mut seen = HashSet::with_capacity(order.len());
    let mut keep = vec![false; order.len()];
    for (index, path) in order.iter().enumerate().rev() {
        keep[index] = seen.insert(path.as_str());
    }
    let mut index = 0;
    order.retain(|_| {
        index += 1;
        keep[index - 1]
    });
}

fn effective_group_type(
    resource: &ResourceId,
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
                resource: resource.clone(),
                path: path.clone(),
                message: format!("group extension cycle: {} -> {path}", visiting.join(" -> ")),
            });
        }
        let extended = group_at_path(document, &path).ok_or_else(|| DtcgError::Projection {
            path: path.clone(),
            message: "extended group was not found".into(),
        })?;
        visiting.push(path);
        let result = effective_group_type(resource, extended, document, inherited, visiting);
        visiting.pop();
        return result;
    }
    Ok(inherited.map(str::to_owned))
}

fn extension_path(reference: &str) -> Result<String, DtcgError> {
    if reference.starts_with("#/") {
        return pointer_parts(reference).map(|parts| parts.join("."));
    }
    reference
        .strip_prefix('{')
        .and_then(|path| path.strip_suffix('}'))
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| DtcgError::Projection {
            path: reference.into(),
            message: "$extends must be a group alias or local JSON Pointer".into(),
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
    pointer_visiting: &mut Vec<String>,
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
    let alias_type = if token.token_type.is_none() {
        whole_token_reference(&token.value)?
            .map(|reference| {
                resolve_token(
                    &reference,
                    raw,
                    resolved,
                    visiting,
                    pointer_visiting,
                    limits,
                )?;
                resolved
                    .get(&reference)
                    .and_then(|target| target.token_type.clone())
                    .ok_or_else(|| DtcgError::Projection {
                        path: reference,
                        message: "alias target has no DTCG type".into(),
                    })
            })
            .transpose()?
    } else {
        None
    };
    let value = resolve_value(
        &token.value,
        token.document.as_ref(),
        raw,
        resolved,
        visiting,
        pointer_visiting,
        limits,
    )?;
    visiting.pop();
    let token_type =
        token
            .token_type
            .clone()
            .or(alias_type)
            .ok_or_else(|| DtcgError::TokenType {
                resource: token.source.clone(),
                path: path.into(),
                message: "token requires a DTCG type".into(),
            })?;
    validate_value(&token_type, &value).map_err(|error| DtcgError::TokenType {
        resource: token.source.clone(),
        path: path.into(),
        message: error.to_string(),
    })?;
    resolved.insert(
        path.into(),
        ResolvedToken {
            path: path.into(),
            token_type: Some(token_type),
            authored_value: token.authored_value.clone(),
            value,
            extensions: token.extensions.clone(),
            source: token.source.clone(),
        },
    );
    Ok(())
}

fn whole_token_reference(value: &Value) -> Result<Option<String>, DtcgError> {
    if let Some(reference) = value
        .as_str()
        .and_then(|value| value.strip_prefix('{'))
        .and_then(|value| value.strip_suffix('}'))
        .filter(|path| !path.is_empty())
    {
        return Ok(Some(reference.into()));
    }
    let Some(object) = value.as_object() else {
        return Ok(None);
    };
    if object.len() != 1 {
        return Ok(None);
    }
    let Some(reference) = object.get("$ref").and_then(Value::as_str) else {
        return Ok(None);
    };
    let parts = pointer_parts(reference)?;
    let Some(value_index) = parts.iter().position(|part| part == "$value") else {
        return Ok(None);
    };
    if value_index + 1 != parts.len() || value_index == 0 {
        return Ok(None);
    }
    Ok(Some(parts[..value_index].join(".")))
}

fn collect_token_dependencies(
    value: &Value,
    document: &Value,
    raw: &BTreeMap<String, RawToken>,
    dependencies: &mut BTreeSet<String>,
    pointer_visiting: &mut Vec<String>,
) -> Result<(), DtcgError> {
    if let Some(reference) = value.as_str().and_then(|text| {
        text.strip_prefix('{')
            .and_then(|text| text.strip_suffix('}'))
    }) {
        if raw.contains_key(reference) {
            dependencies.insert(reference.to_owned());
        }
        return Ok(());
    }
    if let Some(object) = value.as_object()
        && let Some(reference) = object.get("$ref").and_then(Value::as_str)
    {
        let parts = pointer_parts(reference)?;
        if let Some((_, token_path)) = parts
            .iter()
            .position(|part| part == "$value" || part == "$type")
            .filter(|index| *index > 0)
            .map(|index| (index, parts[..index].join(".")))
            .filter(|(_, token_path)| raw.contains_key(token_path))
        {
            dependencies.insert(token_path);
        } else {
            if pointer_visiting.iter().any(|pointer| pointer == reference) {
                return Err(DtcgError::InvalidPointer(format!(
                    "circular JSON Pointer reference `{reference}`"
                )));
            }
            let pointer = reference
                .strip_prefix('#')
                .ok_or_else(|| DtcgError::InvalidPointer(reference.into()))?;
            let target = if reference == "#/" {
                document
            } else {
                document
                    .pointer(pointer)
                    .ok_or_else(|| DtcgError::InvalidPointer(reference.into()))?
            };
            pointer_visiting.push(reference.into());
            let result =
                collect_token_dependencies(target, document, raw, dependencies, pointer_visiting);
            pointer_visiting.pop();
            result?;
        }
        for (key, item) in object {
            if key != "$ref" {
                collect_token_dependencies(item, document, raw, dependencies, pointer_visiting)?;
            }
        }
        return Ok(());
    }
    match value {
        Value::Array(values) => {
            for item in values {
                collect_token_dependencies(item, document, raw, dependencies, pointer_visiting)?;
            }
        }
        Value::Object(values) => {
            for item in values.values() {
                collect_token_dependencies(item, document, raw, dependencies, pointer_visiting)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_name(resource: &ResourceId, prefix: &str, name: &str) -> Result<(), DtcgError> {
    if name.contains(['{', '}', '.']) {
        let path = if prefix.is_empty() {
            name.into()
        } else {
            format!("{prefix}.{name}")
        };
        return Err(DtcgError::Token {
            resource: resource.clone(),
            path,
            message: "token and group names must not contain `{`, `}`, or `.`".into(),
        });
    }
    Ok(())
}

fn resolve_value(
    value: &Value,
    document: &Value,
    raw: &BTreeMap<String, RawToken>,
    resolved: &mut BTreeMap<String, ResolvedToken>,
    visiting: &mut Vec<String>,
    pointer_visiting: &mut Vec<String>,
    limits: &RuntimeLimits,
) -> Result<Value, DtcgError> {
    if let Some(reference) = value.as_str().and_then(|text| {
        text.strip_prefix('{')
            .and_then(|text| text.strip_suffix('}'))
    }) {
        resolve_token(reference, raw, resolved, visiting, pointer_visiting, limits)?;
        return resolved
            .get(reference)
            .map(|token| token.value.clone())
            .ok_or_else(|| DtcgError::Projection {
                path: reference.into(),
                message: "alias target was not resolved".into(),
            });
    }
    if let Some(object) = value.as_object()
        && object.contains_key("$ref")
    {
        return resolve_json_reference(
            object,
            document,
            raw,
            resolved,
            visiting,
            pointer_visiting,
            limits,
        );
    }
    match value {
        Value::Array(values) => values
            .iter()
            .map(|item| {
                resolve_value(
                    item,
                    document,
                    raw,
                    resolved,
                    visiting,
                    pointer_visiting,
                    limits,
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(values) => values
            .iter()
            .map(|(key, item)| {
                resolve_value(
                    item,
                    document,
                    raw,
                    resolved,
                    visiting,
                    pointer_visiting,
                    limits,
                )
                .map(|value| (key.clone(), value))
            })
            .collect::<Result<serde_json::Map<_, _>, _>>()
            .map(Value::Object),
        _ => Ok(value.clone()),
    }
}

fn resolve_json_reference(
    object: &Map<String, Value>,
    document: &Value,
    raw: &BTreeMap<String, RawToken>,
    resolved: &mut BTreeMap<String, ResolvedToken>,
    visiting: &mut Vec<String>,
    pointer_visiting: &mut Vec<String>,
    limits: &RuntimeLimits,
) -> Result<Value, DtcgError> {
    let reference = object
        .get("$ref")
        .and_then(Value::as_str)
        .ok_or_else(|| DtcgError::InvalidPointer("$ref must be a string".into()))?;
    let parts = pointer_parts(reference)?;
    if pointer_visiting.iter().any(|pointer| pointer == reference) {
        return Err(DtcgError::InvalidPointer(format!(
            "circular JSON Pointer reference `{reference}`"
        )));
    }
    pointer_visiting.push(reference.into());
    let result = resolve_json_reference_target(
        object,
        reference,
        &parts,
        document,
        raw,
        resolved,
        visiting,
        pointer_visiting,
        limits,
    );
    pointer_visiting.pop();
    result
}

#[allow(clippy::too_many_arguments)]
fn resolve_json_reference_target(
    object: &Map<String, Value>,
    reference: &str,
    parts: &[String],
    document: &Value,
    raw: &BTreeMap<String, RawToken>,
    resolved: &mut BTreeMap<String, ResolvedToken>,
    visiting: &mut Vec<String>,
    pointer_visiting: &mut Vec<String>,
    limits: &RuntimeLimits,
) -> Result<Value, DtcgError> {
    let token_target = parts
        .iter()
        .position(|part| part == "$value" || part == "$type")
        .filter(|index| *index > 0)
        .map(|index| (index, parts[..index].join(".")))
        .filter(|(_, token_path)| raw.contains_key(token_path));
    let mut referenced = if let Some((marker_index, token_path)) = token_target {
        resolve_token(
            &token_path,
            raw,
            resolved,
            visiting,
            pointer_visiting,
            limits,
        )?;
        let token = resolved
            .get(&token_path)
            .ok_or_else(|| DtcgError::Projection {
                path: token_path.clone(),
                message: "JSON Pointer target was not resolved".into(),
            })?;
        let mut value = if parts[marker_index] == "$value" {
            token.value.clone()
        } else {
            Value::String(
                token
                    .token_type
                    .as_deref()
                    .ok_or_else(|| DtcgError::Projection {
                        path: token_path,
                        message: "JSON Pointer type target is missing".into(),
                    })?
                    .into(),
            )
        };
        if marker_index + 1 < parts.len() {
            value = traverse_value(&value, &parts[marker_index + 1..])
                .cloned()
                .ok_or_else(|| DtcgError::InvalidPointer(reference.into()))?;
        }
        value
    } else {
        let pointer = reference
            .strip_prefix('#')
            .ok_or_else(|| DtcgError::InvalidPointer(reference.into()))?;
        let target = if reference == "#/" {
            document
        } else {
            document
                .pointer(pointer)
                .ok_or_else(|| DtcgError::InvalidPointer(reference.into()))?
        };
        resolve_value(
            target,
            document,
            raw,
            resolved,
            visiting,
            pointer_visiting,
            limits,
        )?
    };
    if object.len() == 1 {
        return Ok(referenced);
    }
    let referenced = referenced
        .as_object_mut()
        .ok_or_else(|| DtcgError::InvalidPointer(reference.into()))?;
    for (key, value) in object {
        if key != "$ref" {
            referenced.insert(
                key.clone(),
                resolve_value(
                    value,
                    document,
                    raw,
                    resolved,
                    visiting,
                    pointer_visiting,
                    limits,
                )?,
            );
        }
    }
    Ok(Value::Object(referenced.clone()))
}

fn traverse_value<'a>(mut value: &'a Value, parts: &[String]) -> Option<&'a Value> {
    for part in parts {
        value = match value {
            Value::Object(object) => object.get(part)?,
            Value::Array(array) => array.get(part.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(value)
}
