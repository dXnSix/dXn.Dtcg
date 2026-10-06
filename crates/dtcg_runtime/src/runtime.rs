use std::{fmt, sync::Arc};

use crate::{
    AffectedResolution, DocumentCatalog, DocumentRequest, DtcgError, DtcgTokens, ModifierAxis,
    Resolution, ResolvedTokens, ResolverEnvelope, ResourceId, RuntimeLimits, SourceOccurrence,
    TypeRegistry,
    catalog::parse_document,
    resolver, selection,
    source_discovery::{self, required_root},
};

#[derive(Clone)]
pub struct DtcgRuntime {
    catalog: DocumentCatalog,
    root: Option<ResourceId>,
    limits: RuntimeLimits,
    registry: TypeRegistry,
}

/// Prints the root, the limits, the catalog revision and the document ids,
/// never document bodies.
impl fmt::Debug for DtcgRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DtcgRuntime")
            .field("root", &self.root)
            .field("limits", &self.limits)
            .field("catalog_revision", &self.catalog.revision())
            .field("documents", &self.catalog.ids().collect::<Vec<_>>())
            .finish()
    }
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

    /// Creates a runtime whose root is the resolver inside a `.tokeneditor`
    /// envelope (see [`ResolverEnvelope`]); referenced documents resolve
    /// against the envelope's resource id.
    pub fn from_envelope(
        resource_id: impl AsRef<str>,
        json: &str,
    ) -> Result<(Self, ResolverEnvelope), DtcgError> {
        let resource = ResourceId::parse(resource_id.as_ref())?;
        let envelope = ResolverEnvelope::parse(&resource, json)?;
        let runtime = Self::from_json(resource.as_str(), envelope.resolver_json().clone())?;
        Ok((runtime, envelope))
    }

    /// Re-parses a `.tokeneditor` envelope with the same checks as
    /// [`DtcgRuntime::from_envelope`] and replaces the stored resolver of the
    /// already-provided document `resource_id`. Later resolutions use the new
    /// resolver. An invalid envelope leaves the catalog unchanged.
    pub fn replace_envelope(
        &mut self,
        resource_id: impl AsRef<str>,
        json: &str,
    ) -> Result<ResolverEnvelope, DtcgError> {
        let resource = ResourceId::parse(resource_id.as_ref())?;
        let envelope = ResolverEnvelope::parse(&resource, json)?;
        self.catalog
            .replace(resource, envelope.resolver_json().clone())?;
        Ok(envelope)
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

    /// The limits every resolution, discovery and inspection call applies.
    pub fn limits(&self) -> &RuntimeLimits {
        &self.limits
    }

    pub fn registry(&self) -> &TypeRegistry {
        &self.registry
    }

    /// The root resolver's modifiers with their contexts and defaults, in
    /// resolution order. A root that is a plain token document has no axes.
    pub fn modifiers(&self) -> Result<Resolution<Vec<ModifierAxis>>, DtcgError> {
        let root = required_root(self.root.as_ref())?;
        let Some(json) = self.catalog.get(root) else {
            return Ok(Resolution::Missing(vec![DocumentRequest {
                resource_id: root.clone(),
                referenced_from: root.clone(),
                original_reference: root.as_str().into(),
            }]));
        };
        selection::modifier_axes(root, &parse_document(root, json)?).map(Resolution::Ready)
    }

    /// Completes a selection with the modifier defaults and validates every
    /// named modifier and context; the result is ordered by axis, so equal
    /// selections normalize to equal vectors.
    pub fn normalize_selection(
        &self,
        selection: &[(&str, &str)],
    ) -> Result<Resolution<Vec<(String, String)>>, DtcgError> {
        match self.modifiers()? {
            Resolution::Ready(axes) => {
                selection::normalize(&axes, selection).map(Resolution::Ready)
            }
            Resolution::Missing(requests) => Ok(Resolution::Missing(requests)),
        }
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

    /// Inspect all authored resolver-source references, including dormant sets,
    /// contexts and shallow overrides. This does not resolve token values or
    /// inspect aliases/unknown extensions. Missing documents use the normal
    /// catalog acquisition protocol.
    pub fn source_references(&self) -> Result<Resolution<Vec<crate::SourceReference>>, DtcgError> {
        crate::source_references::inspect(
            &self.catalog,
            required_root(self.root.as_ref())?,
            &self.limits,
        )
    }

    /// Inspect a source in the root document's resolver context without resolving
    /// token values. Ordered duplicates, empty groups and override origins remain
    /// available. `None` origins belong to the supplied inline source.
    pub fn inspect_source(
        &self,
        source: &serde_json::Value,
        selection: &[(&str, &str)],
    ) -> Result<Resolution<Vec<SourceOccurrence>>, DtcgError> {
        source_discovery::discover_source(
            &self.catalog,
            required_root(self.root.as_ref())?,
            &self.limits,
            source,
            selection,
            false,
        )
    }

    /// Discover dependencies in every context of a supplied source. Like
    /// `validate_complete`, this validates structure, not resolved token values.
    pub fn validate_source(&self, source: &serde_json::Value) -> Result<Resolution<()>, DtcgError> {
        match source_discovery::discover_source(
            &self.catalog,
            required_root(self.root.as_ref())?,
            &self.limits,
            source,
            &[],
            true,
        )? {
            Resolution::Ready(_) => Ok(Resolution::Ready(())),
            Resolution::Missing(requests) => Ok(Resolution::Missing(requests)),
        }
    }

    /// Resolve with the supplied source's occurrences applied last. This uses the
    /// same discovery, ordering, alias, inheritance and limit rules as `resolve`;
    /// it does not alter catalog documents or create a synthetic resolver.
    pub fn preview_source(
        &self,
        source: &serde_json::Value,
        selection: &[(&str, &str)],
    ) -> Result<Resolution<ResolvedTokens>, DtcgError> {
        match self.inspect_source(source, selection)? {
            Resolution::Ready(sources) => self.preview_sources(&sources, selection),
            Resolution::Missing(requests) => Ok(Resolution::Missing(requests)),
        }
    }

    /// Preview one terminal occurrence returned by `inspect_source`, including a
    /// shadowed source within a referenced resolver. The occurrence is a detached
    /// authored snapshot; the surrounding context is resolved from this runtime.
    pub fn preview_occurrence(
        &self,
        source: &SourceOccurrence,
        selection: &[(&str, &str)],
    ) -> Result<Resolution<ResolvedTokens>, DtcgError> {
        self.preview_sources(std::slice::from_ref(source), selection)
    }

    fn preview_sources(
        &self,
        preview: &[SourceOccurrence],
        selection: &[(&str, &str)],
    ) -> Result<Resolution<ResolvedTokens>, DtcgError> {
        let mut sources = match source_discovery::discover(
            &self.catalog,
            required_root(self.root.as_ref())?,
            &self.limits,
            selection,
            false,
        )? {
            Resolution::Ready(sources) => sources,
            Resolution::Missing(requests) => return Ok(Resolution::Missing(requests)),
        };
        if sources.len().saturating_add(preview.len()) > self.limits.max_documents {
            return Err(DtcgError::LimitExceeded {
                kind: "documents",
                limit: self.limits.max_documents,
            });
        }
        sources.extend_from_slice(preview);
        resolver::resolve_sources(&sources, &self.limits).map(Resolution::Ready)
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
