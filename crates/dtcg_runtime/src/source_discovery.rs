use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::{
    DocumentCatalog, DocumentRequest, DtcgError, Resolution, ResourceId, RuntimeLimits,
    SourceOccurrence, SourceOrigin,
    catalog::parse_document,
    error::invalid,
    json_pointer::pointer_parts,
    resolver_definition::{
        modifier_documents, optional_object, resolution_step, resolve_local_reference,
        set_documents, validate_inputs,
    },
};

pub(crate) fn required_root(root: Option<&ResourceId>) -> Result<&ResourceId, DtcgError> {
    root.ok_or_else(|| {
        invalid(
            &ResourceId::parse("catalog").expect("static logical id"),
            "root document is not set",
        )
    })
}

pub(crate) fn discover(
    catalog: &DocumentCatalog,
    root: &ResourceId,
    limits: &RuntimeLimits,
    selection: &[(&str, &str)],
    include_inactive: bool,
) -> Result<Resolution<Vec<SourceOccurrence>>, DtcgError> {
    let mut discovery = Discovery::new(catalog, root, limits, selection, include_inactive)?;
    let root = discovery.load(root, None, root, root.as_str())?;
    discovery.expand(root.into_iter().map(|source| (source, 0)).collect())
}

/// Discover a supplied source in the root resolver's logical reference scope.
pub(crate) fn discover_source(
    catalog: &DocumentCatalog,
    root: &ResourceId,
    limits: &RuntimeLimits,
    source: &Value,
    selection: &[(&str, &str)],
    include_inactive: bool,
) -> Result<Resolution<Vec<SourceOccurrence>>, DtcgError> {
    let mut discovery = Discovery::new(catalog, root, limits, selection, include_inactive)?;
    let Some(scope) = discovery.load(root, None, root, root.as_str())? else {
        return discovery.expand(Vec::new());
    };
    if is_resolver(&scope.value) {
        validate_resolver(&scope, &discovery.selections, include_inactive)?;
    }
    let supplied = SourceOccurrence {
        resource: root.clone(),
        value: source.clone(),
        origin: SourceOrigin {
            resource: None,
            pointer: String::new(),
        },
        member_origins: BTreeMap::new(),
    };
    let mut sources = Vec::new();
    discovery.append_source(&scope, supplied, &mut sources, &mut Vec::new(), 0)?;
    discovery.expand(sources)
}

struct Discovery<'a> {
    catalog: &'a DocumentCatalog,
    limits: &'a RuntimeLimits,
    selections: BTreeMap<&'a str, &'a str>,
    include_inactive: bool,
    missing: BTreeMap<ResourceId, DocumentRequest>,
}

impl<'a> Discovery<'a> {
    fn new(
        catalog: &'a DocumentCatalog,
        root: &ResourceId,
        limits: &'a RuntimeLimits,
        selection: &'a [(&'a str, &'a str)],
        include_inactive: bool,
    ) -> Result<Self, DtcgError> {
        let mut selections = BTreeMap::new();
        for (name, value) in selection {
            if selections.insert(*name, *value).is_some() {
                return Err(invalid(root, &format!("duplicate input `{name}`")));
            }
        }
        Ok(Self {
            catalog,
            limits,
            selections,
            include_inactive,
            missing: BTreeMap::new(),
        })
    }

    fn load(
        &mut self,
        resource: &ResourceId,
        fragment: Option<&str>,
        from: &ResourceId,
        reference: &str,
    ) -> Result<Option<SourceOccurrence>, DtcgError> {
        let Some(json) = self.catalog.get(resource) else {
            self.missing
                .entry(resource.clone())
                .or_insert_with(|| DocumentRequest {
                    resource_id: resource.clone(),
                    referenced_from: from.clone(),
                    original_reference: reference.into(),
                });
            self.check_count(self.missing.len())?;
            return Ok(None);
        };
        let mut value = parse_document(resource, json)?;
        if let Some(fragment) = fragment {
            value = value
                .pointer(fragment)
                .cloned()
                .ok_or_else(|| DtcgError::InvalidPointer(fragment.into()))?;
        }
        Ok(Some(SourceOccurrence {
            resource: resource.clone(),
            value,
            origin: SourceOrigin {
                resource: Some(resource.clone()),
                pointer: fragment.unwrap_or_default().into(),
            },
            member_origins: BTreeMap::new(),
        }))
    }

    fn expand(
        mut self,
        sources: Vec<(SourceOccurrence, usize)>,
    ) -> Result<Resolution<Vec<SourceOccurrence>>, DtcgError> {
        self.check_count(sources.len())?;
        let mut queue = sources
            .into_iter()
            .rev()
            .map(|(source, depth)| (source, depth, Vec::new()))
            .collect::<Vec<_>>();
        let mut terminal = Vec::new();
        while let Some((source, depth, mut ancestors)) = queue.pop() {
            self.check_depth(depth)?;
            if !is_resolver(&source.value) {
                terminal.push(source);
                continue;
            }
            if ancestors.contains(&source.origin) {
                return Err(DtcgError::DocumentCycle(source.resource.to_string()));
            }
            let children = self.resolver_sources(&source, depth + 1)?;
            // Identity is tracked along this occurrence's ancestry only. Repeated
            // resolver sources must still participate at their authored position.
            ancestors.push(source.origin);
            for (child, child_depth) in children.into_iter().rev() {
                queue.push((child, child_depth, ancestors.clone()));
            }
            self.check_count(queue.len().saturating_add(terminal.len()))?;
        }
        if self.missing.is_empty() {
            Ok(Resolution::Ready(terminal))
        } else {
            Ok(Resolution::Missing(self.missing.into_values().collect()))
        }
    }

    fn resolver_sources(
        &mut self,
        source: &SourceOccurrence,
        depth: usize,
    ) -> Result<Vec<(SourceOccurrence, usize)>, DtcgError> {
        validate_resolver(source, &self.selections, self.include_inactive)?;
        let order = source.child("resolutionOrder")?;
        let mut result = Vec::new();
        let mut inline_names = BTreeSet::new();
        // validate_resolver checked the array before provenance traversal.
        for index in 0..order
            .value
            .as_array()
            .expect("validated resolution order")
            .len()
        {
            let step = order.child(&index.to_string())?;
            let (kind, name, definition) = resolution_step(source, &step)?;
            if step.value.get("$ref").is_none() && !inline_names.insert(name.clone()) {
                return Err(invalid(
                    &source.resource,
                    &format!("duplicate inline resolution name `{name}`"),
                ));
            }
            let documents = match kind.as_str() {
                "set" => set_documents(&definition)?,
                "modifier" => {
                    modifier_documents(&definition, &name, &self.selections, self.include_inactive)?
                }
                _ => {
                    return Err(invalid(
                        &source.resource,
                        "resolution step type must be `set` or `modifier`",
                    ));
                }
            };
            for document in documents {
                self.append_source(source, document, &mut result, &mut Vec::new(), depth)?;
            }
        }
        Ok(result)
    }

    fn append_source(
        &mut self,
        scope: &SourceOccurrence,
        source: SourceOccurrence,
        result: &mut Vec<(SourceOccurrence, usize)>,
        references: &mut Vec<String>,
        depth: usize,
    ) -> Result<(), DtcgError> {
        self.check_depth(depth)?;
        let object = source
            .value
            .as_object()
            .ok_or_else(|| invalid(&scope.resource, "document source must be an object"))?;
        let Some(reference) = object.get("$ref") else {
            result.push((source, depth));
            return self.check_count(result.len());
        };
        let reference = reference
            .as_str()
            .ok_or_else(|| invalid(&scope.resource, "reference object `$ref` must be a string"))?;
        if reference.starts_with("#/resolutionOrder/") || reference.starts_with("#/modifiers/") {
            return Err(invalid(
                &scope.resource,
                &format!("forbidden source reference `{reference}`"),
            ));
        }
        if reference.starts_with('#') {
            if references.iter().any(|active| active == reference) {
                return Err(invalid(
                    &scope.resource,
                    &format!("reference cycle at `{reference}`"),
                ));
            }
            references.push(reference.into());
            let parts = pointer_parts(reference)?;
            if let [kind, name] = parts.as_slice()
                && kind == "sets"
            {
                if scope
                    .value
                    .get("sets")
                    .and_then(|sets| sets.get(name))
                    .is_none()
                {
                    return Err(invalid(&scope.resource, &format!("unknown set `{name}`")));
                }
                let set = resolve_local_reference(scope, &source)?;
                for source in set_documents(&set)? {
                    self.append_source(scope, source, result, references, depth + 1)?;
                }
            } else {
                let resolved = resolve_local_reference(scope, &source)?;
                self.append_source(scope, resolved, result, references, depth + 1)?;
            }
            references.pop();
        } else {
            let from = source
                .member_origin("$ref")
                .resource
                .as_ref()
                .unwrap_or(&scope.resource);
            let target = from.resolve(reference)?;
            if let Some(loaded) =
                self.load(target.resource_id(), target.fragment(), from, reference)?
            {
                result.push((loaded.overlay(&source)?, depth));
                self.check_count(result.len())?;
            }
        }
        Ok(())
    }

    fn check_depth(&self, depth: usize) -> Result<(), DtcgError> {
        if depth > self.limits.max_reference_depth {
            Err(DtcgError::LimitExceeded {
                kind: "reference depth",
                limit: self.limits.max_reference_depth,
            })
        } else {
            Ok(())
        }
    }

    fn check_count(&self, count: usize) -> Result<(), DtcgError> {
        if count > self.limits.max_documents {
            Err(DtcgError::LimitExceeded {
                kind: "documents",
                limit: self.limits.max_documents,
            })
        } else {
            Ok(())
        }
    }
}

fn is_resolver(value: &Value) -> bool {
    value
        .as_object()
        .is_some_and(|object| object.contains_key("resolutionOrder"))
}

fn validate_resolver(
    source: &SourceOccurrence,
    selections: &BTreeMap<&str, &str>,
    include_inactive: bool,
) -> Result<(), DtcgError> {
    let parent = &source.resource;
    let object = source
        .value
        .as_object()
        .ok_or_else(|| invalid(parent, "expected resolver object"))?;
    if object.get("version").and_then(Value::as_str) != Some("2025.10") {
        return Err(invalid(parent, "resolver version must be `2025.10`"));
    }
    optional_object(parent, object, "sets")?;
    let modifiers = optional_object(parent, object, "modifiers")?;
    let order = object
        .get("resolutionOrder")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid(parent, "resolutionOrder is required"))?;
    validate_inputs(parent, modifiers, order, selections, include_inactive)
}
