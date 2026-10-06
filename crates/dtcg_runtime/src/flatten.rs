//! Flattening of token sources into dotted token paths.
//!
//! Work is bounded by `RuntimeLimits::max_tokens`, which limits both the
//! emitted tokens and, separately, the `$extends` links applied (each copy of
//! an extended group). A copied group without `$extends` costs at most its
//! authored size, so the total work is bounded by the input size times the
//! limit. `$extends` links are followed iteratively and memoized by group
//! path, so a chain's types are computed once. Flattened paths are at most
//! [`MAX_PATH_DEPTH`] segments deep, which bounds the recursion: authored
//! nesting is already limited by `serde_json`'s parse recursion limit (128),
//! and only nested `$extends` copies can go deeper.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

use serde_json::{Map, Value};

use crate::{
    DtcgError, ResourceId, RuntimeLimits, SourceOccurrence, error::invalid,
    json_pointer::pointer_parts,
};

/// The deepest flattened token or group path, in segments.
pub(crate) const MAX_PATH_DEPTH: usize = 128;

/// A flattened token before resolution. `document` is the source it was
/// authored in, against which its document pointers are evaluated.
#[derive(Clone)]
pub(crate) struct RawToken {
    /// The token's own `$type`.
    pub(crate) own_type: Option<Arc<str>>,
    /// The `$type` of the nearest ancestor group, after `$extends`.
    pub(crate) group_type: Option<Arc<str>>,
    pub(crate) authored_value: Value,
    pub(crate) value: Value,
    pub(crate) extensions: Value,
    pub(crate) source: ResourceId,
    pub(crate) document: Arc<Value>,
}

/// The flattened tokens of every source, keyed by path, and the resolution
/// order: authored order, each path at its last definition.
pub(crate) struct FlattenedSources {
    pub(crate) raw: BTreeMap<String, RawToken>,
    pub(crate) order: Vec<String>,
}

pub(crate) fn flatten_sources(
    sources: &[SourceOccurrence],
    limits: &RuntimeLimits,
) -> Result<FlattenedSources, DtcgError> {
    let mut raw = BTreeMap::new();
    let mut order = Vec::new();
    let mut budget = Budget::default();
    for source in sources {
        let document = Arc::new(source.value.clone());
        let root = without_root_schema(source)?;
        let mut flattener = Flattener {
            resource: &source.resource,
            document: &document,
            limits,
            budget: &mut budget,
            raw: &mut raw,
            order: &mut order,
            links: HashMap::new(),
        };
        flattener.flatten_group(
            "",
            0,
            root.as_ref().unwrap_or(document.as_ref()),
            None,
            &mut Extending::default(),
        )?;
    }
    keep_last_definitions(&mut order);
    Ok(FlattenedSources { raw, order })
}

/// A whole-document source may name its JSON Schema with a string `$schema`
/// root member, which resolution ignores. Fragment sources, inline resolver
/// sources and the inline source supplied to a preview are not document roots,
/// so `$schema` there stays an unknown reserved property.
/// Returns the root without `$schema`, or `None` when nothing was removed.
fn without_root_schema(source: &SourceOccurrence) -> Result<Option<Value>, DtcgError> {
    let Some(schema) = source.value.get("$schema") else {
        return Ok(None);
    };
    if source.origin.resource.is_none() || !source.origin.pointer.is_empty() {
        return Ok(None);
    }
    if !schema.is_string() {
        return Err(DtcgError::Token {
            resource: source.resource.clone(),
            path: String::new(),
            message: "`$schema` must be a string".into(),
        });
    }
    let mut root = source.value.clone();
    if let Some(object) = root.as_object_mut() {
        object.shift_remove("$schema");
    }
    Ok(Some(root))
}

/// Work counted against `max_tokens` across all sources of one resolution.
#[derive(Default)]
struct Budget {
    tokens: usize,
    extensions: usize,
}

/// A group reached through `$extends`: the group it extends next, if any, and
/// the first `$type` found from it along the rest of the chain.
struct Link {
    chain_type: Option<String>,
    next: Option<String>,
}

/// One group of an `$extends` chain, nearest first.
struct ChainLink {
    path: String,
    chain_type: Option<String>,
}

/// The extended groups being copied along the current path, for cycle
/// detection.
#[derive(Default)]
struct Extending {
    paths: Vec<String>,
    active: HashSet<String>,
}

struct Flattener<'a> {
    resource: &'a ResourceId,
    document: &'a Arc<Value>,
    limits: &'a RuntimeLimits,
    budget: &'a mut Budget,
    raw: &'a mut BTreeMap<String, RawToken>,
    order: &'a mut Vec<String>,
    /// Every group path of this document reached through `$extends`.
    links: HashMap<String, Link>,
}

impl Flattener<'_> {
    /// Flattens the group or token `value` at `prefix` (`depth` segments).
    fn flatten_group(
        &mut self,
        prefix: &str,
        depth: usize,
        value: &Value,
        inherited_type: Option<&str>,
        extending: &mut Extending,
    ) -> Result<(), DtcgError> {
        if depth > MAX_PATH_DEPTH {
            return Err(DtcgError::LimitExceeded {
                kind: "path depth",
                limit: MAX_PATH_DEPTH,
            });
        }
        let object = self.validate_group(prefix, value)?;
        if is_token(object) {
            return self.insert_token(prefix, object, inherited_type);
        }
        let chain = match object.get("$extends").and_then(Value::as_str) {
            Some(reference) => self.chain(prefix, reference)?,
            None => Vec::new(),
        };
        let own_type = object
            .get("$type")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| chain.first().and_then(|link| link.chain_type.clone()))
            .or(inherited_type.map(str::to_owned));
        if let Some(link) = chain
            .iter()
            .find(|link| extending.active.contains(&link.path))
        {
            return Err(DtcgError::Token {
                resource: self.resource.clone(),
                path: prefix.into(),
                message: format!(
                    "group extension cycle: {} -> {}",
                    extending.paths.join(" -> "),
                    link.path
                ),
            });
        }
        let document = Arc::clone(self.document);
        for link in &chain {
            extending.paths.push(link.path.clone());
            extending.active.insert(link.path.clone());
        }
        // Deepest extended group first, then each extending group, so later
        // members override earlier ones.
        let mut result = Ok(());
        for link in chain.iter().rev() {
            result = self.count_extension().and_then(|()| {
                let extended = group_at_path(&document, &link.path)
                    .expect("chain links point at existing groups");
                let extended = self.validate_group(prefix, extended)?;
                let link_type = link
                    .chain_type
                    .clone()
                    .or(inherited_type.map(str::to_owned));
                self.flatten_members(prefix, depth, extended, link_type.as_deref(), extending)
            });
            if result.is_err() {
                break;
            }
        }
        if result.is_ok() {
            result = self.flatten_members(prefix, depth, object, own_type.as_deref(), extending);
        }
        for link in &chain {
            extending.active.remove(&link.path);
        }
        extending
            .paths
            .truncate(extending.paths.len() - chain.len());
        result
    }

    /// Flattens a group's `$root` token and child members, without following
    /// its `$extends`.
    fn flatten_members(
        &mut self,
        prefix: &str,
        depth: usize,
        object: &Map<String, Value>,
        group_type: Option<&str>,
        extending: &mut Extending,
    ) -> Result<(), DtcgError> {
        if let Some(root) = object.get("$root") {
            if prefix.is_empty() {
                return Err(invalid(self.resource, "root token is invalid"));
            }
            let root_path = format!("{prefix}.$root");
            if !root.as_object().is_some_and(is_token) {
                return Err(DtcgError::Token {
                    resource: self.resource.clone(),
                    path: root_path,
                    message: "`$root` must be a token".into(),
                });
            }
            self.flatten_group(&root_path, depth + 1, root, group_type, extending)?;
        }
        for (name, child) in object {
            if !name.starts_with('$') {
                validate_name(self.resource, prefix, name)?;
                let path = if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix}.{name}")
                };
                self.flatten_group(&path, depth + 1, child, group_type, extending)?;
            }
        }
        Ok(())
    }

    fn validate_group<'v>(
        &self,
        prefix: &str,
        value: &'v Value,
    ) -> Result<&'v Map<String, Value>, DtcgError> {
        let resource = self.resource;
        let object = value
            .as_object()
            .ok_or_else(|| invalid(resource, "token document groups must be objects"))?;
        let token_error = |message: String| DtcgError::Token {
            resource: resource.clone(),
            path: prefix.into(),
            message,
        };
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
                return Err(token_error(format!("unknown reserved property `{name}`")));
            }
        }
        if object.get("$type").is_some_and(|value| !value.is_string()) {
            return Err(token_error("`$type` must be a string".into()));
        }
        if object
            .get("$extends")
            .is_some_and(|value| !value.is_string())
        {
            return Err(token_error("`$extends` must be a string".into()));
        }
        if object
            .get("$description")
            .is_some_and(|value| !value.is_string())
        {
            return Err(token_error("`$description` must be a string".into()));
        }
        if object
            .get("$deprecated")
            .is_some_and(|value| !value.is_boolean() && !value.is_string())
        {
            return Err(token_error(
                "`$deprecated` must be a boolean or a string".into(),
            ));
        }
        if object
            .get("$extensions")
            .is_some_and(|value| !value.is_object())
        {
            return Err(token_error("`$extensions` must be an object".into()));
        }
        Ok(object)
    }

    /// Records the token `object` at `prefix`. A token has a `$value` or a
    /// token-level `$ref` (not both), no `$extends`, no `$root` and no child
    /// members, and a source root is never a token.
    fn insert_token(
        &mut self,
        prefix: &str,
        object: &Map<String, Value>,
        group_type: Option<&str>,
    ) -> Result<(), DtcgError> {
        let token_error = |message: String| DtcgError::Token {
            resource: self.resource.clone(),
            path: prefix.into(),
            message,
        };
        if prefix.is_empty() {
            return Err(token_error("a source root must not be a token".into()));
        }
        if object.contains_key("$extends") {
            return Err(token_error("`$extends` is only allowed on groups".into()));
        }
        if object.contains_key("$value") && object.contains_key("$ref") {
            return Err(token_error(
                "a token must not have both `$value` and `$ref`".into(),
            ));
        }
        if object
            .keys()
            .any(|name| name == "$root" || !name.starts_with('$'))
        {
            return Err(token_error(format!(
                "token `{prefix}` must not have children"
            )));
        }
        self.budget.tokens += 1;
        if self.budget.tokens > self.limits.max_tokens {
            return Err(DtcgError::LimitExceeded {
                kind: "tokens",
                limit: self.limits.max_tokens,
            });
        }
        let extensions = object.get("$extensions").cloned().unwrap_or(Value::Null);
        let own_type = object.get("$type").and_then(Value::as_str).map(Arc::from);
        let group_type = group_type.map(Arc::from);
        let token = if let Some(token_value) = object.get("$value") {
            RawToken {
                own_type,
                group_type,
                authored_value: token_value.clone(),
                value: token_value.clone(),
                extensions,
                source: self.resource.clone(),
                document: Arc::clone(self.document),
            }
        } else {
            let reference = object["$ref"]
                .as_str()
                .ok_or_else(|| token_error("token-level `$ref` must be a string".into()))?;
            if !reference.starts_with("#/") {
                return Err(token_error(
                    "token-level `$ref` must be a local JSON Pointer".into(),
                ));
            }
            pointer_parts(reference)?;
            RawToken {
                own_type,
                group_type,
                authored_value: Value::Object(object.clone()),
                value: Value::Object(Map::from_iter([(
                    "$ref".into(),
                    Value::String(reference.into()),
                )])),
                extensions,
                source: self.resource.clone(),
                document: Arc::clone(self.document),
            }
        };
        self.raw.insert(prefix.into(), token);
        self.order.push(prefix.into());
        Ok(())
    }

    fn count_extension(&mut self) -> Result<(), DtcgError> {
        self.budget.extensions += 1;
        if self.budget.extensions > self.limits.max_tokens {
            return Err(DtcgError::LimitExceeded {
                kind: "extended groups",
                limit: self.limits.max_tokens,
            });
        }
        Ok(())
    }

    /// The groups `reference` extends, transitively, nearest first. Links are
    /// computed once per group path and reused by every later chain.
    fn chain(&mut self, group_path: &str, reference: &str) -> Result<Vec<ChainLink>, DtcgError> {
        let first = extension_path(self.resource, group_path, reference)?;
        self.compute_links(&first, group_path, reference)?;
        let mut chain = Vec::new();
        let mut next = Some(first);
        while let Some(path) = next {
            let link = &self.links[&path];
            next = link.next.clone();
            chain.push(ChainLink {
                chain_type: link.chain_type.clone(),
                path,
            });
        }
        Ok(chain)
    }

    /// Records a `Link` for `first` and every group it extends, walking the
    /// chain iteratively until a known link or its end.
    fn compute_links(&mut self, first: &str, from: &str, reference: &str) -> Result<(), DtcgError> {
        let document = Arc::clone(self.document);
        let mut pending: Vec<(String, Option<String>, Option<String>)> = Vec::new();
        let mut seen = HashSet::new();
        let mut current = Some((first.to_owned(), from.to_owned(), reference.to_owned()));
        while let Some((path, from, reference)) = current.take() {
            if self.links.contains_key(&path) {
                break;
            }
            if !seen.insert(path.clone()) {
                let visited = pending
                    .iter()
                    .map(|(path, ..)| path.as_str())
                    .collect::<Vec<_>>();
                return Err(DtcgError::Token {
                    resource: self.resource.clone(),
                    path: path.clone(),
                    message: format!("group extension cycle: {} -> {path}", visited.join(" -> ")),
                });
            }
            let extended = group_at_path(&document, &path)
                .ok_or_else(|| missing_extension(self.resource, &from, &reference))?;
            let extended = self.validate_group(&path, extended)?;
            if is_token(extended) {
                return Err(DtcgError::Token {
                    resource: self.resource.clone(),
                    path: from,
                    message: format!("`$extends` target `{reference}` must be a group"),
                });
            }
            let own_type = extended
                .get("$type")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let next = match extended.get("$extends").and_then(Value::as_str) {
                Some(reference) => {
                    let next = extension_path(self.resource, &path, reference)?;
                    current = Some((next.clone(), path.clone(), reference.to_owned()));
                    Some(next)
                }
                None => None,
            };
            pending.push((path, own_type, next));
        }
        for (path, own_type, next) in pending.into_iter().rev() {
            let chain_type = own_type.or_else(|| {
                next.as_ref()
                    .and_then(|next| self.links.get(next))
                    .and_then(|link| link.chain_type.clone())
            });
            self.links.insert(path, Link { chain_type, next });
        }
        Ok(())
    }
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

fn extension_path(
    resource: &ResourceId,
    group_path: &str,
    reference: &str,
) -> Result<String, DtcgError> {
    if reference.starts_with("#/") {
        return pointer_parts(reference).map(|parts| parts.join("."));
    }
    reference
        .strip_prefix('{')
        .and_then(|path| path.strip_suffix('}'))
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| DtcgError::UnresolvedReference {
            resource: resource.clone(),
            path: group_path.into(),
            target: reference.into(),
            message: "`$extends` must be a group alias or local JSON Pointer".into(),
        })
}

fn missing_extension(resource: &ResourceId, group_path: &str, reference: &str) -> DtcgError {
    DtcgError::UnresolvedReference {
        resource: resource.clone(),
        path: group_path.into(),
        target: reference.into(),
        message: "extended group was not found".into(),
    }
}

fn group_at_path<'a>(document: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .try_fold(document, |group, segment| group.get(segment))
}

/// Whether a group member is a token: it has a `$value` or a token-level
/// `$ref`.
fn is_token(object: &Map<String, Value>) -> bool {
    object.contains_key("$value") || object.contains_key("$ref")
}

fn validate_name(resource: &ResourceId, prefix: &str, name: &str) -> Result<(), DtcgError> {
    let message = if name.is_empty() {
        "token and group names must not be empty"
    } else if name.contains(['{', '}', '.']) {
        "token and group names must not contain `{`, `}`, or `.`"
    } else {
        return Ok(());
    };
    let path = if prefix.is_empty() {
        name.into()
    } else {
        format!("{prefix}.{name}")
    };
    Err(DtcgError::Token {
        resource: resource.clone(),
        path,
        message: message.into(),
    })
}
