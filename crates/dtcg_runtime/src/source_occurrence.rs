use std::collections::BTreeMap;

use serde_json::Value;

use crate::{DtcgError, ResourceId, SourceOrigin, error::invalid, json_pointer::pointer_parts};

/// One ordered terminal source, before token shadowing and group inheritance.
/// Its authored tree includes empty groups and shallow reference overrides.
#[derive(Clone, Debug)]
pub struct SourceOccurrence {
    pub(crate) resource: ResourceId,
    pub(crate) value: Value,
    pub(crate) origin: SourceOrigin,
    pub(crate) member_origins: BTreeMap<String, SourceOrigin>,
}

impl SourceOccurrence {
    /// The document that holds this source. An inline source supplied to
    /// `DtcgRuntime::inspect_source` reports the root resolver's id.
    pub fn resource(&self) -> &ResourceId {
        &self.resource
    }

    pub fn value(&self) -> &Value {
        &self.value
    }

    pub fn origin(&self) -> &SourceOrigin {
        &self.origin
    }

    /// Owners of overridden top-level members. Each location points to the
    /// object containing that member; unlisted members belong to `origin()`.
    pub fn member_origins(&self) -> &BTreeMap<String, SourceOrigin> {
        &self.member_origins
    }

    pub(crate) fn member_origin(&self, name: &str) -> &SourceOrigin {
        self.member_origins.get(name).unwrap_or(&self.origin)
    }

    pub(crate) fn child(&self, name: &str) -> Result<Self, DtcgError> {
        let value = match &self.value {
            Value::Array(values) => name
                .parse::<usize>()
                .ok()
                .and_then(|index| values.get(index)),
            Value::Object(values) => values.get(name),
            _ => None,
        }
        .ok_or_else(|| DtcgError::InvalidPointer(self.origin.child(name).pointer))?;
        Ok(Self {
            resource: self.resource.clone(),
            value: value.clone(),
            origin: self.member_origin(name).child(name),
            member_origins: BTreeMap::new(),
        })
    }

    pub(crate) fn select(&self, reference: &str) -> Result<Self, DtcgError> {
        let parts = pointer_parts(reference)?;
        let pointer = reference
            .strip_prefix('#')
            .expect("validated local pointer");
        let value = self
            .value
            .pointer(pointer)
            .cloned()
            .ok_or_else(|| DtcgError::InvalidPointer(pointer.into()))?;
        let mut origin = self.origin.clone();
        for (index, name) in parts.iter().enumerate() {
            if index == 0 {
                origin = self.member_origin(name).clone();
            }
            origin = origin.child(name);
        }
        Ok(Self {
            resource: self.resource.clone(),
            value,
            origin,
            member_origins: BTreeMap::new(),
        })
    }

    pub(crate) fn overlay(mut self, reference: &Self) -> Result<Self, DtcgError> {
        let target = self
            .value
            .as_object_mut()
            .ok_or_else(|| invalid(&self.resource, "reference target must be an object"))?;
        let object = reference
            .value
            .as_object()
            .ok_or_else(|| invalid(&reference.resource, "reference object must be an object"))?;
        for (key, value) in object {
            if key != "$ref" {
                target.insert(key.clone(), value.clone());
                self.member_origins
                    .insert(key.clone(), reference.member_origin(key).clone());
            }
        }
        Ok(self)
    }
}
