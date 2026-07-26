//! String-backed DTCG document resolution and narrow typed projection.

mod catalog;
#[cfg(feature = "rayx_diagnostics")]
mod diagnostics;
mod error;
mod resolver;
mod resource;

use std::sync::Arc;

pub use dtcg_runtime_derive::{DtcgComponentTokens, DtcgTokens};
pub use error::DtcgError;
pub use resolver::{DocumentRequest, Resolution, ResolvedToken, ResolvedTokens, RuntimeLimits};
pub use resource::{DocumentReference, ResourceId};

pub use catalog::DocumentCatalog;
#[cfg(feature = "rayx_diagnostics")]
pub use diagnostics::{DtcgDiagnostic, DtcgDiagnosticCode};

pub trait FromDtcgValue: Sized + 'static {
    fn from_dtcg(token: &ResolvedToken) -> Result<Self, DtcgError>;
}

impl FromDtcgValue for String {
    fn from_dtcg(token: &ResolvedToken) -> Result<Self, DtcgError> {
        token
            .value
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| DtcgError::Projection {
                path: token.path.to_string(),
                message: "expected string value".into(),
            })
    }
}

impl FromDtcgValue for f64 {
    fn from_dtcg(token: &ResolvedToken) -> Result<Self, DtcgError> {
        token.value.as_f64().ok_or_else(|| DtcgError::Projection {
            path: token.path.to_string(),
            message: "expected numeric value".into(),
        })
    }
}

#[derive(Default)]
pub struct TypeRegistry;

impl TypeRegistry {
    pub fn convert<T: FromDtcgValue>(&self, token: &ResolvedToken) -> Result<T, DtcgError> {
        T::from_dtcg(token)
    }
}

pub trait DtcgTokens: Sized {
    fn project(tokens: &ResolvedTokens, registry: &TypeRegistry) -> Result<Self, DtcgError>;
}

pub trait DtcgComponentTokens: DtcgTokens {
    fn component_id() -> &'static str;
}

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
    /// let runtime = DtcgRuntime::from_json("tokens.json", r#"{"color":{"primary":{"$value":"blue"}}}"#)?;
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
