use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use serde_json::Value;

use crate::{DtcgError, ResourceId};

#[derive(Default)]
pub struct DocumentCatalog {
    documents: BTreeMap<ResourceId, Arc<str>>,
    revision: u64,
}

impl DocumentCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    pub fn len(&self) -> usize {
        self.documents.len()
    }

    pub(crate) fn provide(&mut self, id: ResourceId, json: Arc<str>) -> Result<(), DtcgError> {
        validate(&id, &json)?;
        self.documents.insert(id, json);
        self.revision = self.revision.saturating_add(1);
        Ok(())
    }

    pub(crate) fn provide_batch(
        &mut self,
        documents: Vec<(ResourceId, Arc<str>)>,
    ) -> Result<(), DtcgError> {
        let mut seen = BTreeSet::new();
        for (id, json) in &documents {
            if !seen.insert(id.clone()) {
                return Err(DtcgError::DuplicateDocument(id.clone()));
            }
            validate(id, json)?;
        }
        for (id, json) in documents {
            self.documents.insert(id, json);
        }
        self.revision = self.revision.saturating_add(1);
        Ok(())
    }

    pub(crate) fn replace(&mut self, id: ResourceId, json: Arc<str>) -> Result<(), DtcgError> {
        if !self.documents.contains_key(&id) {
            return Err(DtcgError::UnknownDocument(id));
        }
        self.provide(id, json)
    }

    pub(crate) fn remove(&mut self, id: &ResourceId) -> Result<(), DtcgError> {
        if self.documents.remove(id).is_none() {
            return Err(DtcgError::UnknownDocument(id.clone()));
        }
        self.revision = self.revision.saturating_add(1);
        Ok(())
    }

    pub(crate) fn get(&self, id: &ResourceId) -> Option<&Arc<str>> {
        self.documents.get(id)
    }
    pub(crate) fn contains(&self, id: &ResourceId) -> bool {
        self.documents.contains_key(id)
    }
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }
}

pub(crate) fn parse_document(id: &ResourceId, json: &str) -> Result<Value, DtcgError> {
    serde_json::from_str(json).map_err(|error| DtcgError::InvalidJson {
        resource: id.clone(),
        message: error.to_string(),
    })
}

fn validate(id: &ResourceId, json: &str) -> Result<(), DtcgError> {
    parse_document(id, json).map(|_| ())
}
