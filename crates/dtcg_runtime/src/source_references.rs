use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

use serde_json::Value;

use crate::{
    DocumentCatalog, DocumentRequest, DtcgError, Resolution, ResourceId, RuntimeLimits,
    SourceOccurrence, SourceOrigin, SourceReference,
    catalog::parse_document,
    json_pointer::{fragment_parts, walk},
    resolver_definition::{is_resolver, validate_resolver},
};

/// Walk authored source sites rather than effective sources. Relocation must
/// include dormant contexts and overridden definitions, while leaving arbitrary
/// JSON (particularly extension members and token aliases) untouched.
///
/// A reference object is walked as its target with the object's sibling
/// members applied, exactly as discovery overlays it: the override members'
/// sets, modifiers and sources are visited, and a definition an override
/// replaced is not, so no document reachable only through it is requested.
///
/// Each authored site is listed once. The first walk of every authored
/// location is free, so the work is linear in the documents' size. A location
/// reached again through another reference object or resolver is walked again,
/// since overrides may change what it reaches. Such a repeated walk counts in
/// discovery's units toward `max_documents`, with the same two separate
/// counts: steps (each resolver it expands and reference it follows) and
/// sources (each inline token source), so a document discovery accepts is
/// never rejected here except through definitions discovery does not expand. A
/// reference target that holds no source site, such as a token document, is
/// never copied.
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
        walked: BTreeSet::new(),
        repeated_steps: 0,
        repeated_sources: 0,
        sites: BTreeSet::new(),
        references: Vec::new(),
    };
    if walker.load(root, root, root.as_str())? {
        let (pointer, value) = walker.locate(root, None)?;
        let document = SourceOccurrence {
            resource: root.clone(),
            value: value.clone(),
            origin: SourceOrigin {
                resource: Some(root.clone()),
                pointer,
            },
            member_origins: BTreeMap::new(),
        };
        let mut pending = vec![Item {
            scope: Rc::new(Scope::new(document.clone())),
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

/// The resolver occurrence local references resolve against, shared by every
/// item walked inside it.
struct Scope {
    occurrence: SourceOccurrence,
    key: String,
}

impl Scope {
    fn new(occurrence: SourceOccurrence) -> Self {
        let key = describe(&occurrence.origin, &occurrence.member_origins);
        Self { occurrence, key }
    }
}

/// One authored location to walk: an occurrence (possibly overlaid by a
/// reference object), the resolver its local references resolve against, and
/// what kind of member it is.
struct Item {
    scope: Rc<Scope>,
    occurrence: SourceOccurrence,
    position: Position,
    depth: usize,
}

struct Walker<'a> {
    catalog: &'a DocumentCatalog,
    limits: &'a RuntimeLimits,
    documents: BTreeMap<ResourceId, Value>,
    missing: BTreeMap<ResourceId, DocumentRequest>,
    /// Walk items already expanded, by [`identity`].
    visited: BTreeSet<String>,
    /// Authored locations walked at least once, with their position.
    walked: BTreeSet<String>,
    /// Resolver expansions and followed references of walks of an already
    /// walked location, counted against `max_documents`.
    repeated_steps: usize,
    /// Inline token sources of walks of an already walked location, counted
    /// separately against `max_documents`.
    repeated_sources: usize,
    /// Where each listed `$ref` is written, so a site reached through several
    /// reference objects is listed once.
    sites: BTreeSet<(Option<ResourceId>, String)>,
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
        let occurrence = &item.occurrence;
        if !self.visited.insert(identity(
            &occurrence.origin,
            &occurrence.member_origins,
            &item.scope.key,
            item.position,
        )) {
            return Ok(Vec::new());
        }
        let repeated = !self
            .walked
            .insert(format!("{:?}|{:?}", occurrence.origin, item.position));
        if repeated {
            // What discovery counts: steps (expanding a resolver, following a
            // reference) and, separately, collected token sources.
            let count = match item.position {
                Position::Resolver if is_resolver(&occurrence.value) => {
                    Some(&mut self.repeated_steps)
                }
                Position::Source if occurrence.value.get("$ref").is_some() => {
                    Some(&mut self.repeated_steps)
                }
                Position::Source if !is_resolver(&occurrence.value) => {
                    Some(&mut self.repeated_sources)
                }
                _ => None,
            };
            if let Some(count) = count {
                *count += 1;
                if *count > self.limits.max_documents {
                    return Err(DtcgError::LimitExceeded {
                        kind: "documents",
                        limit: self.limits.max_documents,
                    });
                }
            }
        }
        self.expand(item)
    }

    fn expand(&mut self, item: Item) -> Result<Vec<Item>, DtcgError> {
        let Item {
            scope,
            occurrence,
            position,
            depth,
        } = item;
        let mut children = Vec::new();
        let mut push = |scope: &Rc<Scope>, occurrence: SourceOccurrence, position, depth| {
            children.push(Item {
                scope: Rc::clone(scope),
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
                // The same structure rules as discovery, for every reached resolver.
                validate_resolver(&occurrence.resource, &occurrence.value)?;
                let inner = Rc::new(Scope::new(occurrence));
                let occurrence = &inner.occurrence;
                for member in ["sets", "modifiers"] {
                    if let Some(definitions) =
                        occurrence.value.get(member).and_then(Value::as_object)
                    {
                        let definitions_source = occurrence.child(member)?;
                        for name in definitions.keys() {
                            push(
                                &inner,
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
                            &inner,
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
                    if is_resolver(&occurrence.value) {
                        push(&scope, occurrence, Position::Resolver, depth);
                    }
                    return Ok(children);
                };
                let owner = occurrence.member_origin("$ref").clone();
                let from = owner
                    .resource
                    .clone()
                    .unwrap_or_else(|| scope.occurrence.resource.clone());
                let target = from.resolve(reference)?;
                let origin = owner.child("$ref");
                if self
                    .sites
                    .insert((origin.resource.clone(), origin.pointer.clone()))
                {
                    self.references.push(SourceReference {
                        origin,
                        reference: target.clone(),
                    });
                }
                // The members the reference object applies to its target.
                let overrides: BTreeMap<String, SourceOrigin> = occurrence
                    .value
                    .as_object()
                    .into_iter()
                    .flat_map(|object| object.keys())
                    .filter(|key| *key != "$ref")
                    .map(|key| (key.clone(), occurrence.member_origin(key).clone()))
                    .collect();
                if reference.starts_with('#') {
                    let fragment = target.fragment().unwrap_or_default();
                    let parts = fragment_parts(fragment)?;
                    let position = if parts.first().is_some_and(|part| part == "sets") {
                        Position::Definition
                    } else {
                        Position::Source
                    };
                    if position == Position::Source
                        && walk(&scope.occurrence.value, &parts)
                            .is_some_and(|value| leads_nowhere(value, &overrides))
                    {
                        return Ok(children);
                    }
                    // A target already walked the same way is not copied again.
                    let selected_origin = scope.occurrence.selected_origin(&parts);
                    if self.visited.contains(&identity(
                        &selected_origin,
                        &overrides,
                        &scope.key,
                        position,
                    )) {
                        return Ok(children);
                    }
                    let selected = scope.occurrence.select(&format!("#{fragment}"))?;
                    push(&scope, selected.overlay(&occurrence)?, position, depth + 1);
                } else if self.load(target.resource_id(), &from, reference)? {
                    let (pointer, value) = self.locate(target.resource_id(), target.fragment())?;
                    if leads_nowhere(value, &overrides) {
                        return Ok(children);
                    }
                    let target_origin = SourceOrigin {
                        resource: Some(target.resource_id().clone()),
                        pointer,
                    };
                    // A target that is a resolver is its own scope.
                    let resolver = is_resolver(value) || overrides.contains_key("resolutionOrder");
                    let scope_key = if resolver {
                        describe(&target_origin, &overrides)
                    } else {
                        scope.key.clone()
                    };
                    if self.visited.contains(&identity(
                        &target_origin,
                        &overrides,
                        &scope_key,
                        Position::Source,
                    )) {
                        return Ok(children);
                    }
                    let loaded = SourceOccurrence {
                        resource: target.resource_id().clone(),
                        value: value.clone(),
                        origin: target_origin,
                        member_origins: BTreeMap::new(),
                    };
                    let overlaid = loaded.overlay(&occurrence)?;
                    let next_scope = if resolver {
                        Rc::new(Scope::new(overlaid.clone()))
                    } else {
                        Rc::clone(&scope)
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

    /// Parses `resource` once, or records it as missing. Returns whether it is
    /// available.
    fn load(
        &mut self,
        resource: &ResourceId,
        from: &ResourceId,
        reference: &str,
    ) -> Result<bool, DtcgError> {
        if self.documents.contains_key(resource) {
            return Ok(true);
        }
        let Some(json) = self.catalog.get(resource) else {
            self.missing
                .entry(resource.clone())
                .or_insert_with(|| DocumentRequest {
                    resource_id: resource.clone(),
                    referenced_from: from.clone(),
                    original_reference: reference.into(),
                });
            return Ok(false);
        };
        self.documents
            .insert(resource.clone(), parse_document(resource, json)?);
        if self.documents.len() > self.limits.max_documents {
            return Err(DtcgError::LimitExceeded {
                kind: "documents",
                limit: self.limits.max_documents,
            });
        }
        Ok(true)
    }

    /// The pointer and value a fragment of a loaded document selects.
    fn locate(
        &self,
        resource: &ResourceId,
        fragment: Option<&str>,
    ) -> Result<(String, &Value), DtcgError> {
        let document = &self.documents[resource];
        match fragment {
            Some(fragment) => {
                let parts = fragment_parts(fragment)?;
                let value = walk(document, &parts)
                    .ok_or_else(|| DtcgError::InvalidPointer(fragment.into()))?;
                Ok((crate::json_pointer::plain_pointer(&parts), value))
            }
            None => Ok((String::new(), document)),
        }
    }
}

/// Whether a reference target, with the reference object's overrides applied,
/// holds no source site: an object that is neither a reference nor a resolver,
/// such as a token document. Such a target is not copied or walked, so many
/// references to one large token document cost nothing.
fn leads_nowhere(target: &Value, overrides: &BTreeMap<String, SourceOrigin>) -> bool {
    target.is_object()
        && target.get("$ref").is_none()
        && !is_resolver(target)
        && !overrides.contains_key("resolutionOrder")
}

fn describe(origin: &SourceOrigin, member_origins: &BTreeMap<String, SourceOrigin>) -> String {
    format!("{origin:?}{member_origins:?}")
}

/// What makes two walk items the same: the occurrence's location and override
/// owners, its scope's location and override owners, and the position.
fn identity(
    origin: &SourceOrigin,
    member_origins: &BTreeMap<String, SourceOrigin>,
    scope: &str,
    position: Position,
) -> String {
    format!("{}|{scope}|{position:?}", describe(origin, member_origins))
}
