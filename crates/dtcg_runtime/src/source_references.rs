use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::{
    DocumentCatalog, DocumentRequest, DtcgError, Resolution, ResourceId, RuntimeLimits,
    SourceOccurrence, SourceOrigin, SourceReference,
    catalog::parse_document,
    json_pointer::{fragment_parts, walk},
    resolver_definition::is_resolver,
};

/// Walk authored source sites rather than effective sources. Relocation must
/// include dormant contexts and overridden definitions, while leaving arbitrary
/// JSON (particularly extension members and token aliases) untouched.
///
/// A reference object is walked as its target with the object's sibling
/// members applied, exactly as discovery overlays it: the override members'
/// sets, modifiers and sources are visited, and a definition an override
/// replaced is not, so no document reachable only through it is requested.
pub(crate) fn inspect(
    catalog: &DocumentCatalog,
    root: &ResourceId,
    limits: &RuntimeLimits,
) -> Result<Resolution<Vec<SourceReference>>, DtcgError> {
    let mut walker = Walker {
        catalog,
        limits,
        documents: BTreeMap::new(),
        missing: BTreeMap::new(),
        visited: BTreeSet::new(),
        references: Vec::new(),
    };
    if let Some(document) = walker.load(root, None, root, root.as_str())? {
        let mut pending = vec![Item {
            scope: document.clone(),
            occurrence: document,
            position: Position::Resolver,
            depth: 0,
        }];
        while let Some(item) = pending.pop() {
            let children = walker.visit(item)?;
            pending.extend(children.into_iter().rev());
        }
    }
    if walker.missing.is_empty() {
        Ok(Resolution::Ready(walker.references))
    } else {
        Ok(Resolution::Missing(walker.missing.into_values().collect()))
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Position {
    Resolver,
    Definition,
    Source,
}

/// One authored location to walk: an occurrence (possibly overlaid by a
/// reference object), the resolver occurrence its local references resolve
/// against, and what kind of member it is.
struct Item {
    scope: SourceOccurrence,
    occurrence: SourceOccurrence,
    position: Position,
    depth: usize,
}

struct Walker<'a> {
    catalog: &'a DocumentCatalog,
    limits: &'a RuntimeLimits,
    documents: BTreeMap<ResourceId, Value>,
    missing: BTreeMap<ResourceId, DocumentRequest>,
    visited: BTreeSet<String>,
    references: Vec<SourceReference>,
}

impl Walker<'_> {
    fn visit(&mut self, item: Item) -> Result<Vec<Item>, DtcgError> {
        if item.depth > self.limits.max_reference_depth {
            return Err(DtcgError::LimitExceeded {
                kind: "reference depth",
                limit: self.limits.max_reference_depth,
            });
        }
        if !self.visited.insert(identity(&item)) {
            return Ok(Vec::new());
        }
        let Item {
            scope,
            occurrence,
            position,
            depth,
        } = item;
        let mut children = Vec::new();
        let mut push = |scope: &SourceOccurrence, occurrence: SourceOccurrence, position, depth| {
            children.push(Item {
                scope: scope.clone(),
                occurrence,
                position,
                depth,
            });
        };
        match position {
            Position::Resolver => {
                if !is_resolver(&occurrence.value) {
                    return Ok(children);
                }
                for member in ["sets", "modifiers"] {
                    if let Some(definitions) =
                        occurrence.value.get(member).and_then(Value::as_object)
                    {
                        let definitions_source = occurrence.child(member)?;
                        for name in definitions.keys() {
                            push(
                                &occurrence,
                                definitions_source.child(name)?,
                                Position::Definition,
                                depth,
                            );
                        }
                    }
                }
                if let Some(order) = occurrence
                    .value
                    .get("resolutionOrder")
                    .and_then(Value::as_array)
                {
                    let order_source = occurrence.child("resolutionOrder")?;
                    for index in 0..order.len() {
                        push(
                            &occurrence,
                            order_source.child(&index.to_string())?,
                            Position::Definition,
                            depth,
                        );
                    }
                }
            }
            Position::Definition => {
                if let Some(sources) = occurrence.value.get("sources").and_then(Value::as_array) {
                    let sources_source = occurrence.child("sources")?;
                    for index in 0..sources.len() {
                        push(
                            &scope,
                            sources_source.child(&index.to_string())?,
                            Position::Source,
                            depth,
                        );
                    }
                }
                if let Some(contexts) = occurrence.value.get("contexts").and_then(Value::as_object)
                {
                    let contexts_source = occurrence.child("contexts")?;
                    for (name, sources) in contexts {
                        if let Some(sources) = sources.as_array() {
                            let context = contexts_source.child(name)?;
                            for index in 0..sources.len() {
                                push(
                                    &scope,
                                    context.child(&index.to_string())?,
                                    Position::Source,
                                    depth,
                                );
                            }
                        }
                    }
                }
            }
            Position::Source => {
                let Some(reference) = occurrence.value.get("$ref").and_then(Value::as_str) else {
                    // An inline source may itself be a nested resolver.
                    push(&scope, occurrence, Position::Resolver, depth);
                    return Ok(children);
                };
                let owner = occurrence.member_origin("$ref").clone();
                let from = owner
                    .resource
                    .clone()
                    .unwrap_or_else(|| scope.resource.clone());
                let target = from.resolve(reference)?;
                self.references.push(SourceReference {
                    origin: owner.child("$ref"),
                    reference: target.clone(),
                });
                if reference.starts_with('#') {
                    let fragment = target.fragment().unwrap_or_default();
                    let selected = scope.select(&format!("#{fragment}"))?;
                    let is_set = fragment_parts(fragment)?
                        .first()
                        .is_some_and(|part| part == "sets");
                    let overlaid = selected.overlay(&occurrence)?;
                    let position = if is_set {
                        Position::Definition
                    } else {
                        Position::Source
                    };
                    push(&scope, overlaid, position, depth + 1);
                } else if let Some(loaded) =
                    self.load(target.resource_id(), target.fragment(), &from, reference)?
                {
                    let overlaid = loaded.overlay(&occurrence)?;
                    // A target that is a resolver is its own scope.
                    let next_scope = if is_resolver(&overlaid.value) {
                        overlaid.clone()
                    } else {
                        scope.clone()
                    };
                    push(&next_scope, overlaid, Position::Source, depth + 1);
                } else {
                    // The reference object's own override members still name
                    // sources.
                    push(&scope, occurrence, Position::Definition, depth);
                }
            }
        }
        Ok(children)
    }

    fn load(
        &mut self,
        resource: &ResourceId,
        fragment: Option<&str>,
        from: &ResourceId,
        reference: &str,
    ) -> Result<Option<SourceOccurrence>, DtcgError> {
        if !self.documents.contains_key(resource) {
            let Some(json) = self.catalog.get(resource) else {
                self.missing
                    .entry(resource.clone())
                    .or_insert_with(|| DocumentRequest {
                        resource_id: resource.clone(),
                        referenced_from: from.clone(),
                        original_reference: reference.into(),
                    });
                return Ok(None);
            };
            self.documents
                .insert(resource.clone(), parse_document(resource, json)?);
            if self.documents.len() > self.limits.max_documents {
                return Err(DtcgError::LimitExceeded {
                    kind: "documents",
                    limit: self.limits.max_documents,
                });
            }
        }
        let document = &self.documents[resource];
        let (value, pointer) = match fragment {
            Some(fragment) => {
                let parts = fragment_parts(fragment)?;
                let value = walk(document, &parts)
                    .cloned()
                    .ok_or_else(|| DtcgError::InvalidPointer(fragment.into()))?;
                (value, crate::json_pointer::plain_pointer(&parts))
            }
            None => (document.clone(), String::new()),
        };
        Ok(Some(SourceOccurrence {
            resource: resource.clone(),
            value,
            origin: SourceOrigin {
                resource: Some(resource.clone()),
                pointer,
            },
            member_origins: BTreeMap::new(),
        }))
    }
}

/// What makes two walk items the same: the occurrence's location and override
/// owners, its scope's location and override owners, and the position.
fn identity(item: &Item) -> String {
    let describe = |occurrence: &SourceOccurrence| {
        format!(
            "{:?}{:?}",
            occurrence.origin,
            occurrence.member_origins.iter().collect::<Vec<_>>()
        )
    };
    format!(
        "{}|{}|{:?}",
        describe(&item.occurrence),
        describe(&item.scope),
        item.position
    )
}
