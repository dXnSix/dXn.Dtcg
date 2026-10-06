use crate::{DtcgError, ResourceId, SourceOccurrence, error::invalid, json_pointer::pointer_parts};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub(crate) fn optional_object<'a>(
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

pub(crate) fn resolution_step(
    document: &SourceOccurrence,
    step: &SourceOccurrence,
) -> Result<(String, String, SourceOccurrence), DtcgError> {
    let parent = &document.resource;
    let object = step
        .value
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
        return Ok((kind.into(), name, resolve_local_reference(document, step)?));
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

pub(crate) fn resolve_local_reference(
    document: &SourceOccurrence,
    reference: &SourceOccurrence,
) -> Result<SourceOccurrence, DtcgError> {
    let pointer = reference
        .value
        .get("$ref")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            invalid(
                &document.resource,
                "reference object requires a string `$ref`",
            )
        })?;
    if !pointer.starts_with('#') {
        return Err(invalid(
            &document.resource,
            "resolver structure reference must be local",
        ));
    }
    if pointer_parts(pointer)?.first().map(String::as_str) == Some("resolutionOrder") {
        return Err(invalid(
            &document.resource,
            "references into resolutionOrder are forbidden",
        ));
    }
    document.select(pointer)?.overlay(reference)
}

/// Read a set's ordered source array, validating its DTCG shape without
/// traversing references or resolving token values.
pub fn set_sources<'a>(
    resource: &ResourceId,
    definition: &'a Value,
) -> Result<&'a [Value], DtcgError> {
    definition
        .get("sources")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| invalid(resource, "set sources are required"))
}

/// Read a modifier's contexts and validate its optional default. A default is
/// optional in DTCG; applications may require one as an authoring policy.
pub fn modifier_contexts<'a>(
    resource: &ResourceId,
    definition: &'a Value,
) -> Result<&'a Map<String, Value>, DtcgError> {
    let modifier = definition
        .as_object()
        .ok_or_else(|| invalid(resource, "modifier must be an object"))?;
    let contexts = modifier
        .get("contexts")
        .and_then(Value::as_object)
        .filter(|contexts| !contexts.is_empty())
        .ok_or_else(|| invalid(resource, "modifier requires contexts"))?;
    for sources in contexts.values() {
        if !sources.is_array() {
            return Err(invalid(resource, "modifier context must be an array"));
        }
    }
    if let Some(default) = modifier.get("default") {
        let default = default
            .as_str()
            .ok_or_else(|| invalid(resource, "modifier default must be a string"))?;
        if !contexts.contains_key(default) {
            return Err(invalid(resource, "modifier default context is unknown"));
        }
    }
    Ok(contexts)
}

pub(crate) fn set_documents(set: &SourceOccurrence) -> Result<Vec<SourceOccurrence>, DtcgError> {
    set_sources(&set.resource, &set.value)?;
    array_members(&set.child("sources")?)
}

pub(crate) fn modifier_documents(
    modifier: &SourceOccurrence,
    name: &str,
    selections: &BTreeMap<&str, &str>,
    include_inactive: bool,
) -> Result<Vec<SourceOccurrence>, DtcgError> {
    let parent = &modifier.resource;
    let contexts = modifier_contexts(parent, &modifier.value)?;
    let contexts_source = modifier.child("contexts")?;
    if include_inactive {
        let mut documents = Vec::new();
        for name in contexts.keys() {
            documents.extend(array_members(&contexts_source.child(name)?)?);
        }
        return Ok(documents);
    }
    let choice = selections
        .get(name)
        .copied()
        .or_else(|| modifier.value.get("default").and_then(Value::as_str))
        .ok_or_else(|| invalid(parent, "modifier has no selected or default context"))?;
    if !contexts.get(choice).is_some_and(Value::is_array) {
        return Err(DtcgError::Token {
            resource: parent.clone(),
            path: format!("#/modifiers/{name}"),
            message: "selected modifier context is unknown".into(),
        });
    }
    array_members(&contexts_source.child(choice)?)
}

fn array_members(source: &SourceOccurrence) -> Result<Vec<SourceOccurrence>, DtcgError> {
    let values = source
        .value
        .as_array()
        .ok_or_else(|| invalid(&source.resource, "modifier context must be an array"))?;
    (0..values.len())
        .map(|index| source.child(&index.to_string()))
        .collect()
}

pub(crate) fn validate_inputs(
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
        modifier_contexts(parent, modifier)?;
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
