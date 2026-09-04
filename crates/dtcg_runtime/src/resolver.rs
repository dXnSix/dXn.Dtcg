use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::{
    DocumentReference, DtcgError, ResourceId,
    catalog::{DocumentCatalog, parse_document},
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

#[derive(Clone, Debug)]
pub struct ResolvedToken {
    pub path: Arc<str>,
    pub token_type: Option<Arc<str>>,
    pub authored_value: Value,
    pub value: Value,
    pub source: ResourceId,
}

#[derive(Clone, Debug, Default)]
pub struct ResolvedTokens {
    tokens: BTreeMap<String, ResolvedToken>,
    order: Vec<String>,
    fingerprint: Arc<str>,
}

impl ResolvedTokens {
    pub fn get(&self, path: &str) -> Option<&ResolvedToken> {
        self.tokens.get(path)
    }
    pub fn iter(&self) -> impl Iterator<Item = (&str, &ResolvedToken)> {
        self.order
            .iter()
            .filter_map(|path| self.tokens.get(path).map(|token| (path.as_str(), token)))
    }
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
}

#[derive(Clone)]
struct Source {
    id: ResourceId,
    fragment: Option<Arc<str>>,
    inline: Option<Value>,
    overrides: Map<String, Value>,
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
    let mut order = Vec::new();
    for source in sources {
        flatten_source(catalog, &source, &mut raw, &mut order)?;
    }
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
    let mut hasher = Sha256::new();
    for path in &order {
        let token = resolved
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
    Ok(Resolution::Ready(ResolvedTokens {
        tokens: resolved,
        order,
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
    let mut selections = BTreeMap::new();
    for (name, value) in selection {
        if selections.insert(*name, *value).is_some() {
            return Err(invalid(root, &format!("duplicate input `{name}`")));
        }
    }
    let mut queue = vec![(
        Source {
            id: root.clone(),
            fragment: None,
            inline: None,
            overrides: Map::new(),
            referenced_from: root.clone(),
            original: root.as_str().into(),
        },
        0usize,
        Vec::new(),
    )];
    let mut terminal = Vec::new();
    let mut missing = BTreeMap::<ResourceId, DocumentRequest>::new();
    let mut expanded = BTreeSet::new();
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
        if source.inline.is_none() && !catalog.contains(&source.id) {
            missing.entry(source.id.clone()).or_insert(DocumentRequest {
                resource_id: source.id,
                referenced_from: source.referenced_from,
                original_reference: source.original,
            });
            continue;
        }
        let value = source_value(catalog, &source)?;
        if !is_resolver(&value) {
            terminal.push(source);
            continue;
        }
        if ancestors.contains(&source.id) {
            return Err(DtcgError::DocumentCycle(source.id.to_string()));
        }
        if !expanded.insert((source.id.clone(), source.fragment.clone())) {
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

fn source_value(catalog: &DocumentCatalog, source: &Source) -> Result<Value, DtcgError> {
    let document = match &source.inline {
        Some(value) => value.clone(),
        None => {
            let json = catalog
                .get(&source.id)
                .ok_or_else(|| DtcgError::UnknownDocument(source.id.clone()))?;
            parse_document(&source.id, json)?
        }
    };
    let mut value = source
        .fragment
        .as_deref()
        .map_or(Ok(document.clone()), |fragment| {
            document
                .pointer(fragment)
                .cloned()
                .ok_or_else(|| DtcgError::InvalidPointer(fragment.into()))
        })?;
    if !source.overrides.is_empty() {
        let object = value
            .as_object_mut()
            .ok_or_else(|| invalid(&source.id, "reference overrides require an object target"))?;
        for (key, override_value) in &source.overrides {
            object.insert(key.clone(), override_value.clone());
        }
    }
    Ok(value)
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
    if object.get("version").and_then(Value::as_str) != Some("2025.10") {
        return Err(invalid(parent, "resolver version must be `2025.10`"));
    }
    let sets = optional_object(parent, object, "sets")?;
    let modifiers = optional_object(parent, object, "modifiers")?;
    let order = object
        .get("resolutionOrder")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid(parent, "resolutionOrder is required"))?;
    validate_inputs(parent, modifiers, order, selections, include_inactive)?;

    let mut result = Vec::new();
    let mut inline_names = BTreeSet::new();
    for step in order {
        let (kind, name, definition) = resolution_step(parent, value, step)?;
        if step.get("$ref").is_none() && !inline_names.insert(name.clone()) {
            return Err(invalid(
                parent,
                &format!("duplicate inline resolution name `{name}`"),
            ));
        }
        let documents = match kind.as_str() {
            "set" => set_documents(parent, &definition)?,
            "modifier" => {
                modifier_documents(parent, &definition, &name, selections, include_inactive)?
            }
            _ => {
                return Err(invalid(
                    parent,
                    "resolution step type must be `set` or `modifier`",
                ));
            }
        };
        for document in documents {
            append_source(parent, value, sets, &document, &mut result, &mut Vec::new())?;
        }
    }
    Ok(result)
}

fn optional_object<'a>(
    parent: &ResourceId,
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<Option<&'a Map<String, Value>>, DtcgError> {
    object
        .get(key)
        .map(|value| {
            value
                .as_object()
                .ok_or_else(|| invalid(parent, &format!("resolver {key} must be an object")))
        })
        .transpose()
}

fn resolution_step(
    parent: &ResourceId,
    document: &Value,
    step: &Value,
) -> Result<(String, String, Value), DtcgError> {
    let object = step
        .as_object()
        .ok_or_else(|| invalid(parent, "resolution step must be an object"))?;
    if let Some(pointer) = object.get("$ref").and_then(Value::as_str) {
        let parts = pointer_parts(pointer)?;
        let (kind, name) = match parts.as_slice() {
            [kind, name] if kind == "sets" => ("set", name.clone()),
            [kind, name] if kind == "modifiers" => ("modifier", name.clone()),
            _ => {
                return Err(invalid(
                    parent,
                    &format!("unknown resolver source `{pointer}`"),
                ));
            }
        };
        return Ok((
            kind.into(),
            name,
            resolve_local_reference(parent, document, object)?,
        ));
    }
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(parent, "inline resolution step requires `type`"))?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| invalid(parent, "inline resolution step requires `name`"))?;
    Ok((kind.into(), name.into(), step.clone()))
}

fn resolve_local_reference(
    parent: &ResourceId,
    document: &Value,
    reference: &Map<String, Value>,
) -> Result<Value, DtcgError> {
    let pointer = reference
        .get("$ref")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(parent, "reference object requires a string `$ref`"))?;
    let pointer = pointer
        .strip_prefix('#')
        .ok_or_else(|| invalid(parent, "resolver structure reference must be local"))?;
    if pointer.starts_with("/resolutionOrder/") {
        return Err(invalid(
            parent,
            "references into resolutionOrder are forbidden",
        ));
    }
    let mut target = document
        .pointer(pointer)
        .cloned()
        .ok_or_else(|| DtcgError::InvalidPointer(pointer.into()))?;
    let target = target
        .as_object_mut()
        .ok_or_else(|| invalid(parent, "reference target must be an object"))?;
    for (key, value) in reference {
        if key != "$ref" {
            target.insert(key.clone(), value.clone());
        }
    }
    Ok(Value::Object(target.clone()))
}

fn set_documents(parent: &ResourceId, set: &Value) -> Result<Vec<Value>, DtcgError> {
    set.get("sources")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| invalid(parent, "set sources are required"))
}

fn modifier_documents(
    parent: &ResourceId,
    modifier: &Value,
    name: &str,
    selections: &BTreeMap<&str, &str>,
    include_inactive: bool,
) -> Result<Vec<Value>, DtcgError> {
    let modifier = modifier
        .as_object()
        .ok_or_else(|| invalid(parent, "modifier must be an object"))?;
    let contexts = modifier
        .get("contexts")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid(parent, "modifier contexts are required"))?;
    if contexts.is_empty() {
        return Err(invalid(parent, "modifier contexts must not be empty"));
    }
    if include_inactive {
        let mut documents = Vec::new();
        for sources in contexts.values() {
            let sources = sources
                .as_array()
                .ok_or_else(|| invalid(parent, "modifier context must be an array"))?;
            documents.extend(sources.iter().cloned());
        }
        return Ok(documents);
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
        .ok_or_else(|| DtcgError::Token {
            resource: parent.clone(),
            path: format!("#/modifiers/{name}"),
            message: "selected modifier context is unknown".into(),
        })
}

fn validate_inputs(
    parent: &ResourceId,
    root_modifiers: Option<&Map<String, Value>>,
    order: &[Value],
    selections: &BTreeMap<&str, &str>,
    include_inactive: bool,
) -> Result<(), DtcgError> {
    let mut modifiers = BTreeMap::<String, Value>::new();
    if let Some(root_modifiers) = root_modifiers {
        modifiers.extend(
            root_modifiers
                .iter()
                .map(|(name, modifier)| (name.clone(), modifier.clone())),
        );
    }
    for step in order {
        let Some(object) = step.as_object() else {
            continue;
        };
        if object.get("$ref").is_none()
            && object.get("type").and_then(Value::as_str) == Some("modifier")
        {
            let name = object
                .get("name")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
                .ok_or_else(|| invalid(parent, "inline modifier requires `name`"))?;
            modifiers.insert(name.into(), step.clone());
        }
    }
    for (name, modifier) in &modifiers {
        let modifier = modifier
            .as_object()
            .ok_or_else(|| invalid(parent, &format!("modifier `{name}` must be an object")))?;
        let contexts = modifier
            .get("contexts")
            .and_then(Value::as_object)
            .filter(|contexts| !contexts.is_empty())
            .ok_or_else(|| invalid(parent, &format!("modifier `{name}` requires contexts")))?;
        for sources in contexts.values() {
            if !sources.is_array() {
                return Err(invalid(
                    parent,
                    &format!("modifier `{name}` context must be an array"),
                ));
            }
        }
        if let Some(default) = modifier.get("default") {
            let default = default.as_str().ok_or_else(|| {
                invalid(
                    parent,
                    &format!("modifier `{name}` default must be a string"),
                )
            })?;
            if !contexts.contains_key(default) {
                return Err(invalid(
                    parent,
                    &format!("modifier `{name}` default context is unknown"),
                ));
            }
        }
        if !include_inactive
            && modifier.get("default").is_none()
            && !selections.contains_key(name.as_str())
        {
            return Err(invalid(
                parent,
                &format!("missing input for modifier `{name}`"),
            ));
        }
    }
    if include_inactive {
        return Ok(());
    }
    for (name, choice) in selections {
        let modifier = modifiers
            .get(*name)
            .and_then(Value::as_object)
            .ok_or_else(|| invalid(parent, &format!("unknown input `{name}`")))?;
        if !modifier
            .get("contexts")
            .and_then(Value::as_object)
            .is_some_and(|contexts| contexts.contains_key(*choice))
        {
            return Err(DtcgError::Token {
                resource: parent.clone(),
                path: format!("#/modifiers/{name}"),
                message: "selected modifier context is unknown".into(),
            });
        }
    }
    Ok(())
}

fn append_source(
    parent: &ResourceId,
    document: &Value,
    sets: Option<&Map<String, Value>>,
    value: &Value,
    result: &mut Vec<Source>,
    references: &mut Vec<String>,
) -> Result<(), DtcgError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid(parent, "document source must be an object"))?;
    let Some(reference) = object.get("$ref") else {
        result.push(Source {
            id: parent.clone(),
            fragment: None,
            inline: Some(value.clone()),
            overrides: Map::new(),
            referenced_from: parent.clone(),
            original: "inline".into(),
        });
        return Ok(());
    };
    let reference = reference
        .as_str()
        .ok_or_else(|| invalid(parent, "reference object `$ref` must be a string"))?;
    if reference.starts_with("#/resolutionOrder/") || reference.starts_with("#/modifiers/") {
        return Err(invalid(
            parent,
            &format!("forbidden source reference `{reference}`"),
        ));
    }
    if reference.starts_with('#') {
        if references.iter().any(|active| active == reference) {
            return Err(invalid(
                parent,
                &format!("reference cycle at `{reference}`"),
            ));
        }
        references.push(reference.into());
        let parts = pointer_parts(reference)?;
        if let [kind, name] = parts.as_slice()
            && kind == "sets"
        {
            let set = sets
                .and_then(|sets| sets.get(name))
                .ok_or_else(|| invalid(parent, &format!("unknown set `{name}`")))?;
            let mut set = set.clone();
            let set_object = set
                .as_object_mut()
                .ok_or_else(|| invalid(parent, "referenced set must be an object"))?;
            for (key, value) in object {
                if key != "$ref" {
                    set_object.insert(key.clone(), value.clone());
                }
            }
            for source in set_documents(parent, &set)? {
                append_source(parent, document, sets, &source, result, references)?;
            }
        } else {
            let resolved = resolve_local_reference(parent, document, object)?;
            append_source(parent, document, sets, &resolved, result, references)?;
        }
        references.pop();
        return Ok(());
    }

    let reference: DocumentReference = parent.resolve(reference)?;
    let overrides = object
        .iter()
        .filter(|(key, _)| key.as_str() != "$ref")
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    result.push(Source {
        id: reference.resource_id().clone(),
        fragment: reference.fragment().map(Arc::from),
        inline: None,
        overrides,
        referenced_from: parent.clone(),
        original: reference.original().into(),
    });
    Ok(())
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
    order: &mut Vec<String>,
) -> Result<(), DtcgError> {
    let value = Arc::new(source_value(catalog, source)?);
    let mut output = FlattenedTokens { target, order };
    flatten_group(
        &source.id,
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
    if output.target.insert(path.into(), token).is_some() {
        output.order.retain(|current| current != path);
    }
    output.order.push(path.into());
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
    let token_type = token
        .token_type
        .clone()
        .or(alias_type)
        .ok_or_else(|| DtcgError::Token {
            resource: token.source.clone(),
            path: path.into(),
            message: "token requires a DTCG type".into(),
        })?;
    validate_value(&token_type, &value).map_err(|error| DtcgError::Token {
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

fn invalid(resource: &ResourceId, message: &str) -> DtcgError {
    DtcgError::InvalidDocument {
        resource: resource.clone(),
        message: message.into(),
    }
}
