//! The single owner of resolver structure (Resolver §4): the allowed members,
//! the `resolutionOrder` and context-count rules, and the effective definition
//! of every `resolutionOrder` step. Discovery, input validation, `modifiers()`,
//! `normalize_selection` and source inspection all read resolvers through this
//! module, so they cannot disagree about a step.

use std::collections::BTreeSet;

use serde_json::{Map, Value};

use crate::{DtcgError, ResourceId, SourceOccurrence, error::invalid, json_pointer::pointer_parts};

const ROOT_MEMBERS: &[&str] = &[
    "name",
    "version",
    "description",
    "sets",
    "modifiers",
    "resolutionOrder",
    "$schema",
    "$defs",
];
const SET_MEMBERS: &[&str] = &["description", "sources", "$extensions"];
const MODIFIER_MEMBERS: &[&str] = &["description", "contexts", "default", "$extensions"];
const INLINE_SET_MEMBERS: &[&str] = &["type", "name", "description", "sources", "$extensions"];
const INLINE_MODIFIER_MEMBERS: &[&str] = &[
    "type",
    "name",
    "description",
    "contexts",
    "default",
    "$extensions",
];

/// Whether a document is a resolver: an object with `resolutionOrder`.
pub(crate) fn is_resolver(value: &Value) -> bool {
    value
        .as_object()
        .is_some_and(|object| object.contains_key("resolutionOrder"))
}

/// Validates a resolver document's structure: the version, the allowed
/// members of the root, sets, modifiers and inline steps, a non-empty
/// `resolutionOrder`, and modifier contexts. Reference objects are not
/// restricted, because their extra members are overrides.
pub(crate) fn validate_resolver(resource: &ResourceId, value: &Value) -> Result<(), DtcgError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid(resource, "expected resolver object"))?;
    if object.get("version").and_then(Value::as_str) != Some("2025.10") {
        return Err(invalid(resource, "resolver version must be `2025.10`"));
    }
    check_members(resource, object, ROOT_MEMBERS, "the resolver root")?;
    if let Some(sets) = optional_object(resource, object, "sets")? {
        for (name, set) in sets {
            let set_object = set
                .as_object()
                .ok_or_else(|| invalid(resource, "set must be an object"))?;
            check_members(resource, set_object, SET_MEMBERS, &format!("set `{name}`"))?;
            set_sources(resource, set)?;
        }
    }
    if let Some(modifiers) = optional_object(resource, object, "modifiers")? {
        for (name, modifier) in modifiers {
            let modifier_object = modifier
                .as_object()
                .ok_or_else(|| invalid(resource, "modifier must be an object"))?;
            check_members(
                resource,
                modifier_object,
                MODIFIER_MEMBERS,
                &format!("modifier `{name}`"),
            )?;
            modifier_contexts(resource, modifier)?;
        }
    }
    let order = object
        .get("resolutionOrder")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid(resource, "resolutionOrder is required"))?;
    if order.is_empty() {
        return Err(invalid(resource, "resolutionOrder must not be empty"));
    }
    for (index, step) in order.iter().enumerate() {
        let step = step
            .as_object()
            .ok_or_else(|| invalid(resource, "resolution step must be an object"))?;
        if let Some(reference) = step.get("$ref") {
            reference
                .as_str()
                .ok_or_else(|| invalid(resource, "reference object `$ref` must be a string"))?;
            continue;
        }
        let location = |kind: &str| match step.get("name").and_then(Value::as_str) {
            Some(name) => format!("inline {kind} step `{name}`"),
            None => format!("inline {kind} step {index}"),
        };
        match step.get("type").and_then(Value::as_str) {
            Some("set") => {
                check_members(resource, step, INLINE_SET_MEMBERS, &location("set"))?;
                set_sources(resource, &Value::Object(step.clone()))?;
            }
            Some("modifier") => {
                check_members(
                    resource,
                    step,
                    INLINE_MODIFIER_MEMBERS,
                    &location("modifier"),
                )?;
                modifier_contexts(resource, &Value::Object(step.clone()))?;
            }
            Some(_) => {
                return Err(invalid(
                    resource,
                    "resolution step type must be `set` or `modifier`",
                ));
            }
            None => return Err(invalid(resource, "inline resolution step requires `type`")),
        }
        if step
            .get("name")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            return Err(invalid(resource, "inline resolution step requires `name`"));
        }
    }
    Ok(())
}

fn check_members(
    resource: &ResourceId,
    object: &Map<String, Value>,
    allowed: &[&str],
    location: &str,
) -> Result<(), DtcgError> {
    match object.keys().find(|name| !allowed.contains(&name.as_str())) {
        Some(name) => Err(invalid(
            resource,
            &format!("unknown resolver member `{name}` in {location}"),
        )),
        None => Ok(()),
    }
}

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

/// Whether a `resolutionOrder` step contributes a set or a modifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StepKind {
    Set,
    Modifier,
}

/// One `resolutionOrder` step with its effective definition: a `$ref` step's
/// target with the step's sibling members applied as overrides, or the
/// inline step itself.
pub(crate) struct Step {
    pub(crate) kind: StepKind,
    pub(crate) name: String,
    pub(crate) definition: SourceOccurrence,
}

/// The effective steps of a validated resolver, in order. Inline steps must
/// have distinct names, and no two steps may define a modifier of the same
/// name.
pub(crate) fn effective_steps(document: &SourceOccurrence) -> Result<Vec<Step>, DtcgError> {
    let resource = &document.resource;
    let order = document.child("resolutionOrder")?;
    let length = order.value.as_array().map_or(0, Vec::len);
    let mut steps = Vec::with_capacity(length);
    let mut inline_names = BTreeSet::new();
    let mut modifier_names = BTreeSet::new();
    for index in 0..length {
        let step = order.child(&index.to_string())?;
        let inline = step.value.get("$ref").is_none();
        let resolved = resolution_step(document, &step)?;
        if inline && !inline_names.insert(resolved.name.clone()) {
            return Err(invalid(
                resource,
                &format!("duplicate inline resolution name `{}`", resolved.name),
            ));
        }
        if resolved.kind == StepKind::Modifier && !modifier_names.insert(resolved.name.clone()) {
            return Err(invalid(
                resource,
                &format!(
                    "resolutionOrder defines modifier `{}` more than once",
                    resolved.name
                ),
            ));
        }
        steps.push(resolved);
    }
    Ok(steps)
}

/// A modifier a resolver declares, with its effective definition.
pub(crate) struct DeclaredModifier {
    pub(crate) name: String,
    pub(crate) contexts: Vec<String>,
    pub(crate) default: Option<String>,
}

/// The modifiers a validated resolver declares: those its `resolutionOrder`
/// defines (with step overrides applied), then the remaining root modifiers
/// in authored order.
pub(crate) fn declared_modifiers(
    document: &SourceOccurrence,
    steps: &[Step],
) -> Result<Vec<DeclaredModifier>, DtcgError> {
    let resource = &document.resource;
    let mut modifiers = Vec::new();
    let mut declare = |name: &str, definition: &Value| -> Result<(), DtcgError> {
        let contexts = modifier_contexts(resource, definition)?;
        modifiers.push(DeclaredModifier {
            name: name.into(),
            contexts: contexts.keys().cloned().collect(),
            default: definition
                .get("default")
                .and_then(Value::as_str)
                .map(str::to_owned),
        });
        Ok(())
    };
    for step in steps.iter().filter(|step| step.kind == StepKind::Modifier) {
        declare(&step.name, &step.definition.value)?;
    }
    let defined = steps
        .iter()
        .filter(|step| step.kind == StepKind::Modifier)
        .map(|step| step.name.as_str())
        .collect::<BTreeSet<_>>();
    if let Some(root) = document.value.get("modifiers").and_then(Value::as_object) {
        for (name, definition) in root {
            if !defined.contains(name.as_str()) {
                declare(name, definition)?;
            }
        }
    }
    Ok(modifiers)
}

fn resolution_step(
    document: &SourceOccurrence,
    step: &SourceOccurrence,
) -> Result<Step, DtcgError> {
    let parent = &document.resource;
    let object = step
        .value
        .as_object()
        .ok_or_else(|| invalid(parent, "resolution step must be an object"))?;
    if let Some(pointer) = object.get("$ref").and_then(Value::as_str) {
        let parts = pointer_parts(pointer)?;
        let (kind, name) = match parts.as_slice() {
            [kind, name] if kind == "sets" => (StepKind::Set, name.clone()),
            [kind, name] if kind == "modifiers" => (StepKind::Modifier, name.clone()),
            _ => {
                return Err(invalid(
                    parent,
                    &format!("unknown resolver source `{pointer}`"),
                ));
            }
        };
        let definition = resolve_local_reference(document, step)?;
        match kind {
            StepKind::Set => {
                set_sources(parent, &definition.value)?;
            }
            StepKind::Modifier => {
                modifier_contexts(parent, &definition.value)?;
            }
        }
        return Ok(Step {
            kind,
            name,
            definition,
        });
    }
    let kind = match object.get("type").and_then(Value::as_str) {
        Some("set") => StepKind::Set,
        Some("modifier") => StepKind::Modifier,
        Some(_) => {
            return Err(invalid(
                parent,
                "resolution step type must be `set` or `modifier`",
            ));
        }
        None => return Err(invalid(parent, "inline resolution step requires `type`")),
    };
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| invalid(parent, "inline resolution step requires `name`"))?;
    Ok(Step {
        kind,
        name: name.into(),
        definition: step.clone(),
    })
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

/// Read a modifier's contexts and validate its optional default. A modifier
/// needs at least two contexts (Resolver §4.2), each an array of sources; a
/// default is optional in DTCG, and applications may require one as an
/// authoring policy.
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
    if contexts.len() == 1 {
        return Err(invalid(resource, "modifier requires at least two contexts"));
    }
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

/// The sources of every context of a modifier.
pub(crate) fn all_context_documents(
    modifier: &SourceOccurrence,
) -> Result<Vec<SourceOccurrence>, DtcgError> {
    let contexts = modifier_contexts(&modifier.resource, &modifier.value)?;
    let contexts_source = modifier.child("contexts")?;
    let mut documents = Vec::new();
    for name in contexts.keys() {
        documents.extend(array_members(&contexts_source.child(name)?)?);
    }
    Ok(documents)
}

/// The sources of one declared context of a modifier.
pub(crate) fn context_documents(
    modifier: &SourceOccurrence,
    context: &str,
) -> Result<Vec<SourceOccurrence>, DtcgError> {
    array_members(&modifier.child("contexts")?.child(context)?)
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
