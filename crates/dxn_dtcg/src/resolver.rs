use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::{
    AffectedResolution, DtcgError, ResourceId, RuntimeOptions, SourceOccurrence,
    catalog::DocumentCatalog,
    flatten::{FlattenedSources, RawGroup, RawToken, flatten_sources},
    source_discovery::{discover, required_root},
    token_resolution::{Dependencies, DependencyCollector, Resolver},
};

/// A document the caller must load before resolution can continue.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct DocumentRequest {
    pub resource_id: ResourceId,
    pub referenced_from: ResourceId,
    pub original_reference: Arc<str>,
    /// What to provide: a JSON document
    /// ([`DtcgRuntime::provide_requested_json`](crate::DtcgRuntime::provide_requested_json))
    /// or a JavaScript or TypeScript module
    /// ([`DtcgRuntime::provide_requested_module`](crate::DtcgRuntime::provide_requested_module)).
    pub kind: DocumentKind,
}

/// What a catalog document holds.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum DocumentKind {
    /// A JSON document: a token document or a resolver.
    Json,
    /// The source text of a JavaScript or TypeScript ES module that token
    /// expressions import (feature `expressions`); its language follows its
    /// extension (`.js`/`.mjs` or `.ts`/`.mts`).
    Module,
}

impl DocumentKind {
    pub(crate) fn describe(self) -> &'static str {
        match self {
            Self::Json => "a JSON document",
            Self::Module => "a module",
        }
    }
}

/// The result of an operation that may need documents the catalog lacks.
#[derive(Clone, Debug, PartialEq)]
pub enum Resolution<T> {
    Ready(T),
    Missing(Vec<DocumentRequest>),
}

impl<T> Resolution<T> {
    /// The ready value, or `None` when documents are missing.
    pub fn ready(self) -> Option<T> {
        match self {
            Self::Ready(value) => Some(value),
            Self::Missing(_) => None,
        }
    }

    /// Converts a ready value and keeps missing-document requests unchanged.
    pub fn map<U>(self, function: impl FnOnce(T) -> U) -> Resolution<U> {
        match self {
            Self::Ready(value) => Resolution::Ready(function(value)),
            Self::Missing(requests) => Resolution::Missing(requests),
        }
    }

    /// Whether every document was available and the value is ready.
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready(_))
    }

    /// Whether the caller must load documents before the value is ready.
    pub fn is_missing(&self) -> bool {
        matches!(self, Self::Missing(_))
    }

    /// The documents to load, or `None` when the value is ready.
    pub fn missing(&self) -> Option<&[DocumentRequest]> {
        match self {
            Self::Ready(_) => None,
            Self::Missing(requests) => Some(requests),
        }
    }
}

/// Bounds on the work one call may perform. Construct it with
/// [`RuntimeLimits::default`] and the chainable `with_*` setters.
///
/// Two fixed bounds complement these limits: resolved values nest at most 128
/// levels (`LimitExceeded { kind: "value depth" }`), and flattened token paths
/// are at most 128 segments deep (`kind: "path depth"`), matching
/// `serde_json`'s parse recursion limit.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct RuntimeLimits {
    /// Bounds one discovery (default 256): the source occurrences it collects
    /// (loaded and inline sources and missing-document requests), and
    /// separately its expansion steps (each set reference, local reference,
    /// document load and nested resolver, even when it yields no source).
    pub max_documents: usize,
    /// Nesting depth of resolver and source references (default 64).
    pub max_reference_depth: usize,
    /// Reference hops in one resolution chain, counting token aliases,
    /// token-target pointers and document pointers (default 64). Literal
    /// values cost nothing, so a limit of 0 still resolves literal tokens.
    pub max_alias_depth: usize,
    /// JSON nodes one resolution may copy into resolved values through
    /// references, checked before each copy (default 4,000,000).
    pub max_value_nodes: usize,
    /// Flattened tokens of one resolution, counting those produced by
    /// `$extends`; it separately bounds the JSON nodes (tokens and groups)
    /// that `$extends` copies (default 100,000). Both report
    /// `LimitExceeded { kind: "tokens" }`.
    pub max_tokens: usize,
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            max_documents: 256,
            max_reference_depth: 64,
            max_alias_depth: 64,
            max_value_nodes: 4_000_000,
            max_tokens: 100_000,
        }
    }
}

impl RuntimeLimits {
    /// Sets [`RuntimeLimits::max_documents`].
    pub fn with_max_documents(mut self, limit: usize) -> Self {
        self.max_documents = limit;
        self
    }

    /// Sets [`RuntimeLimits::max_reference_depth`].
    pub fn with_max_reference_depth(mut self, limit: usize) -> Self {
        self.max_reference_depth = limit;
        self
    }

    /// Sets [`RuntimeLimits::max_alias_depth`].
    pub fn with_max_alias_depth(mut self, limit: usize) -> Self {
        self.max_alias_depth = limit;
        self
    }

    /// Sets [`RuntimeLimits::max_value_nodes`].
    pub fn with_max_value_nodes(mut self, limit: usize) -> Self {
        self.max_value_nodes = limit;
        self
    }

    /// Sets [`RuntimeLimits::max_tokens`].
    pub fn with_max_tokens(mut self, limit: usize) -> Self {
        self.max_tokens = limit;
        self
    }
}

/// Whether a token or group is deprecated (`$deprecated`, Format §5.2.4).
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub enum Deprecation {
    /// Not deprecated: no `$deprecated` applies, or it is `false`.
    #[default]
    NotDeprecated,
    /// `$deprecated: true`.
    Deprecated,
    /// `$deprecated` with an explanation, such as `"Use {color.accent}"`.
    DeprecatedWithReason(Arc<str>),
}

impl Deprecation {
    /// Reads a `$deprecated` value: `true`, `false` or a reason string.
    pub(crate) fn from_json(value: &Value) -> Self {
        match value {
            Value::String(reason) => Self::DeprecatedWithReason(reason.as_str().into()),
            Value::Bool(true) => Self::Deprecated,
            _ => Self::NotDeprecated,
        }
    }

    pub fn is_deprecated(&self) -> bool {
        !matches!(self, Self::NotDeprecated)
    }

    /// The explanation of a deprecation that has one.
    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::DeprecatedWithReason(reason) => Some(reason),
            _ => None,
        }
    }

    /// The `$deprecated` value that expresses this deprecation.
    pub fn to_json(&self) -> Value {
        match self {
            Self::NotDeprecated => Value::Bool(false),
            Self::Deprecated => Value::Bool(true),
            Self::DeprecatedWithReason(reason) => Value::String(reason.to_string()),
        }
    }
}

/// One resolved token. Fields may be added in minor releases.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ResolvedToken {
    pub path: Arc<str>,
    pub token_type: Option<Arc<str>>,
    pub authored_value: Value,
    /// The resolved value: the resolved `$value`, or the result of the
    /// token's expression when expressions are enabled.
    pub value: Value,
    /// For a token whose expression was evaluated, the resolved `$value` the
    /// result replaced; `None` for every other token. A value that differs
    /// from its snapshot means the authored `$value` is stale.
    pub snapshot: Option<Value>,
    /// The token's own authored `$extensions` object, or `Null` when it has
    /// none. Never inherited from groups.
    pub extensions: Value,
    /// The token's own `$description`. Never inherited from groups.
    pub description: Option<Arc<str>>,
    /// The token's own `$deprecated`, or else the nearest ancestor group's,
    /// where `false` ends an inherited deprecation.
    pub deprecated: Deprecation,
    /// The document of the winning definition. A token copied by `$extends`
    /// names the document of that `$extends`.
    pub source: ResourceId,
}

/// A group of the merged token structure that contains at least one resolved
/// token, with its properties after merge and `$extends`. Fields may be added
/// in minor releases.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ResolvedGroup {
    /// The dotted group path; empty for the merged root.
    pub path: Arc<str>,
    /// The `$type` its tokens inherit: its own, or else its nearest
    /// ancestor's.
    pub token_type: Option<Arc<str>>,
    pub description: Option<Arc<str>>,
    /// Its own `$deprecated`, when it declares one.
    pub deprecated: Option<Deprecation>,
    /// Its merged `$extensions` object, or `Null` when it has none.
    pub extensions: Value,
}

/// The tokens of one resolution. Equality and the fingerprint ignore the
/// member order of object values: `serde_json` objects compare as maps, and the
/// fingerprint hashes them with sorted keys.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResolvedTokens {
    tokens: BTreeMap<String, ResolvedToken>,
    order: Vec<String>,
    groups: BTreeMap<String, ResolvedGroup>,
    fingerprint: Arc<str>,
}

impl ResolvedTokens {
    pub fn get(&self, path: &str) -> Option<&ResolvedToken> {
        self.tokens.get(path)
    }
    /// The group at `path` (`""` for the merged root), when it contains at
    /// least one resolved token.
    pub fn group(&self, path: &str) -> Option<&ResolvedGroup> {
        self.groups.get(path)
    }
    /// Every group that contains a resolved token, in path order.
    pub fn groups(&self) -> impl Iterator<Item = &ResolvedGroup> {
        self.groups.values()
    }

    /// Writes this resolution as a DTCG 2025.10 token document (Resolver §8).
    ///
    /// Tokens are written in resolution order, each with an explicit `$type`,
    /// its resolved `$value` (no references), and its own `$description` and
    /// `$extensions`; `$deprecated` is written where it differs from what the
    /// token inherits from the written groups. `$root` tokens are written as
    /// `$root` members. Each group is created with its first token and keeps
    /// its own `$description`, `$deprecated` and `$extensions`; the root keeps
    /// its merged `$extensions`. No `$schema` is written.
    ///
    /// Resolving the document again yields the same paths, types, values and
    /// metadata. A nested document keeps a group's tokens together, so when
    /// resolution order interleaves the tokens of different groups (a later
    /// source redefined a token), the re-resolved order is grouped: the
    /// tokens come in the order their groups were first written.
    pub fn to_document(&self) -> Value {
        let mut root = Map::new();
        let mut root_deprecation = Deprecation::NotDeprecated;
        if let Some(group) = self.groups.get("") {
            write_group_metadata(&mut root, group);
            if let Some(deprecated) = &group.deprecated {
                root_deprecation = deprecated.clone();
            }
        }
        for token in self.iter() {
            let segments = token.path.split('.').collect::<Vec<_>>();
            let (name, parents) = segments.split_last().expect("token paths are not empty");
            let mut inherited = root_deprecation.clone();
            let mut current = &mut root;
            let mut path = String::new();
            for segment in parents {
                if !path.is_empty() {
                    path.push('.');
                }
                path.push_str(segment);
                let group = self.groups.get(&path);
                if let Some(deprecated) = group.and_then(|group| group.deprecated.as_ref()) {
                    inherited = deprecated.clone();
                }
                current = current
                    .entry((*segment).to_owned())
                    .or_insert_with(|| {
                        let mut members = Map::new();
                        if let Some(group) = group {
                            write_group_metadata(&mut members, group);
                        }
                        Value::Object(members)
                    })
                    .as_object_mut()
                    .expect("groups are objects");
            }
            let mut written = Map::new();
            if let Some(token_type) = &token.token_type {
                written.insert("$type".into(), Value::String(token_type.to_string()));
            }
            written.insert("$value".into(), token.value.clone());
            if let Some(description) = &token.description {
                written.insert(
                    "$description".into(),
                    Value::String(description.to_string()),
                );
            }
            if token.deprecated != inherited {
                written.insert("$deprecated".into(), token.deprecated.to_json());
            }
            if !token.extensions.is_null() {
                written.insert("$extensions".into(), token.extensions.clone());
            }
            current.insert((*name).to_owned(), Value::Object(written));
        }
        Value::Object(root)
    }
    /// Tokens in resolution order.
    pub fn iter(&self) -> impl Iterator<Item = &ResolvedToken> {
        self.order.iter().filter_map(|path| self.tokens.get(path))
    }
    /// A SHA-256 digest of every token's path, type, canonical value and
    /// source, in resolution order. Metadata does not contribute.
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
    pub fn len(&self) -> usize {
        self.order.len()
    }
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
    /// Applies an affected re-resolution of the same selection: removes the
    /// removed paths, replaces and adds the affected tokens, adopts the new
    /// order and groups, and refreshes the fingerprint. The result equals a
    /// full resolution of the edited documents.
    pub fn apply_affected(&mut self, affected: &AffectedResolution) {
        for path in affected.removed() {
            self.tokens.remove(path);
        }
        for token in affected.iter() {
            self.tokens.insert(token.path.to_string(), token.clone());
        }
        self.order = affected.order.clone();
        self.groups = affected.groups.clone();
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
        hash_canonical(&mut hasher, &token.value);
        hasher.update([0]);
        hasher.update(token.source.as_str().as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize()).into()
}

/// Hashes `value` as JSON with object keys sorted recursively, so equal values
/// hash equally whatever their authored member order.
fn hash_canonical(hasher: &mut Sha256, value: &Value) {
    match value {
        Value::Array(values) => {
            hasher.update(b"[");
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    hasher.update(b",");
                }
                hash_canonical(hasher, value);
            }
            hasher.update(b"]");
        }
        Value::Object(object) => {
            let mut members = object.iter().collect::<Vec<_>>();
            members.sort_unstable_by_key(|(key, _)| *key);
            hasher.update(b"{");
            for (index, (key, value)) in members.into_iter().enumerate() {
                if index > 0 {
                    hasher.update(b",");
                }
                hasher.update(Value::String(key.clone()).to_string().as_bytes());
                hasher.update(b":");
                hash_canonical(hasher, value);
            }
            hasher.update(b"}");
        }
        scalar => hasher.update(scalar.to_string().as_bytes()),
    }
}

pub(crate) fn resolve(
    catalog: &DocumentCatalog,
    root: Option<&ResourceId>,
    options: &RuntimeOptions,
    selection: &[(&str, &str)],
) -> Result<Resolution<ResolvedTokens>, DtcgError> {
    let sources = discover(
        catalog,
        required_root(root)?,
        &options.limits,
        selection,
        false,
    )?;
    let Resolution::Ready(sources) = sources else {
        return Ok(sources.map(|_| unreachable!()));
    };
    resolve_sources(catalog, &sources, options)
}

pub(crate) fn resolve_sources(
    catalog: &DocumentCatalog,
    sources: &[SourceOccurrence],
    options: &RuntimeOptions,
) -> Result<Resolution<ResolvedTokens>, DtcgError> {
    let FlattenedSources {
        raw, order, groups, ..
    } = flatten_sources(sources, &options.limits)?;
    #[cfg(feature = "expressions")]
    let modules = match crate::expressions::document_modules(catalog, sources, &raw, options)? {
        Resolution::Ready(modules) => modules,
        Resolution::Missing(requests) => return Ok(Resolution::Missing(requests)),
    };
    #[cfg(not(feature = "expressions"))]
    let _ = catalog;
    let mut resolver = Resolver::new(&raw, options);
    #[cfg(feature = "expressions")]
    resolver.with_document_modules(&modules);
    for path in &order {
        resolver.resolve_token(path)?;
    }
    let resolved = resolver.into_resolved();
    let fingerprint = fingerprint(&order, &resolved);
    Ok(Resolution::Ready(ResolvedTokens {
        tokens: resolved,
        order,
        groups: resolved_groups(groups),
        fingerprint,
    }))
}

fn write_group_metadata(members: &mut Map<String, Value>, group: &ResolvedGroup) {
    if let Some(description) = &group.description {
        members.insert(
            "$description".into(),
            Value::String(description.to_string()),
        );
    }
    if let Some(deprecated) = &group.deprecated {
        members.insert("$deprecated".into(), deprecated.to_json());
    }
    if !group.extensions.is_null() {
        members.insert("$extensions".into(), group.extensions.clone());
    }
}

fn resolved_groups(groups: BTreeMap<String, RawGroup>) -> BTreeMap<String, ResolvedGroup> {
    groups
        .into_iter()
        .map(|(path, group)| {
            let resolved = ResolvedGroup {
                path: path.as_str().into(),
                token_type: group.token_type,
                description: group.description,
                deprecated: group.deprecated,
                extensions: group.extensions,
            };
            (path, resolved)
        })
        .collect()
}

/// The flattened tokens of a selection, for comparing an edit against the
/// state before it.
pub(crate) fn flatten_selection(
    catalog: &DocumentCatalog,
    root: Option<&ResourceId>,
    options: &RuntimeOptions,
    selection: &[(&str, &str)],
) -> Result<Resolution<BTreeMap<String, RawToken>>, DtcgError> {
    let sources = discover(
        catalog,
        required_root(root)?,
        &options.limits,
        selection,
        false,
    )?;
    let Resolution::Ready(sources) = sources else {
        return Ok(sources.map(|_| unreachable!()));
    };
    flatten_sources(&sources, &options.limits).map(|flattened| Resolution::Ready(flattened.raw))
}

pub(crate) fn resolve_affected(
    catalog: &DocumentCatalog,
    root: Option<&ResourceId>,
    options: &RuntimeOptions,
    selection: &[(&str, &str)],
    changed_paths: &[&str],
    previous: Option<&BTreeMap<String, RawToken>>,
) -> Result<Resolution<AffectedResolution>, DtcgError> {
    let limits = &options.limits;
    let sources = discover(catalog, required_root(root)?, limits, selection, false)?;
    let Resolution::Ready(sources) = sources else {
        return Ok(sources.map(|_| unreachable!()));
    };
    let FlattenedSources {
        raw,
        order,
        groups,
        has_extends,
    } = flatten_sources(&sources, limits)?;
    #[cfg(feature = "expressions")]
    let modules = match crate::expressions::document_modules(catalog, &sources, &raw, options)? {
        Resolution::Ready(modules) => modules,
        Resolution::Missing(requests) => return Ok(Resolution::Missing(requests)),
    };

    // Seeds: named tokens and every token under a named group, tokens whose
    // flattened definition changed or is new, and every token when the
    // previous state is unknown or any group uses `$extends`.
    let mut affected = BTreeSet::new();
    for changed in changed_paths {
        let prefix = format!("{changed}.");
        affected.extend(
            raw.keys()
                .filter(|path| changed.is_empty() || *path == changed || path.starts_with(&prefix))
                .cloned(),
        );
    }
    let removed = previous.map_or_else(Vec::new, |previous| {
        previous
            .keys()
            .filter(|path| !raw.contains_key(*path))
            .cloned()
            .collect::<Vec<_>>()
    });
    match previous {
        Some(previous) if !has_extends => affected.extend(
            raw.iter()
                .filter(|(path, token)| {
                    previous
                        .get(*path)
                        .is_none_or(|before| !same_definition(before, token))
                })
                .map(|(path, _)| path.clone()),
        ),
        _ => affected.extend(raw.keys().cloned()),
    }
    let mut reverse_dependencies = BTreeMap::<String, BTreeSet<String>>::new();
    let mut collector = DependencyCollector::new(&raw, limits);
    for (path, token) in &raw {
        let mut dependencies = Dependencies::default();
        collector.collect(&token.value, token.document.as_ref(), &mut dependencies)?;
        // An expression also reads its inputs. A malformed expression object
        // is reported when the token resolves.
        #[cfg(feature = "expressions")]
        if options.expressions.is_some()
            && let Some(inputs) = token
                .extensions
                .get(crate::EXPRESSION_EXTENSION)
                .and_then(|expression| expression.get("inputs"))
                .and_then(Value::as_object)
        {
            for input in inputs.values() {
                collector.collect(input, token.document.as_ref(), &mut dependencies)?;
            }
        }
        // A document pointer may read any edited location.
        if dependencies.reads_document {
            affected.insert(path.clone());
        }
        for dependency in dependencies.tokens {
            reverse_dependencies
                .entry(dependency)
                .or_default()
                .insert(path.clone());
        }
    }
    let mut pending = affected.iter().chain(&removed).cloned().collect::<Vec<_>>();
    while let Some(path) = pending.pop() {
        if let Some(dependents) = reverse_dependencies.get(&path) {
            for dependent in dependents {
                if affected.insert(dependent.clone()) {
                    pending.push(dependent.clone());
                }
            }
        }
    }

    let mut resolver = Resolver::new(&raw, options);
    #[cfg(feature = "expressions")]
    resolver.with_document_modules(&modules);
    for path in order.iter().filter(|path| affected.contains(*path)) {
        resolver.resolve_token(path)?;
    }
    let resolved = resolver.into_resolved();
    #[cfg(feature = "diagnostics")]
    let nodes_visited = resolved.len();
    let tokens = order
        .iter()
        .filter(|path| affected.contains(*path))
        .filter_map(|path| resolved.get(path).cloned())
        .collect();
    Ok(Resolution::Ready(AffectedResolution {
        tokens,
        removed,
        order,
        groups: resolved_groups(groups),
        #[cfg(feature = "diagnostics")]
        nodes_visited,
    }))
}

/// Whether two flattened definitions of a path resolve alike, given equal
/// dependencies. Document contents are compared separately: every token that
/// reads a document pointer is re-resolved.
fn same_definition(before: &RawToken, after: &RawToken) -> bool {
    before.own_type == after.own_type
        && before.group_type == after.group_type
        && before.authored_value == after.authored_value
        && before.value == after.value
        && before.extensions == after.extensions
        && before.description == after.description
        && before.deprecated == after.deprecated
        && before.source == after.source
}

pub(crate) fn validate_complete(
    catalog: &DocumentCatalog,
    root: Option<&ResourceId>,
    options: &RuntimeOptions,
) -> Result<Resolution<()>, DtcgError> {
    match discover(catalog, required_root(root)?, &options.limits, &[], true)? {
        // With expressions, every module a source of any context declares,
        // and what it imports, must be loaded too.
        #[cfg(feature = "expressions")]
        Resolution::Ready(sources) => {
            crate::expressions::validate_modules(catalog, &sources, options)
        }
        #[cfg(not(feature = "expressions"))]
        Resolution::Ready(_) => Ok(Resolution::Ready(())),
        Resolution::Missing(requests) => Ok(Resolution::Missing(requests)),
    }
}
