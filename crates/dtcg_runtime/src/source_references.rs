use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::{
    DocumentCatalog, DocumentRequest, DtcgError, Resolution, ResourceId, RuntimeLimits,
    SourceOrigin, SourceReference,
    catalog::parse_document,
    json_pointer::{escape, fragment_parts, plain_pointer},
};

/// Walk authored source sites rather than effective sources. Relocation must
/// include dormant contexts and overridden definitions, while leaving arbitrary
/// JSON (particularly extension members and token aliases) untouched.
pub(crate) fn inspect(
    catalog: &DocumentCatalog,
    root: &ResourceId,
    limits: &RuntimeLimits,
) -> Result<Resolution<Vec<SourceReference>>, DtcgError> {
    let mut documents = BTreeMap::new();
    let mut missing = BTreeMap::new();
    let mut visited = BTreeSet::new();
    let mut references = Vec::new();
    let mut pending = vec![(
        root.clone(),
        String::new(),
        String::new(),
        Position::Resolver,
        0,
    )];
    while let Some((resource, pointer, scope, position, depth)) = pending.pop() {
        if !visited.insert((resource.clone(), pointer.clone(), scope.clone(), position)) {
            continue;
        }
        if depth > limits.max_reference_depth {
            return Err(DtcgError::LimitExceeded {
                kind: "reference depth",
                limit: limits.max_reference_depth,
            });
        }
        if !documents.contains_key(&resource) {
            let Some(json) = catalog.get(&resource) else {
                missing
                    .entry(resource.clone())
                    .or_insert_with(|| DocumentRequest {
                        resource_id: resource.clone(),
                        referenced_from: resource.clone(),
                        original_reference: resource.as_str().into(),
                    });
                continue;
            };
            documents.insert(resource.clone(), parse_document(&resource, json)?);
            if documents.len() > limits.max_documents {
                return Err(DtcgError::LimitExceeded {
                    kind: "documents",
                    limit: limits.max_documents,
                });
            }
        }
        let value = documents
            .get(&resource)
            .and_then(|document| document.pointer(&pointer))
            .ok_or_else(|| DtcgError::InvalidPointer(pointer.clone()))?;
        let mut children = Vec::new();
        match position {
            Position::Resolver => {
                if value.get("resolutionOrder").is_none() {
                    continue;
                }
                for member in ["sets", "modifiers"] {
                    if let Some(definitions) = value.get(member).and_then(Value::as_object) {
                        for name in definitions.keys() {
                            children.push((
                                resource.clone(),
                                format!("{pointer}/{member}/{}", escape(name)),
                                pointer.clone(),
                                Position::Definition,
                                depth,
                            ));
                        }
                    }
                }
                if let Some(order) = value.get("resolutionOrder").and_then(Value::as_array) {
                    for index in 0..order.len() {
                        children.push((
                            resource.clone(),
                            format!("{pointer}/resolutionOrder/{index}"),
                            pointer.clone(),
                            Position::Definition,
                            depth,
                        ));
                    }
                }
            }
            Position::Definition => {
                if let Some(sources) = value.get("sources").and_then(Value::as_array) {
                    for index in 0..sources.len() {
                        children.push((
                            resource.clone(),
                            format!("{pointer}/sources/{index}"),
                            scope.clone(),
                            Position::Source,
                            depth,
                        ));
                    }
                }
                if let Some(contexts) = value.get("contexts").and_then(Value::as_object) {
                    for (name, sources) in contexts {
                        if let Some(sources) = sources.as_array() {
                            for index in 0..sources.len() {
                                children.push((
                                    resource.clone(),
                                    format!("{pointer}/contexts/{}/{index}", escape(name)),
                                    scope.clone(),
                                    Position::Source,
                                    depth,
                                ));
                            }
                        }
                    }
                }
            }
            Position::Source => {
                let Some(reference) = value.get("$ref").and_then(Value::as_str) else {
                    children.push((resource, pointer, scope, Position::Resolver, depth));
                    pending.extend(children.into_iter().rev());
                    continue;
                };
                let target = resource.resolve(reference)?;
                references.push(SourceReference {
                    origin: SourceOrigin {
                        resource: Some(resource.clone()),
                        pointer: format!("{pointer}/$ref"),
                    },
                    reference: target.clone(),
                });
                // Reference-object overrides can themselves contain source arrays.
                children.push((
                    resource.clone(),
                    pointer.clone(),
                    scope.clone(),
                    Position::Definition,
                    depth,
                ));
                children.push((
                    resource.clone(),
                    pointer.clone(),
                    scope.clone(),
                    Position::Resolver,
                    depth,
                ));
                let fragment =
                    plain_pointer(&fragment_parts(target.fragment().unwrap_or_default())?);
                if reference.starts_with('#') {
                    let position = if fragment.starts_with("/sets/") {
                        Position::Definition
                    } else {
                        Position::Source
                    };
                    children.push((
                        resource,
                        format!("{scope}{fragment}"),
                        scope,
                        position,
                        depth + 1,
                    ));
                } else if catalog.contains(target.resource_id()) {
                    children.push((
                        target.resource_id().clone(),
                        fragment,
                        String::new(),
                        Position::Source,
                        depth + 1,
                    ));
                } else {
                    missing
                        .entry(target.resource_id().clone())
                        .or_insert_with(|| DocumentRequest {
                            resource_id: target.resource_id().clone(),
                            referenced_from: resource,
                            original_reference: reference.into(),
                        });
                }
            }
        }
        pending.extend(children.into_iter().rev());
    }
    if missing.is_empty() {
        Ok(Resolution::Ready(references))
    } else {
        Ok(Resolution::Missing(missing.into_values().collect()))
    }
}

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
enum Position {
    Resolver,
    Definition,
    Source,
}
