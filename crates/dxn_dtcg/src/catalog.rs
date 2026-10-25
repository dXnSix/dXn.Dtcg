use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use serde_json::Value;

use crate::{DocumentKind, DtcgError, ResourceId};

/// One catalog entry: a JSON document or a module's source text.
#[derive(Clone)]
struct Entry {
    kind: DocumentKind,
    text: Arc<str>,
}

#[derive(Clone, Default)]
pub struct DocumentCatalog {
    documents: BTreeMap<ResourceId, Entry>,
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

    /// Stores `text` as a document of `kind`; JSON is validated. An entry of
    /// the other kind at `id` is a kind mismatch, and nothing changes.
    pub(crate) fn provide(
        &mut self,
        id: ResourceId,
        kind: DocumentKind,
        text: Arc<str>,
    ) -> Result<(), DtcgError> {
        self.check_kind(&id, kind)?;
        if kind == DocumentKind::Json {
            validate(&id, &text)?;
        }
        self.documents.insert(id, Entry { kind, text });
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
            self.check_kind(id, DocumentKind::Json)?;
            validate(id, json)?;
        }
        for (id, json) in documents {
            self.documents.insert(
                id,
                Entry {
                    kind: DocumentKind::Json,
                    text: json,
                },
            );
        }
        self.revision = self.revision.saturating_add(1);
        Ok(())
    }

    /// Replaces the document of `kind` at `id`, which must exist.
    pub(crate) fn replace(
        &mut self,
        id: ResourceId,
        kind: DocumentKind,
        text: Arc<str>,
    ) -> Result<(), DtcgError> {
        if !self.documents.contains_key(&id) {
            return Err(DtcgError::UnknownDocument(id));
        }
        self.provide(id, kind, text)
    }

    pub(crate) fn remove(&mut self, id: &ResourceId) -> Result<(), DtcgError> {
        if self.documents.remove(id).is_none() {
            return Err(DtcgError::UnknownDocument(id.clone()));
        }
        self.revision = self.revision.saturating_add(1);
        Ok(())
    }

    /// The JSON document at `id`.
    pub(crate) fn get(&self, id: &ResourceId) -> Option<&Arc<str>> {
        self.documents
            .get(id)
            .filter(|entry| entry.kind == DocumentKind::Json)
            .map(|entry| &entry.text)
    }

    /// The JSON document at `id`, failing when `id` holds a module.
    pub(crate) fn json(&self, id: &ResourceId) -> Result<Option<&Arc<str>>, DtcgError> {
        match self.documents.get(id) {
            Some(entry) if entry.kind == DocumentKind::Module => Err(DtcgError::InvalidDocument {
                resource: id.clone(),
                message: "the catalog holds it as a module, not a JSON document".into(),
            }),
            entry => Ok(entry.map(|entry| &entry.text)),
        }
    }

    /// The module source at `id`.
    #[cfg(feature = "expressions")]
    pub(crate) fn module(&self, id: &ResourceId) -> Option<&Arc<str>> {
        self.documents
            .get(id)
            .filter(|entry| entry.kind == DocumentKind::Module)
            .map(|entry| &entry.text)
    }

    /// The kind of the document at `id`.
    pub(crate) fn kind(&self, id: &ResourceId) -> Option<DocumentKind> {
        self.documents.get(id).map(|entry| entry.kind)
    }

    pub(crate) fn ids(&self) -> impl Iterator<Item = &ResourceId> {
        self.documents.keys()
    }
    pub(crate) fn contains(&self, id: &ResourceId) -> bool {
        self.documents.contains_key(id)
    }
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    fn check_kind(&self, id: &ResourceId, kind: DocumentKind) -> Result<(), DtcgError> {
        match self.kind(id) {
            Some(existing) if existing != kind => Err(DtcgError::InvalidDocument {
                resource: id.clone(),
                message: format!(
                    "the catalog holds it as {}, not {}",
                    existing.describe(),
                    kind.describe()
                ),
            }),
            _ => Ok(()),
        }
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
