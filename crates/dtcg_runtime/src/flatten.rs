//! Folding of token sources into one merged token structure, and flattening
//! of that structure into dotted token paths (Resolver §6.2–§6.3).
//!
//! Every terminal source occurrence is folded, in discovery order, into one
//! merged JSON tree: groups merge member by member, group `$` properties merge
//! per key (later values win; `$extensions` objects merge recursively), a
//! token replaces any earlier token at its path, and a token meeting a group
//! at the same path is a collision error. The merged tree is then flattened
//! once, so type inheritance, `$extends`, `$deprecated` inheritance and every
//! JSON Pointer see all active sources as if they were one document.
//!
//! Work is bounded by `RuntimeLimits::max_tokens`, which limits both the
//! emitted tokens and, separately, the JSON nodes `$extends` copies. `$extends`
//! expansions are resolved iteratively in dependency order and memoized by
//! group path, so a group may extend a group in another source or one that is
//! itself produced by `$extends`. Flattened paths are at most
//! [`MAX_PATH_DEPTH`] segments deep, which bounds the recursion: authored
//! nesting is already limited by `serde_json`'s parse recursion limit (128),
//! and only nested `$extends` copies can go deeper.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    rc::Rc,
    sync::Arc,
};

use serde_json::{Map, Value};

use crate::{
    Deprecation, DtcgError, ResourceId, RuntimeLimits, SourceOccurrence, error::invalid,
    json_pointer::pointer_parts,
};

/// The deepest flattened token or group path, in segments.
pub(crate) const MAX_PATH_DEPTH: usize = 128;

/// An internal member of groups copied by `$extends`: the path of the group
/// the copy was made from. Authored documents cannot contain it, because
/// validation rejects unknown `$` members; groups without it are authored at
/// their own path.
const ORIGIN: &str = "$origin";

/// A flattened token before resolution. `document` is the merged token
/// structure, against which its document pointers are evaluated.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RawToken {
    /// The token's own `$type`.
    pub(crate) own_type: Option<Arc<str>>,
    /// The `$type` of the nearest ancestor group, after `$extends`.
    pub(crate) group_type: Option<Arc<str>>,
    pub(crate) authored_value: Value,
    pub(crate) value: Value,
    pub(crate) extensions: Value,
    pub(crate) description: Option<Arc<str>>,
    /// The token's own `$deprecated`, or else the nearest group's.
    pub(crate) deprecated: Deprecation,
    pub(crate) source: ResourceId,
    pub(crate) document: Arc<Value>,
}

/// A flattened group's own properties, after merge and `$extends`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RawGroup {
    /// The `$type` its tokens inherit: its own, or else its nearest
    /// ancestor's.
    pub(crate) token_type: Option<Arc<str>>,
    pub(crate) description: Option<Arc<str>>,
    pub(crate) deprecated: Option<Deprecation>,
    pub(crate) extensions: Value,
}

/// The flattened tokens of the merged structure, keyed by path; the
/// resolution order (authored order, each path at its last definition); the
/// groups that contain tokens; and whether any group uses `$extends`.
pub(crate) struct FlattenedSources {
    pub(crate) raw: BTreeMap<String, RawToken>,
    pub(crate) order: Vec<String>,
    pub(crate) groups: BTreeMap<String, RawGroup>,
    pub(crate) has_extends: bool,
}

pub(crate) fn flatten_sources(
    sources: &[SourceOccurrence],
    limits: &RuntimeLimits,
) -> Result<FlattenedSources, DtcgError> {
    let mut merger = Merger::default();
    let mut root = Map::new();
    for source in sources {
        merger.fold(&mut root, source)?;
    }
    let has_extends = merger.groups.values().any(|group| group.extends.is_some());
    let fallback = sources
        .first()
        .map(|source| source.resource.clone())
        .unwrap_or_else(|| ResourceId::parse("tokens").expect("static id"));
    let document = Arc::new(Value::Object(root));
    let mut flattener = Flattener {
        root: document.as_object().expect("the merged root is an object"),
        document: &document,
        tokens: &merger.tokens,
        origins: &merger.groups,
        fallback: &fallback,
        limits,
        raw: BTreeMap::new(),
        keys: HashMap::new(),
        groups: BTreeMap::new(),
        expansions: HashMap::new(),
        emitted: 0,
        copied: 0,
        counter: 0,
        active_origins: Vec::new(),
    };
    let root = flattener.root;
    flattener.flatten_group("", 0, root, &Inherited::default(), None)?;
    let Flattener {
        raw,
        keys,
        mut groups,
        ..
    } = flattener;
    let mut order = raw.keys().cloned().collect::<Vec<_>>();
    order.sort_by_key(|path| keys[path]);
    let mut populated = BTreeSet::new();
    for path in raw.keys() {
        populated.insert(String::new());
        let mut end = path.len();
        while let Some(index) = path[..end].rfind('.') {
            populated.insert(path[..index].to_owned());
            end = index;
        }
    }
    groups.retain(|path, _| populated.contains(path));
    Ok(FlattenedSources {
        raw,
        order,
        groups,
        has_extends,
    })
}

/// Where an authored token was last defined: its document and its position in
/// the global definition sequence.
struct TokenOrigin {
    resource: ResourceId,
    sequence: u64,
}

/// The document that first defined a group, and the document and position of
/// the group occurrence that last set its `$extends`.
struct GroupOrigin {
    resource: ResourceId,
    extends: Option<(ResourceId, u64)>,
}

#[derive(Default)]
struct Merger {
    tokens: HashMap<String, TokenOrigin>,
    groups: HashMap<String, GroupOrigin>,
    sequence: u64,
}

impl Merger {
    fn next(&mut self) -> u64 {
        self.sequence += 1;
        self.sequence
    }

    fn fold(
        &mut self,
        root: &mut Map<String, Value>,
        source: &SourceOccurrence,
    ) -> Result<(), DtcgError> {
        let stripped = without_root_schema(source)?;
        let value = stripped.as_ref().unwrap_or(&source.value);
        let resource = &source.resource;
        let object = value
            .as_object()
            .ok_or_else(|| invalid(resource, "token document groups must be objects"))?;
        validate_group(resource, "", object)?;
        if is_token(object) {
            return Err(DtcgError::Token {
                resource: resource.clone(),
                path: String::new(),
                message: "a source root must not be a token".into(),
            });
        }
        self.merge_group(resource, root, object, "")
    }

    fn merge_group(
        &mut self,
        resource: &ResourceId,
        target: &mut Map<String, Value>,
        source: &Map<String, Value>,
        path: &str,
    ) -> Result<(), DtcgError> {
        let entry = self.next();
        let origin = self
            .groups
            .entry(path.to_owned())
            .or_insert_with(|| GroupOrigin {
                resource: resource.clone(),
                extends: None,
            });
        if source.contains_key("$extends") {
            origin.extends = Some((resource.clone(), entry));
        }
        for (name, value) in source {
            if name == "$root" {
                if path.is_empty() {
                    return Err(invalid(resource, "root token is invalid"));
                }
                let root_path = format!("{path}.$root");
                if !value.as_object().is_some_and(is_token) {
                    return Err(DtcgError::Token {
                        resource: resource.clone(),
                        path: root_path,
                        message: "`$root` must be a token".into(),
                    });
                }
                self.merge_member(resource, target, name, value, &root_path)?;
            } else if name == "$extensions" {
                merge_extensions(target.entry(name.clone()).or_insert(Value::Null), value);
            } else if name.starts_with('$') {
                target.insert(name.clone(), value.clone());
            } else {
                validate_name(resource, path, name)?;
                let member_path = if path.is_empty() {
                    name.clone()
                } else {
                    format!("{path}.{name}")
                };
                self.merge_member(resource, target, name, value, &member_path)?;
            }
        }
        Ok(())
    }

    fn merge_member(
        &mut self,
        resource: &ResourceId,
        target: &mut Map<String, Value>,
        name: &str,
        value: &Value,
        path: &str,
    ) -> Result<(), DtcgError> {
        let object = value
            .as_object()
            .ok_or_else(|| invalid(resource, "token document groups must be objects"))?;
        validate_group(resource, path, object)?;
        let existing = target.get(name).and_then(Value::as_object);
        if is_token(object) {
            validate_token(resource, path, object)?;
            if existing.is_some_and(|existing| !is_token(existing)) {
                return Err(collision(path, resource, &self.groups[path].resource));
            }
            target.insert(name.to_owned(), value.clone());
            let sequence = self.next();
            self.tokens.insert(
                path.to_owned(),
                TokenOrigin {
                    resource: resource.clone(),
                    sequence,
                },
            );
            return Ok(());
        }
        if existing.is_some_and(is_token) {
            return Err(collision(path, &self.tokens[path].resource, resource));
        }
        let child = target
            .entry(name.to_owned())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .expect("a group member is an object");
        self.merge_group(resource, child, object, path)
    }
}

fn collision(path: &str, token: &ResourceId, group: &ResourceId) -> DtcgError {
    DtcgError::Token {
        resource: token.clone(),
        path: path.into(),
        message: format!("path is a token in {token} and a group in {group}"),
    }
}

/// Merges `source` into `target`: objects member by member, recursively; any
/// other value replaces the target.
///
/// Group and root `$extensions` merge this way across sources and through
/// `$extends`, unlike other group properties, which replace per key: a pointer
/// an earlier source declares into `#/…/$extensions/…` keeps resolving when a
/// later source adds other members, while the later source still wins at every
/// location it sets. Token `$extensions` are not merged; tokens replace whole.
fn merge_extensions(target: &mut Value, source: &Value) {
    match (target.as_object_mut(), source.as_object()) {
        (Some(target), Some(source)) => {
            for (key, value) in source {
                match target.get_mut(key) {
                    Some(existing) if existing.is_object() && value.is_object() => {
                        merge_extensions(existing, value);
                    }
                    _ => {
                        target.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        _ => *target = source.clone(),
    }
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

/// What a group passes to its members.
#[derive(Clone, Default)]
struct Inherited {
    token_type: Option<Arc<str>>,
    deprecated: Deprecation,
}

/// The authored `$extends` an expansion belongs to: its definition sequence
/// and document. Copied tokens are ordered and attributed by it.
type Anchor<'a> = (u64, &'a ResourceId);

/// A group's content while locating a path: borrowed from the merged tree, or
/// a memoized `$extends` expansion.
enum Content<'a> {
    Merged(&'a Map<String, Value>),
    Expanded(Rc<Map<String, Value>>),
}

impl Content<'_> {
    fn map(&self) -> &Map<String, Value> {
        match self {
            Self::Merged(map) => map,
            Self::Expanded(map) => map,
        }
    }
}

enum Located {
    Found(Rc<Map<String, Value>>),
    /// The group at this path must be expanded first.
    Needs(String),
    Missing,
}

struct Flattener<'a> {
    root: &'a Map<String, Value>,
    document: &'a Arc<Value>,
    tokens: &'a HashMap<String, TokenOrigin>,
    origins: &'a HashMap<String, GroupOrigin>,
    fallback: &'a ResourceId,
    limits: &'a RuntimeLimits,
    raw: BTreeMap<String, RawToken>,
    /// Each token's position: `(sequence, 0)` for an authored token, and
    /// `(anchor sequence, counter)` for a token copied by `$extends`.
    keys: HashMap<String, (u64, u64)>,
    groups: BTreeMap<String, RawGroup>,
    /// Expanded content of groups with `$extends`, and of groups located
    /// inside expansions, by path.
    expansions: HashMap<String, Rc<Map<String, Value>>>,
    emitted: usize,
    copied: usize,
    counter: u64,
    /// The origins of the groups along the path being flattened.
    active_origins: Vec<String>,
}

impl<'a> Flattener<'a> {
    /// Flattens the group `content` at `path` (`depth` segments).
    fn flatten_group(
        &mut self,
        path: &str,
        depth: usize,
        content: &Map<String, Value>,
        inherited: &Inherited,
        anchor: Option<Anchor<'a>>,
    ) -> Result<(), DtcgError> {
        if depth > MAX_PATH_DEPTH {
            return Err(DtcgError::LimitExceeded {
                kind: "path depth",
                limit: MAX_PATH_DEPTH,
            });
        }
        let mut anchor = anchor;
        let expanded;
        let content = match content.get("$extends").and_then(Value::as_str) {
            Some(reference) => {
                let resource = self.extends_resource(path, anchor);
                let target = extension_path(resource, path, reference)?;
                // Extending the group itself or an ancestor copies the
                // extending group into itself, without end. Other cycles are
                // found while resolving the expansions' dependencies.
                if target.is_empty() || path == target || path.starts_with(&format!("{target}.")) {
                    return Err(DtcgError::Token {
                        resource: resource.clone(),
                        path: path.into(),
                        message: format!("group extension cycle: {path} -> {target}"),
                    });
                }
                if let Some((resource, sequence)) = self
                    .origins
                    .get(path)
                    .and_then(|origin| origin.extends.as_ref())
                {
                    anchor = Some((*sequence, resource));
                }
                expanded = self.expansion(path)?;
                expanded.as_ref()
            }
            None => content,
        };
        // A group's content depends only on where it was authored, so a group
        // copied into a copy of itself would repeat without end.
        let origin = content
            .get(ORIGIN)
            .and_then(Value::as_str)
            .unwrap_or(path)
            .to_owned();
        if self.active_origins.contains(&origin) {
            return Err(DtcgError::Token {
                resource: self.extends_resource(path, anchor).clone(),
                path: path.into(),
                message: format!("group extension cycle: {path} copies {origin} into itself"),
            });
        }
        self.active_origins.push(origin);
        let result = self.flatten_members(path, depth, content, inherited, anchor);
        self.active_origins.pop();
        result
    }

    fn flatten_members(
        &mut self,
        path: &str,
        depth: usize,
        content: &Map<String, Value>,
        inherited: &Inherited,
        anchor: Option<Anchor<'a>>,
    ) -> Result<(), DtcgError> {
        let own_type = content.get("$type").and_then(Value::as_str).map(Arc::from);
        let own_deprecated = content.get("$deprecated").map(Deprecation::from_json);
        let inherited = Inherited {
            token_type: own_type.or_else(|| inherited.token_type.clone()),
            deprecated: own_deprecated
                .clone()
                .unwrap_or_else(|| inherited.deprecated.clone()),
        };
        self.groups.insert(
            path.to_owned(),
            RawGroup {
                token_type: inherited.token_type.clone(),
                description: content
                    .get("$description")
                    .and_then(Value::as_str)
                    .map(Arc::from),
                deprecated: own_deprecated,
                extensions: content.get("$extensions").cloned().unwrap_or(Value::Null),
            },
        );
        if let Some(root) = content.get("$root").and_then(Value::as_object) {
            self.emit(&format!("{path}.$root"), root, &inherited, anchor)?;
        }
        for (name, member) in content {
            if name.starts_with('$') {
                continue;
            }
            let member_path = if path.is_empty() {
                name.clone()
            } else {
                format!("{path}.{name}")
            };
            let member = member.as_object().expect("merged members are objects");
            if is_token(member) {
                self.emit(&member_path, member, &inherited, anchor)?;
            } else {
                self.flatten_group(&member_path, depth + 1, member, &inherited, anchor)?;
            }
        }
        Ok(())
    }

    fn emit(
        &mut self,
        path: &str,
        object: &Map<String, Value>,
        inherited: &Inherited,
        anchor: Option<Anchor<'a>>,
    ) -> Result<(), DtcgError> {
        self.emitted += 1;
        if self.emitted > self.limits.max_tokens {
            return Err(DtcgError::LimitExceeded {
                kind: "tokens",
                limit: self.limits.max_tokens,
            });
        }
        let (key, source) = match self.tokens.get(path) {
            Some(origin) => ((origin.sequence, 0), origin.resource.clone()),
            None => {
                self.counter += 1;
                let (sequence, resource) = anchor.unwrap_or((0, self.fallback));
                ((sequence, self.counter), resource.clone())
            }
        };
        let (authored_value, value) = match object.get("$value") {
            Some(value) => (value.clone(), value.clone()),
            None => (
                Value::Object(object.clone()),
                Value::Object(Map::from_iter([("$ref".into(), object["$ref"].clone())])),
            ),
        };
        self.raw.insert(
            path.to_owned(),
            RawToken {
                own_type: object.get("$type").and_then(Value::as_str).map(Arc::from),
                group_type: inherited.token_type.clone(),
                authored_value,
                value,
                extensions: object.get("$extensions").cloned().unwrap_or(Value::Null),
                description: object
                    .get("$description")
                    .and_then(Value::as_str)
                    .map(Arc::from),
                deprecated: object
                    .get("$deprecated")
                    .map(Deprecation::from_json)
                    .unwrap_or_else(|| inherited.deprecated.clone()),
                source,
                document: Arc::clone(self.document),
            },
        );
        self.keys.insert(path.to_owned(), key);
        Ok(())
    }

    /// The document an `$extends` at `path` is reported against.
    fn extends_resource(&self, path: &str, anchor: Option<Anchor<'a>>) -> &'a ResourceId {
        self.origins
            .get(path)
            .map(|origin| {
                origin
                    .extends
                    .as_ref()
                    .map_or(&origin.resource, |(resource, _)| resource)
            })
            .or(anchor.map(|(_, resource)| resource))
            .unwrap_or(self.fallback)
    }

    /// The expanded content of the group with `$extends` at `path`. Every
    /// expansion it depends on (its target, and groups with `$extends` on
    /// the way to it) is resolved first, iteratively, so chains of any length
    /// use no recursion; a dependency on an expansion in progress is a cycle.
    fn expansion(&mut self, path: &str) -> Result<Rc<Map<String, Value>>, DtcgError> {
        let mut pending = vec![path.to_owned()];
        while let Some(top) = pending.last().cloned() {
            if self.expansions.contains_key(&top) {
                pending.pop();
                continue;
            }
            let own = match self.locate(&top, false)? {
                Located::Found(own) => own,
                Located::Needs(dependency) => {
                    self.require(&mut pending, dependency)?;
                    continue;
                }
                Located::Missing => unreachable!("an expanded group exists"),
            };
            let Some(reference) = own.get("$extends").and_then(Value::as_str) else {
                // A group located inside an expansion without its own `$extends`.
                self.expansions.insert(top, own);
                pending.pop();
                continue;
            };
            let resource = self.extends_resource(&top, None);
            let target_path = extension_path(resource, &top, reference)?;
            match self.locate(&target_path, true)? {
                Located::Needs(dependency) => self.require(&mut pending, dependency)?,
                Located::Missing => {
                    return Err(DtcgError::UnresolvedReference {
                        resource: resource.clone(),
                        path: top,
                        target: reference.into(),
                        message: "extended group was not found".into(),
                    });
                }
                Located::Found(target) => {
                    if is_token(&target) {
                        return Err(DtcgError::Token {
                            resource: resource.clone(),
                            path: top,
                            message: format!("`$extends` target `{reference}` must be a group"),
                        });
                    }
                    self.charge(&target)?;
                    let mut expanded = target.as_ref().clone();
                    mark_origins(&mut expanded, &target_path);
                    merge_own(&mut expanded, &own);
                    self.expansions.insert(top, Rc::new(expanded));
                    pending.pop();
                }
            }
        }
        Ok(Rc::clone(&self.expansions[path]))
    }

    /// Queues `dependency` for expansion, or reports the cycle it closes.
    fn require(&self, pending: &mut Vec<String>, dependency: String) -> Result<(), DtcgError> {
        if let Some(index) = pending.iter().position(|path| *path == dependency) {
            let mut cycle = pending[index + 1..].to_vec();
            cycle.push(dependency);
            let first = cycle[0].clone();
            return Err(DtcgError::Token {
                resource: self.extends_resource(&first, None).clone(),
                path: first.clone(),
                message: format!("group extension cycle: {} -> {first}", cycle.join(" -> ")),
            });
        }
        pending.push(dependency);
        Ok(())
    }

    /// Finds the content at `path`, walking from the merged root through
    /// expanded groups. `expand_last` also requires the group at `path` itself
    /// to be expanded. A token is returned as found content.
    fn locate(&mut self, path: &str, expand_last: bool) -> Result<Located, DtcgError> {
        let segments = if path.is_empty() {
            Vec::new()
        } else {
            path.split('.').collect::<Vec<_>>()
        };
        let mut current = Content::Merged(self.root);
        let mut current_path = String::new();
        if self.root.contains_key("$extends") && (expand_last || !segments.is_empty()) {
            match self.expansions.get("") {
                Some(expanded) => current = Content::Expanded(Rc::clone(expanded)),
                None => return Ok(Located::Needs(String::new())),
            }
        }
        for (index, segment) in segments.iter().enumerate() {
            let last = index + 1 == segments.len();
            if !current_path.is_empty() {
                current_path.push('.');
            }
            current_path.push_str(segment);
            let Some(child) = current.map().get(*segment).and_then(Value::as_object) else {
                return Ok(Located::Missing);
            };
            if is_token(child) {
                return Ok(if last {
                    Located::Found(Rc::new(child.clone()))
                } else {
                    Located::Missing
                });
            }
            if child.contains_key("$extends") && (expand_last || !last) {
                match self.expansions.get(&current_path) {
                    Some(expanded) => {
                        current = Content::Expanded(Rc::clone(expanded));
                        continue;
                    }
                    None => return Ok(Located::Needs(current_path)),
                }
            }
            current = match current {
                Content::Merged(map) => {
                    Content::Merged(map[*segment].as_object().expect("the child is an object"))
                }
                Content::Expanded(_) => {
                    let copy = match self.expansions.get(&current_path) {
                        Some(copy) => Rc::clone(copy),
                        None => {
                            self.charge(child)?;
                            let copy = Rc::new(child.clone());
                            if !child.contains_key("$extends") {
                                self.expansions
                                    .insert(current_path.clone(), Rc::clone(&copy));
                            }
                            copy
                        }
                    };
                    Content::Expanded(copy)
                }
            };
        }
        Ok(Located::Found(match current {
            Content::Merged(map) => Rc::new(map.clone()),
            Content::Expanded(map) => map,
        }))
    }

    /// Charges a copy of `content` against the `$extends` copy budget.
    fn charge(&mut self, content: &Map<String, Value>) -> Result<(), DtcgError> {
        self.copied = self.copied.saturating_add(group_nodes(content));
        if self.copied > self.limits.max_tokens {
            return Err(DtcgError::LimitExceeded {
                kind: "tokens",
                limit: self.limits.max_tokens,
            });
        }
        Ok(())
    }
}

/// The tokens and groups in `content`, itself included.
fn group_nodes(content: &Map<String, Value>) -> usize {
    1 + content
        .iter()
        .filter(|(name, _)| !name.starts_with('$') || *name == "$root")
        .filter_map(|(_, member)| member.as_object())
        .map(|member| {
            if is_token(member) {
                1
            } else {
                group_nodes(member)
            }
        })
        .sum::<usize>()
}

/// Records, on every nested group copied from the group at `path` that has
/// no origin yet, the path it was authored at.
fn mark_origins(content: &mut Map<String, Value>, path: &str) {
    for (name, member) in content.iter_mut() {
        if name.starts_with('$') {
            continue;
        }
        let Some(member) = member.as_object_mut() else {
            continue;
        };
        if is_token(member) || member.contains_key(ORIGIN) {
            continue;
        }
        let member_path = if path.is_empty() {
            name.clone()
        } else {
            format!("{path}.{name}")
        };
        mark_origins(member, &member_path);
        member.insert(ORIGIN.into(), Value::String(member_path));
    }
}

/// Applies a group's own content over the content it extends: properties
/// replace per key (`$extensions` merges recursively), tokens replace whole,
/// and groups merge member by member. The result has the own group's origin.
fn merge_own(target: &mut Map<String, Value>, own: &Map<String, Value>) {
    target.shift_remove("$extends");
    target.shift_remove(ORIGIN);
    for (name, value) in own {
        if name == "$extends" {
            continue;
        }
        if name == "$extensions" {
            merge_extensions(target.entry(name.clone()).or_insert(Value::Null), value);
            continue;
        }
        let both_groups = !name.starts_with('$')
            && value.as_object().is_some_and(|value| !is_token(value))
            && target
                .get(name)
                .and_then(Value::as_object)
                .is_some_and(|existing| !is_token(existing));
        if both_groups {
            let existing = target
                .get_mut(name)
                .and_then(Value::as_object_mut)
                .expect("checked above");
            merge_own_group(existing, value.as_object().expect("checked above"));
        } else {
            target.insert(name.clone(), value.clone());
        }
    }
}

/// Merges a nested own group over the extended group at the same path,
/// keeping the own group's `$extends`, if any.
fn merge_own_group(target: &mut Map<String, Value>, own: &Map<String, Value>) {
    let own_extends = own.get("$extends").cloned();
    merge_own(target, own);
    if let Some(extends) = own_extends {
        target.insert("$extends".into(), extends);
    }
}

fn validate_group(
    resource: &ResourceId,
    path: &str,
    object: &Map<String, Value>,
) -> Result<(), DtcgError> {
    let token_error = |message: String| DtcgError::Token {
        resource: resource.clone(),
        path: path.into(),
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
    Ok(())
}

/// A token has a `$value` or a token-level `$ref` (not both), no `$extends`,
/// no `$root` and no child members; a token-level `$ref` is a local JSON
/// Pointer string.
fn validate_token(
    resource: &ResourceId,
    path: &str,
    object: &Map<String, Value>,
) -> Result<(), DtcgError> {
    let token_error = |message: String| DtcgError::Token {
        resource: resource.clone(),
        path: path.into(),
        message,
    };
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
            "token `{path}` must not have children"
        )));
    }
    if let Some(reference) = object.get("$ref") {
        let reference = reference
            .as_str()
            .ok_or_else(|| token_error("token-level `$ref` must be a string".into()))?;
        if !reference.starts_with("#/") {
            return Err(token_error(
                "token-level `$ref` must be a local JSON Pointer".into(),
            ));
        }
        pointer_parts(reference)?;
    }
    Ok(())
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
