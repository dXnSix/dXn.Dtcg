use std::collections::BTreeMap;

use serde_json::Value;

use crate::{DtcgError, ResourceId, error::invalid, resolver_definition::modifier_contexts};

/// One resolver input: a modifier with its contexts in authored order and its
/// optional default. Axes are listed in `resolutionOrder` order, followed by
/// the remaining root modifiers in authored order.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ModifierAxis {
    pub name: String,
    pub contexts: Vec<String>,
    pub default: Option<String>,
}

pub(crate) fn modifier_axes(
    resource: &ResourceId,
    resolver: &Value,
) -> Result<Vec<ModifierAxis>, DtcgError> {
    let Some(object) = resolver.as_object() else {
        return Ok(Vec::new());
    };
    let root = object
        .get("modifiers")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut axes = Vec::<ModifierAxis>::new();
    let mut push = |name: &str, definition: &Value| -> Result<(), DtcgError> {
        if axes.iter().any(|axis| axis.name == name) {
            return Ok(());
        }
        let contexts = modifier_contexts(resource, definition)?;
        axes.push(ModifierAxis {
            name: name.into(),
            contexts: contexts.keys().cloned().collect(),
            default: definition
                .get("default")
                .and_then(Value::as_str)
                .map(str::to_owned),
        });
        Ok(())
    };
    for step in object
        .get("resolutionOrder")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(step) = step.as_object() else {
            continue;
        };
        if let Some(name) = step
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(|pointer| pointer.strip_prefix("#/modifiers/"))
        {
            let name = name.replace("~1", "/").replace("~0", "~");
            let definition = root
                .get(&name)
                .ok_or_else(|| invalid(resource, &format!("unknown modifier `{name}`")))?;
            push(&name, definition)?;
        } else if step.get("type").and_then(Value::as_str) == Some("modifier")
            && let Some(name) = step.get("name").and_then(Value::as_str)
        {
            push(name, &Value::Object(step.clone()))?;
        }
    }
    for (name, definition) in &root {
        push(name, definition)?;
    }
    Ok(axes)
}

/// Completes and validates a selection against the axes: omitted axes take
/// their default, every named modifier and context must exist, and the result
/// is ordered by axis so equal selections compare equal.
pub(crate) fn normalize(
    axes: &[ModifierAxis],
    selection: &[(&str, &str)],
) -> Result<Vec<(String, String)>, DtcgError> {
    let mut chosen = BTreeMap::<&str, &str>::new();
    for (name, context) in selection {
        let Some(axis) = axes.iter().find(|axis| axis.name == *name) else {
            return Err(DtcgError::UnknownModifier {
                modifier: (*name).into(),
                known: axes.iter().map(|axis| axis.name.clone()).collect(),
            });
        };
        if chosen.insert(name, context).is_some() {
            return Err(context_error(axis, context, "modifier is selected twice"));
        }
    }
    axes.iter()
        .map(|axis| {
            let context = match chosen.get(axis.name.as_str()) {
                Some(context) => *context,
                None => axis.default.as_deref().ok_or_else(|| {
                    context_error(axis, "", "modifier has no default and was not selected")
                })?,
            };
            if !axis.contexts.iter().any(|allowed| allowed == context) {
                return Err(context_error(
                    axis,
                    context,
                    "selected modifier context is unknown",
                ));
            }
            Ok((axis.name.clone(), context.to_owned()))
        })
        .collect()
}

fn context_error(axis: &ModifierAxis, context: &str, message: &str) -> DtcgError {
    DtcgError::UnknownContext {
        modifier: axis.name.clone(),
        context: context.into(),
        allowed: axis.contexts.clone(),
        message: message.into(),
    }
}
