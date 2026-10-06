use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    AffectedResolution, DtcgError, ResourceId, SourceOccurrence,
    catalog::DocumentCatalog,
    flatten::{FlattenedSources, flatten_sources},
    source_discovery::{discover, required_root},
    token_resolution::{DependencyCollector, Resolver},
};

/// A document the caller must load before resolution can continue.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct DocumentRequest {
    pub resource_id: ResourceId,
    pub referenced_from: ResourceId,
    pub original_reference: Arc<str>,
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
    /// `$extends`; it separately bounds the `$extends` links applied
    /// (default 100,000).
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

/// One resolved token. Fields may be added in minor releases.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ResolvedToken {
    pub path: Arc<str>,
    pub token_type: Option<Arc<str>>,
    pub authored_value: Value,
    pub value: Value,
    /// The token's authored `$extensions` object, or `Null` when it has none.
    pub extensions: Value,
    pub source: ResourceId,
}

/// The tokens of one resolution. Equality and the fingerprint ignore the
/// member order of object values: `serde_json` objects compare as maps, and the
/// fingerprint hashes them with sorted keys.
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
    let FlattenedSources { raw, order } = flatten_sources(sources, limits)?;
    let mut resolver = Resolver::new(&raw, limits);
    for path in &order {
        resolver.resolve_token(path)?;
    }
    let resolved = resolver.into_resolved();
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
    let FlattenedSources { raw, order } = flatten_sources(&sources, limits)?;

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
    let mut collector = DependencyCollector::new(&raw, limits);
    for (path, token) in &raw {
        let mut dependencies = BTreeSet::new();
        collector.collect(&token.value, token.document.as_ref(), &mut dependencies)?;
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

    let mut resolver = Resolver::new(&raw, limits);
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
