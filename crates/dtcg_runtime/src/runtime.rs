use std::sync::Arc;

use crate::{
    AffectedResolution, DocumentCatalog, DocumentRequest, DtcgError, DtcgTokens, Resolution,
    ResolvedTokens, ResourceId, RuntimeLimits, TypeRegistry, resolver,
};

pub struct DtcgRuntime {
    catalog: DocumentCatalog,
    root: Option<ResourceId>,
    limits: RuntimeLimits,
    registry: TypeRegistry,
}

impl Default for DtcgRuntime {
    fn default() -> Self {
        Self::with_limits(RuntimeLimits::default())
    }
}

impl DtcgRuntime {
    pub fn with_limits(limits: RuntimeLimits) -> Self {
        Self {
            catalog: DocumentCatalog::default(),
            root: None,
            limits,
            registry: TypeRegistry,
        }
    }

    /// Creates a runtime from an already-loaded JSON string.
    ///
    /// ```
    /// use dtcg_runtime::{DtcgRuntime, Resolution};
    ///
    /// let runtime = DtcgRuntime::from_json(
    ///     "tokens.json",
    ///     r#"{"spacing":{"small":{"$type":"number","$value":4}}}"#,
    /// )?;
    /// assert!(matches!(runtime.resolve(&[])?, Resolution::Ready(_)));
    /// # Ok::<(), dtcg_runtime::DtcgError>(())
    /// ```
    pub fn from_json(
        resource_id: impl AsRef<str>,
        json: impl Into<Arc<str>>,
    ) -> Result<Self, DtcgError> {
        let mut runtime = Self::default();
        runtime.provide_json(resource_id.as_ref(), json)?;
        runtime.root = Some(ResourceId::parse(resource_id)?);
        Ok(runtime)
    }

    pub fn provide_json(
        &mut self,
        resource_id: impl AsRef<str>,
        json: impl Into<Arc<str>>,
    ) -> Result<(), DtcgError> {
        self.catalog
            .provide(ResourceId::parse(resource_id)?, json.into())
    }

    pub fn provide_requested_json(
        &mut self,
        request: &DocumentRequest,
        json: impl Into<Arc<str>>,
    ) -> Result<(), DtcgError> {
        self.catalog
            .provide(request.resource_id.clone(), json.into())
    }

    pub fn provide_json_batch<I, Id, Json>(&mut self, documents: I) -> Result<(), DtcgError>
    where
        I: IntoIterator<Item = (Id, Json)>,
        Id: AsRef<str>,
        Json: Into<Arc<str>>,
    {
        let documents = documents
            .into_iter()
            .map(|(id, json)| Ok((ResourceId::parse(id)?, json.into())))
            .collect::<Result<Vec<_>, DtcgError>>()?;
        self.catalog.provide_batch(documents)
    }

    pub fn replace_json(
        &mut self,
        resource_id: impl AsRef<str>,
        json: impl Into<Arc<str>>,
    ) -> Result<(), DtcgError> {
        self.catalog
            .replace(ResourceId::parse(resource_id)?, json.into())
    }

    /// Replaces one loaded document and resolves only the changed tokens and
    /// their transitive dependents. The prior document is restored if the
    /// replacement cannot produce a ready affected projection.
    pub fn replace_json_and_resolve_affected(
        &mut self,
        resource_id: impl AsRef<str>,
        json: impl Into<Arc<str>>,
        selection: &[(&str, &str)],
        changed_paths: &[&str],
    ) -> Result<Resolution<AffectedResolution>, DtcgError> {
        let resource_id = ResourceId::parse(resource_id)?;
        let previous = self
            .catalog
            .get(&resource_id)
            .cloned()
            .ok_or_else(|| DtcgError::UnknownDocument(resource_id.clone()))?;
        self.catalog.replace(resource_id.clone(), json.into())?;

        let result = resolver::resolve_affected(
            &self.catalog,
            self.root.as_ref(),
            &self.limits,
            selection,
            changed_paths,
        );
        if matches!(&result, Ok(Resolution::Ready(_))) {
            return result;
        }

        self.catalog.replace(resource_id, previous)?;
        result
    }

    pub fn remove_document(&mut self, resource_id: impl AsRef<str>) -> Result<(), DtcgError> {
        self.catalog.remove(&ResourceId::parse(resource_id)?)
    }

    pub fn contains_document(&self, resource_id: impl AsRef<str>) -> Result<bool, DtcgError> {
        Ok(self.catalog.contains(&ResourceId::parse(resource_id)?))
    }

    pub fn catalog_revision(&self) -> u64 {
        self.catalog.revision()
    }

    pub fn set_root(&mut self, resource_id: impl AsRef<str>) -> Result<(), DtcgError> {
        self.root = Some(ResourceId::parse(resource_id)?);
        Ok(())
    }

    pub fn registry(&self) -> &TypeRegistry {
        &self.registry
    }

    pub fn resolve(
        &self,
        selection: &[(&str, &str)],
    ) -> Result<Resolution<ResolvedTokens>, DtcgError> {
        resolver::resolve(&self.catalog, self.root.as_ref(), &self.limits, selection)
    }

    pub fn validate_complete(&self) -> Result<Resolution<()>, DtcgError> {
        resolver::validate_complete(&self.catalog, self.root.as_ref(), &self.limits)
    }

    pub fn project<T: DtcgTokens>(
        &self,
        selection: &[(&str, &str)],
    ) -> Result<Resolution<T>, DtcgError> {
        match self.resolve(selection)? {
            Resolution::Ready(tokens) => T::project(&tokens, &self.registry).map(Resolution::Ready),
            Resolution::Missing(requests) => Ok(Resolution::Missing(requests)),
        }
    }
}
